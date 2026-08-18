use crate::errors::billing::{BillingError, WebhookError};
use crate::models::billing::{CreemWebhookEvent, extract_subscription};
use crate::services::billing::change_creem_subscription_product;
use crate::services::email::send_subscription_canceled_email;
use crate::services::{
    auth::extract_user_id,
    billing::{CreateCheckoutError, create_checkout, upsert_subscription, verify_creem_signature},
    email::{send_payment_failed_email, send_subscription_ended_email},
};
use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Pool, Postgres, query, query_scalar};
use std::env::var;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateCheckoutBody {
    pub product_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangePlanBody {
    pub product_id: String,
    pub plan_name: String,
}

/// POST /api/v1/billing/checkout
///
/// It's the real endpoint your frontend calls when someone wants to start a
/// subscription. It confirms who's asking, then hands off to Creem to
/// actually build a real, ready-to-pay checkout link for that specific plan.
///
/// It confirms who's genuinely signed in, right now, sends the chosen plan
/// and the real user's ID to Creem, to build the checkout, and if anything
/// went wrong along the way like an unrecognized user, a broken connection to
/// Creem, or Creem itself refusing the request, it reports back specifically
/// which of those actually happened, instead of a single, generic failure.
/// Otherwise, it hands back the real, working checkout link.
pub async fn create_checkout_handler(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(body): Json<CreateCheckoutBody>,
) -> Result<Json<Value>, BillingError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| BillingError::InternalError("Failed to verify session".to_string()))?
        .ok_or(BillingError::Unauthorized)?;

    let checkout = create_checkout(&body.product_id, user_id)
        .await
        .map_err(|e| match e {
            CreateCheckoutError::MissingApiKey => {
                eprintln!("CREEM_API_KEY not set");
                BillingError::InternalError("Checkout is temporarily unavailable".to_string())
            }
            CreateCheckoutError::RequestFailed(msg) => {
                eprintln!("Creem checkout request failed: {}", msg);
                BillingError::ServiceUnavailable("Checkout provider is unreachable".to_string())
            }
            CreateCheckoutError::CreemRejected(msg) => {
                eprintln!("Creem rejected checkout: {}", msg);
                BillingError::InvalidRequest(format!("Checkout request rejected: {}", msg))
            }
        })?;

    Ok(Json(json!({ "checkout_url": checkout.checkout_url })))
}

