use crate::errors::billing::{BillingError, WebhookError};
use crate::services::billing::extract_subscription;
use crate::services::{
    auth::extract_user_id,
    billing::{
        CreateCheckoutError, apply_scheduled_downgrade_if_due, apply_upgrade, cancel_with_creem,
        create_checkout, fetch_subscriber_email, handle_subscription_granted,
        handle_subscription_lost, handle_subscription_past_due, handle_subscription_update,
        verify_and_parse_webhook,
    },
    email::send_subscription_canceled_email,
};
use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Pool, Postgres, query, query_as, query_scalar};
use std::env::var;

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
/// to a subscription - trials starting, payments succeeding or failing,
/// cancellations - and it's what keeps your own database's picture of
/// who's paying, trialing, or lost access genuinely in sync with what's
/// real on Creem's side.
///
/// It first confirms the webhook is genuinely, verifiably from Creem
/// itself, then routes whichever event actually occurred to its own
/// specific handler. Once verification passes, nothing below it can fail
/// the whole request - a real problem handling one specific event gets
/// logged, not thrown back at Creem, since a failed response would just
/// cause endless retries of the same event.
pub async fn creem_webhook(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, WebhookError> {
    let event = verify_and_parse_webhook(&headers, &body).await?;

    match event.event_type.as_str() {
        "checkout.completed" => {
            eprintln!(
                "checkout.completed received for checkout {} - no action taken (subscription.paid handles granting access)",
                event.id
            );
        }
        "subscription.active" | "subscription.trialing" | "subscription.paid" => {
            if let Some(parsed) = extract_subscription(&event.event_type, &event.object) {
                handle_subscription_granted(&pool, &parsed).await;
            }
        }
        "subscription.past_due" => {
            if let Some(parsed) = extract_subscription(&event.event_type, &event.object) {
                handle_subscription_past_due(&pool, &parsed).await;
            }
        }
        "subscription.paused" | "subscription.expired" | "subscription.canceled" => {
            if let Some(parsed) = extract_subscription(&event.event_type, &event.object) {
                handle_subscription_lost(&pool, &parsed, &event.event_type).await;
            }
        }
        "subscription.update" => {
            if let Some(parsed) = extract_subscription(&event.event_type, &event.object) {
                handle_subscription_update(&pool, &parsed).await;
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

/// POST /api/v1/billing/cancel-subscription
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

/// GET /api/v1/billing/subscription-status
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
    )> = query_as(
        "SELECT creem_subscription_id, plan_name, status::text, current_period_end, scheduled_product_id, scheduled_plan_name
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

/// POST /api/v1/billing/change-plan
///
/// Switches an existing subscription to a different plan. It upgrades, apply
/// and charge immediately; downgrades are scheduled for when the current
/// paid period actually ends, matching standard Netflix/Spotify-style
/// downgrade behavior.
pub async fn change_plan_handler(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(body): Json<ChangePlanBody>,
) -> Result<Json<Value>, BillingError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| BillingError::InternalError("Failed to verify session".to_string()))?
        .ok_or(BillingError::Unauthorized)?;

    let current: Option<(String, String, String)> = query_as(
        "SELECT creem_subscription_id, plan_name, status::text FROM subscriptions
         WHERE user_id = $1 AND status IN ('active', 'trialing')
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .map_err(|_| BillingError::InternalError("Failed to look up subscription".to_string()))?;

    let (sub_id, current_plan, current_status) = current
        .ok_or_else(|| BillingError::NotFound("No active subscription found".to_string()))?;

    let is_upgrade = current_plan == "Team" && body.plan_name == "Enterprise";
    let is_downgrade = current_plan == "Enterprise" && body.plan_name == "Team";

    if current_status == "trialing" {
        return Err(BillingError::Conflict(
            "Plan changes aren't available during your trial".to_string(),
        ));
    }

    if !is_upgrade && !is_downgrade {
        return Err(BillingError::InvalidRequest(
            "Not a valid plan change".to_string(),
        ));
    }

    if is_upgrade {
        return apply_upgrade(&pool, &sub_id, &body).await;
    }

    query(
        "UPDATE subscriptions SET scheduled_product_id = $1, scheduled_plan_name = $2, updated_at = NOW()
         WHERE creem_subscription_id = $3",
    )
    .bind(&body.product_id)
    .bind(&body.plan_name)
    .bind(&sub_id)
    .execute(&pool)
    .await
    .map_err(|_| BillingError::InternalError("Failed to schedule downgrade".to_string()))?;

    Ok(Json(json!({ "applied": "scheduled" })))
}

/// GET /api/v1/billing/product-ids
///
/// It hands the frontend the real Creem product IDs for each plan, so
/// checkout requests always use the actual, correct ID - rather than the
/// frontend needing to hardcode them itself.
pub async fn get_product_ids() -> Result<Json<Value>, BillingError> {
    let team_id = var("CREEM_TEAM_PRODUCT_ID")
        .map_err(|_| BillingError::InternalError("CREEM_TEAM_PRODUCT_ID not set".to_string()))?;
    let enterprise_id = var("CREEM_ENTERPRISE_PRODUCT_ID").map_err(|_| {
        BillingError::InternalError("CREEM_ENTERPRISE_PRODUCT_ID not set".to_string())
    })?;

    Ok(Json(json!({
        "Team": team_id,
        "Enterprise": enterprise_id,
    })))
}