/// POST /api/v1/webhooks/creem
///
/// It's the real, external endpoint Creem calls whenever something happens
/// to a subscription like trials starting, payments succeeding or failing,
/// cancellations and it's what keeps your own database of who's
/// paying, trialing, or lost access genuinely in sync with what's real on
/// Creem's side.
///
/// It first confirms the webhook is genuinely, verifiably from Creem itself,
/// via a real signature check and then processes whichever event actually
/// occurred. Once the verification passes, nothing below it can fail the whole
/// request; a real problem handling one specific event gets logged, not
/// thrown back at Creem, since a failed response would just cause endless
/// retries of the same event, even if the real cause were a bug on our own
/// side.
pub async fn creem_webhook(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, WebhookError> {
    let secret = var("CREEM_WEBHOOK_SECRET")
        .map_err(|_| WebhookError::Misconfigured("CREEM_WEBHOOK_SECRET not set".to_string()))?;

    let signature = headers
        .get("creem-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(WebhookError::MissingSignature)?;

    let raw_body = std::str::from_utf8(&body).map_err(|_| WebhookError::InvalidBody)?;

    if !verify_creem_signature(raw_body, signature, &secret) {
        eprintln!("Creem webhook: signature verification FAILED");
        return Err(WebhookError::InvalidSignature);
    }

    let event: CreemWebhookEvent = serde_json::from_str(raw_body)
        .map_err(|e| WebhookError::InvalidPayload(format!("Invalid webhook payload: {}", e)))?;

    match event.event_type.as_str() {
        "checkout.completed" => {
            eprintln!(
                "checkout.completed received for checkout {} - no action taken (subscription.paid handles granting access)",
                event.id
            );
        }

        "subscription.active" | "subscription.trialing" | "subscription.paid" => {
            if let Some(parsed) = extract_subscription(&event.event_type, &event.object) {
                if let Some(user_id) = parsed
                    .metadata
                    .as_ref()
                    .and_then(|m| m.safely_user_id.as_ref())
                    .and_then(|s| Uuid::parse_str(s).ok())
                {
                    // Use Creem's own real status field directly, rather than
                    // guessing from which event name fired.
                    if let Err(e) =
                        upsert_subscription(&pool, user_id, &parsed, &parsed.status).await
                    {
                        eprintln!("Failed to upsert subscription: {}", e);
                    }
                } else {
                    eprintln!(
                        "subscription event missing safely_user_id in metadata - cannot link to an account"
                    );
                }
            }
        }

        "subscription.past_due" => {
            if let Some(parsed) = extract_subscription(&event.event_type, &event.object) {
                let portal_url = format!(
                    "{}/dashboard/?manage_billing=1",
                    var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
                );
                if let Err(e) = send_payment_failed_email(&parsed.customer.email, &portal_url).await
                {
                    eprintln!("Failed to send payment-failed email: {:?}", e);
                }
                if let Some(user_id) = parsed
                    .metadata
                    .as_ref()
                    .and_then(|m| m.safely_user_id.as_ref())
                    .and_then(|s| Uuid::parse_str(s).ok())
                {
                    if let Err(e) = upsert_subscription(&pool, user_id, &parsed, "past_due").await {
                        eprintln!("Failed to upsert subscription: {}", e);
                    }
                }
            }
        }

        "subscription.paused" | "subscription.expired" | "subscription.canceled" => {
            if let Some(parsed) = extract_subscription(&event.event_type, &event.object) {
                let status = match event.event_type.as_str() {
                    "subscription.paused" => "paused",
                    "subscription.expired" => "expired",
                    _ => "canceled",
                };

                // If they already canceled through our the site, don't
                // send another email. Instead Creem's just confirming what we
                // already know. Only email them if this is the moment
                // Creem gave up trying to charge them.
                let previous_status: Option<String> = sqlx::query_scalar(
                    "SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1",
                )
                .bind(&parsed.id)
                .fetch_optional(&pool)
                .await
                .unwrap_or(None);

                if let Some(user_id) = parsed
                    .metadata
                    .as_ref()
                    .and_then(|m| m.safely_user_id.as_ref())
                    .and_then(|s| Uuid::parse_str(s).ok())
                {
                    if let Err(e) = upsert_subscription(&pool, user_id, &parsed, status).await {
                        eprintln!("Failed to upsert subscription: {}", e);
                    }
                }

                if event.event_type == "subscription.canceled"
                    && previous_status.as_deref() != Some("canceled")
                {
                    if let Err(e) = send_subscription_ended_email(&parsed.customer.email).await {
                        eprintln!("Failed to send subscription-ended email: {:?}", e);
                    }
                }
            }
        }

        "subscription.update" => {
            // In case someone changes a subscription directly in Creem's
            // dashboard instead of through our site - this keeps our
            // database matching what's actually true.
            if let Some(parsed) = extract_subscription(&event.event_type, &event.object) {
                if let Some(user_id) = parsed
                    .metadata
                    .as_ref()
                    .and_then(|m| m.safely_user_id.as_ref())
                    .and_then(|s| Uuid::parse_str(s).ok())
                {
                    if let Err(e) =
                        upsert_subscription(&pool, user_id, &parsed, &parsed.status).await
                    {
                        eprintln!("Failed to sync subscription.update: {}", e);
                    }
                }
            }
        }

        "refund.created" => {
            eprintln!("refund.created received for event {}", event.id);
        }

        other => {
            eprintln!("Unhandled Creem event type: {}", other);
        }
    }

    Ok(StatusCode::OK)
}

/// POST /api/v1/billing/cancel
///
/// It cancels the person's active subscription by telling Creem to actually
/// stop billing them, then immediately updating our own database to match,
/// rather than waiting for Creem's webhook to confirm something we already
/// know for certain, since we're the ones who just caused it.
///
/// It confirms who's genuinely signed in, finds their real, currently
/// active subscription, tells Creem to cancel it, updates our own database
/// right away to reflect that, and tries to send a real confirmation email
/// although a failed email doesn't undo the cancellation itself, since
/// that part has already genuinely succeeded by then.
pub async fn cancel_subscription_handler(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<Value>, BillingError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| BillingError::InternalError("Failed to verify session".to_string()))?
        .ok_or(BillingError::Unauthorized)?;

    let sub_id: Option<String> = query_scalar(
        "SELECT creem_subscription_id FROM subscriptions
         WHERE user_id = $1 AND status IN ('active', 'trialing', 'past_due')
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .map_err(|_| BillingError::InternalError("Failed to look up subscription".to_string()))?;

    let sub_id =
        sub_id.ok_or_else(|| BillingError::NotFound("No active subscription found".to_string()))?;

    cancel_with_creem(&sub_id).await?;

    query(
        "UPDATE subscriptions SET status = 'canceled'::subscription_status, canceled_at = NOW(), updated_at = NOW()
         WHERE creem_subscription_id = $1",
    )
    .bind(&sub_id)
    .execute(&pool)
    .await
    .map_err(|_| BillingError::InternalError("Failed to update subscription".to_string()))?;

    if let Some(email) = fetch_subscriber_email(&pool, &sub_id).await {
        if let Err(e) = send_subscription_canceled_email(&email).await {
            eprintln!("Failed to send cancellation confirmation email: {:?}", e);
        }
    }

    Ok(Json(json!({ "success": true })))
}

/// It's the one real step that actually talks to Creem — telling them,
/// for real, to stop billing this specific subscription.
///
/// It builds the real request with the correct API key, sends it to Creem's
/// real cancellation endpoint, and checks that Creem genuinely accepted it,
/// rather than just assuming the request going out means it worked.
async fn cancel_with_creem(sub_id: &str) -> Result<(), BillingError> {
    let api_key = var("CREEM_API_KEY")
        .map_err(|_| BillingError::InternalError("CREEM_API_KEY not set".to_string()))?;
    let creem_base_url =
        var("CREEM_API_BASE_URL").unwrap_or_else(|_| "https://test-api.creem.io".to_string());

    let response = Client::new()
        .post(format!(
            "{}/v1/subscriptions/{}/cancel",
            creem_base_url, sub_id
        ))
        .header("x-api-key", api_key)
        .send()
        .await
        .map_err(|_| BillingError::ServiceUnavailable("Could not reach Creem".to_string()))?;

    if !response.status().is_success() {
        return Err(BillingError::ServiceUnavailable(
            "Creem rejected the cancellation".to_string(),
        ));
    }

    Ok(())
}

/// It finds the real email address of whoever owns this subscription, so
/// the cancellation confirmation actually reaches the right person.
///
/// It joins the subscription back to its real user and returns their
/// email if found — quietly returning nothing if it wasn't, since a
/// missing email should only mean the confirmation gets skipped, not that
/// anything about the cancellation itself failed.
async fn fetch_subscriber_email(pool: &Pool<Postgres>, sub_id: &str) -> Option<String> {
    sqlx::query_scalar(
        "SELECT u.email FROM users u
         JOIN subscriptions s ON s.user_id = u.id
         WHERE s.creem_subscription_id = $1",
    )
    .bind(sub_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
}

/// GET /api/v1/billing/status
///
/// It reports back the person's real, current subscription state — which
/// plan they're on, what status it's in, and when it renews so the
/// frontend can show accurate billing info without guessing.
///
/// It confirms who's genuinely signed in, looks up their most recent real
/// subscription, and if a scheduled downgrade's deferred period has
/// genuinely ended by now, actually applies it at this exact moment,
/// before reporting back the final, up-to-date picture.
pub async fn get_subscription_status(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<Value>, BillingError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| BillingError::InternalError("Failed to verify session".to_string()))?
        .ok_or(BillingError::Unauthorized)?;

    let row: Option<(
        String,
        String,
        String,
        Option<DateTime<Utc>>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT creem_subscription_id, plan_name, status::text, current_period_end,
                scheduled_product_id, scheduled_plan_name
         FROM subscriptions WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .map_err(|_| BillingError::InternalError("Failed to look up subscription".to_string()))?;

    let Some((
        sub_id,
        mut plan_name,
        status,
        current_period_end,
        scheduled_product_id,
        scheduled_plan_name,
    )) = row
    else {
        return Ok(Json(json!({
            "plan_name": null, "status": null, "current_period_end": null, "scheduled_plan_name": null,
        })));
    };

    let mut scheduled_plan_name_response = scheduled_plan_name.clone();
    if let Some(new_plan_name) = apply_scheduled_downgrade_if_due(
        &pool,
        &sub_id,
        scheduled_product_id.as_deref(),
        scheduled_plan_name.as_deref(),
        current_period_end,
    )
    .await
    {
        plan_name = new_plan_name;
        scheduled_plan_name_response = None;
    }

    Ok(Json(json!({
        "plan_name": plan_name,
        "status": status,
        "current_period_end": current_period_end,
        "scheduled_plan_name": scheduled_plan_name_response,
    })))
}

/// If a scheduled downgrade's deferred period has genuinely ended by now,
/// this actually applies it - telling Creem, then updating our own
/// database - and hands back the new plan name if it did.
///
/// It checks all three pieces are genuinely present, and that the
/// deferred period has actually passed. If so, tells Creem to make the
/// change real, then updates our own row to match by quietly logging,
/// not failing, if either step goes wrong, since a delayed downgrade
/// isn't worth breaking someone's whole status check over.
async fn apply_scheduled_downgrade_if_due(
    pool: &Pool<Postgres>,
    sub_id: &str,
    scheduled_product_id: Option<&str>,
    scheduled_plan_name: Option<&str>,
    current_period_end: Option<DateTime<Utc>>,
) -> Option<String> {
    let (sched_product_id, sched_plan_name, period_end) = (
        scheduled_product_id?,
        scheduled_plan_name?,
        current_period_end?,
    );

    if Utc::now() < period_end {
        return None;
    }

    if let Err(e) =
        change_creem_subscription_product(sub_id, sched_product_id, "proration-charge-immediately")
            .await
    {
        eprintln!("Failed to apply scheduled downgrade: {}", e);
        return None;
    }

    let _ = query(
        "UPDATE subscriptions SET plan_name = $1, creem_product_id = $2,
         scheduled_product_id = NULL, scheduled_plan_name = NULL, updated_at = NOW()
         WHERE creem_subscription_id = $3",
    )
    .bind(sched_plan_name)
    .bind(sched_product_id)
    .bind(sub_id)
    .execute(pool)
    .await;

    Some(sched_plan_name.to_string())
}

/// POST /api/v1/billing/change-plan
///
/// Handles switching an EXISTING active subscription to a different
/// plan - genuinely different from create_checkout_handler, which is
/// only for a brand-new signup with no subscription at all yet.
///
/// Upgrade (moving to a more expensive plan): applied immediately,
/// charged immediately via Creem's real upgrade endpoint - matches
/// standard SaaS practice of never blocking someone who wants to pay
/// you more for more value.
///
/// Downgrade (moving to a cheaper plan): NOT applied immediately.
/// Creem has no native "schedule for later" primitive, so this is
/// recorded in our own database instead, and only actually applied
/// once get_subscription_status later notices the current period has
/// genuinely ended - the person keeps their already-paid-for plan's
/// full features until then, exactly like Netflix/Spotify-style
/// downgrades work.
pub async fn change_plan_handler(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(body): Json<ChangePlanBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .expect("expected to extract the user id")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let current: Option<(String, String, String)> = sqlx::query_as(
        "SELECT creem_subscription_id, plan_name, status::text FROM subscriptions
         WHERE user_id = $1 AND status IN ('active', 'trialing')
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (sub_id, current_plan, current_status) = current.ok_or(StatusCode::NOT_FOUND)?;
    let is_trialing = current_status == "trialing";

    // Only two tiers exist right now, so this simple check correctly
    // identifies which direction the change is - revisit this if a
    // third tier is ever added, since it would need real price-based
    // ranking instead of a plain name comparison.
    let is_upgrade = current_plan == "Team" && body.plan_name == "Enterprise";
    let is_downgrade = current_plan == "Enterprise" && body.plan_name == "Team";

    if is_trialing {
        // Creem's own /upgrade endpoint explicitly requires "active"
        // status before allowing any modification - confirmed directly
        // via a real rejected request. Rather than fight this, we're
        // honest about the limitation: switching plans isn't available
        // until the trial genuinely converts to a paid subscription.
        return Err(StatusCode::CONFLICT);
    }

    if !is_upgrade && !is_downgrade {
        return Err(StatusCode::BAD_REQUEST);
    }

    if is_upgrade {
        crate::services::billing::change_creem_subscription_product(
            &sub_id,
            &body.product_id,
            "proration-charge-immediately",
        )
        .await
        .map_err(|e| {
            eprintln!("Failed to upgrade subscription: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        sqlx::query(
            "UPDATE subscriptions SET plan_name = $1, creem_product_id = $2,
             scheduled_product_id = NULL, scheduled_plan_name = NULL, updated_at = NOW()
             WHERE creem_subscription_id = $3",
        )
        .bind(&body.plan_name)
        .bind(&body.product_id)
        .bind(&sub_id)
        .execute(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        return Ok(Json(serde_json::json!({ "applied": "immediately" })));
    }

    // Downgrade - just record the intent, don't touch Creem or the
    // real plan yet.
    sqlx::query(
        "UPDATE subscriptions SET scheduled_product_id = $1, scheduled_plan_name = $2, updated_at = NOW()
         WHERE creem_subscription_id = $3",
    )
    .bind(&body.product_id)
    .bind(&body.plan_name)
    .bind(&sub_id)
    .execute(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "applied": "scheduled" })))
}

pub async fn get_product_ids() -> Json<serde_json::Value> {
    let team_id = std::env::var("CREEM_TEAM_PRODUCT_ID").unwrap_or_default();
    let enterprise_id = std::env::var("CREEM_ENTERPRISE_PRODUCT_ID").unwrap_or_default();
    Json(serde_json::json!({
        "Team": team_id,
        "Enterprise": enterprise_id,
    }))
}
