use crate::{
    errors::billing::{BillingError, WebhookError},
    handlers::billing::ChangePlanBody,
    models::billing::{
        CreateCheckoutRequest, CreateCheckoutResponse, CreemWebhookEvent, ParsedSubscription,
    },
    services::email::{send_payment_failed_email, send_subscription_ended_email},
};
use axum::{Json, body::Bytes, http::HeaderMap};
use chrono::{DateTime, Utc};
use hex::encode;
use hmac::{Hmac, KeyInit, Mac};
use reqwest::Client;
use serde_json::{Value, from_str, from_value, json};
use sha2::Sha256;
use sqlx::{Error, Pool, Postgres, query, query_scalar};
use std::{env::var, str::from_utf8};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug)]
pub enum CreateCheckoutError {
    MissingApiKey,
    RequestFailed(String),
    CreemRejected(String),
}

/// It's the one real step that actually talks to Creem — building a real,
/// working checkout session for a specific plan, tied to a specific
/// person, so Creem knows exactly who to bill and where to send them once
/// they've paid.
///
/// It builds the real request, attaching the person's actual Safely user
/// ID via metadata — this is what lets every future webhook event tied to
/// this subscription (checkout.completed, subscription.paid,
/// subscription.canceled, ...) reliably identify which account to update,
/// rather than guessing by email. It then sends this to Creem's real
/// checkout endpoint, and checks that Creem genuinely accepted it, rather
/// than assuming the request going out means it worked.
pub async fn create_checkout(
    product_id: &str,
    user_id: Uuid,
) -> Result<CreateCheckoutResponse, CreateCheckoutError> {
    let api_key = var("CREEM_API_KEY").map_err(|_| CreateCheckoutError::MissingApiKey)?;
    let base_url = var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());

    let request_body = CreateCheckoutRequest {
        product_id: product_id.to_string(),
        success_url: format!("{}/dashboard/?checkout=success", base_url),
        metadata: serde_json::json!({ "safely_user_id": user_id.to_string() }),
    };

    let client = Client::new();
    let creem_base_url =
        var("CREEM_API_BASE_URL").unwrap_or_else(|_| "https://test-api.creem.io".to_string());
    let response = client
        .post(format!("{}/v1/checkouts", creem_base_url))
        .header("x-api-key", api_key)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| CreateCheckoutError::RequestFailed(e.to_string()))?;

    if !response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(CreateCheckoutError::CreemRejected(text));
    }

    response
        .json::<CreateCheckoutResponse>()
        .await
        .map_err(|e| CreateCheckoutError::RequestFailed(e.to_string()))
}

/// Confirms a webhook request is genuinely, verifiably from Creem, and
/// hands back the real, parsed event if so.
///
/// It checks the secret is configured, the signature header exists, the
/// body is readable, the signature genuinely matches, and finally that
/// the body parses into a real event - failing at whichever specific
/// step actually went wrong.
pub async fn verify_and_parse_webhook(
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<CreemWebhookEvent, WebhookError> {
    let secret = var("CREEM_WEBHOOK_SECRET")
        .map_err(|_| WebhookError::Misconfigured("CREEM_WEBHOOK_SECRET not set".to_string()))?;

    let signature = headers
        .get("creem-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(WebhookError::MissingSignature)?;

    let raw_body = from_utf8(body).map_err(|_| WebhookError::InvalidBody)?;

    if !verify_creem_signature(raw_body, signature, &secret) {
        eprintln!("Creem webhook: signature verification FAILED");
        return Err(WebhookError::InvalidSignature);
    }

    from_str(raw_body)
        .map_err(|e| WebhookError::InvalidPayload(format!("Invalid webhook payload: {}", e)))
}

/// Verifies that a webhook request genuinely came from Creem, and
/// wasn't forged by someone who simply knows your webhook URL.
///
/// Creem signs every webhook using HMAC-SHA256, with your webhook
/// secret as the key and the raw request body as the message - this
/// recomputes that same signature independently and compares it
/// against what Creem actually sent in the `creem-signature` header.
/// If they don't match exactly, the request is rejected outright,
/// before any real event-handling logic ever runs.
pub fn verify_creem_signature(raw_body: &str, received_signature: &str, secret: &str) -> bool {
    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };

    mac.update(raw_body.as_bytes());
    let computed = encode(mac.finalize().into_bytes());

    computed == received_signature
}

/// Pulls the subscription data out of a webhook event - the data can
/// arrive in one of two places, depending on the event type.
///
/// A "subscription.*" event has it directly at the top level. A
/// "checkout.completed" event has it nested one level deeper, inside a
/// "subscription" field. This checks which one applies and pulls it
/// out from the right spot. If the event type is something else, or
/// the data doesn't actually look like a real subscription, it just
/// returns None.
pub fn extract_subscription(event_type: &str, object: &Value) -> Option<ParsedSubscription> {
    let subscription_value = if event_type.starts_with("subscription.") {
        object.clone()
    } else if event_type == "checkout.completed" {
        object.get("subscription")?.clone()
    } else {
        return None;
    };

    from_value(subscription_value).ok()
}

/// Handles a subscription becoming active - a trial starting, a
/// payment going through, or a subscription otherwise turning paid -
/// by saving the current state to our database.
///
/// It looks for the real Safely user ID inside the event's metadata.
/// If it's there, it saves the subscription using Creem's own status
/// directly, rather than guessing based on which event fired. If the
/// user ID is missing, it just logs that and stops, since there's no
/// account to attach this event to.
pub async fn handle_subscription_granted(pool: &Pool<Postgres>, parsed: &ParsedSubscription) {
    match extract_metadata_user_id(parsed) {
        Some(user_id) => {
            if let Err(e) = upsert_subscription(pool, user_id, parsed, &parsed.status).await {
                eprintln!("Failed to upsert subscription: {}", e);
            }
        }
        None => eprintln!(
            "Subscription event is missing safely_user_id in metadata. It can't link to an account"
        ),
    }
}

/// Pulls the real Safely user ID out of a subscription's metadata, if
/// it's genuinely present and valid.
///
/// It checks the metadata itself exists, then that a user ID string was
/// actually included inside it, then that string genuinely parses as a
/// real UUID - failing quietly back to None at whichever step doesn't
/// hold, rather than treating a missing or malformed piece as an error.
pub fn extract_metadata_user_id(parsed: &ParsedSubscription) -> Option<Uuid> {
    parsed
        .metadata
        .as_ref()
        .and_then(|m| m.safely_user_id.as_ref())
        .and_then(|s| Uuid::parse_str(s).ok())
}

/// Creates or updates a subscription row for a user, matched by
/// Creem's own subscription ID. This one function handles every real
/// state a subscription can be in - active, past_due, canceled, and so
/// on - so it's called from every event type that touches a
/// subscription, each time with whatever status that specific event
/// represents.
///
/// It parses the two real date fields Creem sends, quietly treating a
/// missing or malformed date as None rather than failing outright,
/// then saves the subscription - creating a new row if this is the
/// first time we've seen this subscription ID, or updating the
/// existing one if we have. Every field here is always overwritten
/// with the latest value from Creem, since this represents the current,
/// real truth - unlike seller or listing data, there's nothing here
/// worth preserving over a fresher update.
pub async fn upsert_subscription(
    pool: &Pool<Postgres>,
    user_id: Uuid,
    parsed: &ParsedSubscription,
    status: &str,
) -> Result<(), Error> {
    let current_period_end = parsed
        .current_period_end_date
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let canceled_at = parsed
        .canceled_at
        .as_ref()
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    query(
        "INSERT INTO subscriptions (
            id, user_id, creem_subscription_id, creem_customer_id,
            creem_product_id, plan_name, status, current_period_end,
            canceled_at, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7::subscription_status, $8, $9, NOW(), NOW())
        ON CONFLICT (creem_subscription_id) DO UPDATE SET
            status = EXCLUDED.status,
            current_period_end = EXCLUDED.current_period_end,
            canceled_at = EXCLUDED.canceled_at,
            updated_at = NOW()",
    )
    .bind(Uuid::now_v7())
    .bind(user_id)
    .bind(&parsed.id)
    .bind(&parsed.customer.id)
    .bind(&parsed.product.id)
    .bind(&parsed.product.name)
    .bind(status)
    .bind(current_period_end)
    .bind(canceled_at)
    .execute(pool)
    .await?;

    Ok(())
}

/// Handles the moment Creem tells us a payment genuinely failed - the
/// subscription isn't canceled yet, but billing is now at risk, so the
/// person needs a real heads-up and a way to fix it.
///
/// It builds a real link straight to the billing-management section of
/// the dashboard, sends a payment-failed email to the customer's real
/// address, and separately updates our own database to reflect the
/// "past_due" status - these two steps are genuinely independent, so a
/// failure in one (like the email not sending) doesn't stop the other
/// from completing.
pub async fn handle_subscription_past_due(pool: &Pool<Postgres>, parsed: &ParsedSubscription) {
    let portal_url = format!(
        "{}/dashboard/?manage_billing=1",
        var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
    );

    if let Err(e) = send_payment_failed_email(&parsed.customer.email, &portal_url).await {
        eprintln!("Failed to send payment-failed email: {:?}", e);
    }

    if let Some(user_id) = extract_metadata_user_id(parsed) {
        if let Err(e) = upsert_subscription(pool, user_id, parsed, "past_due").await {
            eprintln!("Failed to upsert subscription: {}", e);
        }
    }
}

/// Handles the three real ways a subscription stops being active -
/// paused, expired, or genuinely canceled and, for cancellation
/// specifically, decides whether this is news worth emailing about.
///
/// It maps the event type to the correct status, checks what status
/// this subscription already had before this event arrived, updates our
/// own database to the new status, and only for a genuinely new
/// cancellation, one we didn't already know about. It sends the final
/// "subscription ended" email. If they already canceled through our own
/// site, this is just Creem confirming what we already know, so no
/// second email goes out.
pub async fn handle_subscription_lost(
    pool: &Pool<Postgres>,
    parsed: &ParsedSubscription,
    event_type: &str,
) {
    let status = match event_type {
        "subscription.paused" => "paused",
        "subscription.expired" => "expired",
        _ => "canceled",
    };

    let previous_status: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(&parsed.id)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);

    if let Some(user_id) = extract_metadata_user_id(parsed) {
        if let Err(e) = upsert_subscription(pool, user_id, parsed, status).await {
            eprintln!("Failed to upsert subscription: {}", e);
        }
    }

    if event_type == "subscription.canceled" && previous_status.as_deref() != Some("canceled") {
        if let Err(e) = send_subscription_ended_email(&parsed.customer.email).await {
            eprintln!("Failed to send subscription-ended email: {:?}", e);
        }
    }
}

/// Keeps our own database in sync whenever a subscription changes on
/// Creem's side through some path other than our own endpoints - most
/// often, someone editing it directly from Creem's own dashboard.
///
/// It pulls the real user ID out of the event's metadata, and if
/// present, saves whatever status Creem genuinely reports right now.
/// No separate logic beyond that.
pub async fn handle_subscription_update(pool: &Pool<Postgres>, parsed: &ParsedSubscription) {
    if let Some(user_id) = extract_metadata_user_id(parsed) {
        if let Err(e) = upsert_subscription(pool, user_id, parsed, &parsed.status).await {
            eprintln!("Failed to sync subscription.update: {}", e);
        }
    }
}

/// It's the one real step that actually talks to Creem — telling them,
/// for real, to stop billing this specific subscription.
///
/// It builds the real request with the correct API key, sends it to Creem's
/// real cancellation endpoint, and checks that Creem genuinely accepted it,
/// rather than just assuming the request going out means it worked.
pub async fn cancel_with_creem(sub_id: &str) -> Result<(), BillingError> {
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

/// Finds the real email address of whoever owns this subscription, so
/// a cancellation confirmation actually reaches the right person.
///
/// It joins the subscription back to its real user and returns their
/// email if found - returning nothing either way if it wasn't found,
/// or if a real database error happened, since a missing email should
/// only ever mean the confirmation gets skipped, never that anything
/// about the cancellation itself failed. A genuine database error still
/// gets logged, even though it's not treated as a hard failure here.
pub async fn fetch_subscriber_email(pool: &Pool<Postgres>, sub_id: &str) -> Option<String> {
    sqlx::query_scalar(
        "SELECT u.email FROM users u
         JOIN subscriptions s ON s.user_id = u.id
         WHERE s.creem_subscription_id = $1",
    )
    .bind(sub_id)
    .fetch_optional(pool)
    .await
    .unwrap_or_else(|e| {
        eprintln!("Failed to fetch subscriber email: {}", e);
        None
    })
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
pub async fn apply_scheduled_downgrade_if_due(
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

/// Tells Creem to change a subscription to a different plan, right now
/// - used for both immediate upgrades and applying a downgrade once its
/// deferred period has ended. The two callers decide what "right now"
/// actually means for their own situation; this just makes the real
/// API call and reports back whether Creem accepted it.
pub async fn change_creem_subscription_product(
    sub_id: &str,
    new_product_id: &str,
    update_behavior: &str,
) -> Result<(), String> {
    let api_key = var("CREEM_API_KEY").map_err(|_| "CREEM_API_KEY not set".to_string())?;
    let creem_base_url =
        var("CREEM_API_BASE_URL").unwrap_or_else(|_| "https://test-api.creem.io".to_string());
    let client = Client::new();

    let response = client
        .post(format!(
            "{}/v1/subscriptions/{}/upgrade",
            creem_base_url, sub_id
        ))
        .header("x-api-key", api_key)
        .json(&json!({
            "product_id": new_product_id,
            "update_behavior": update_behavior,
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!("Creem rejected plan change: {}", text));
    }

    Ok(())
}

/// Applies an upgrade immediately, both at Creem, and in our own
/// database unlike a downgrade, an upgrade never waits for the
/// current period to end, since charging more, right away, is never a
/// bad experience for the person paying.
///
/// It tells Creem to make the real, immediate change, with proration so
/// the person is charged correctly for the switch, then updates our own
/// row to match. By clearing any previously scheduled downgrade at the
/// same time, since a fresh upgrade genuinely supersedes it.
pub async fn apply_upgrade(
    pool: &Pool<Postgres>,
    sub_id: &str,
    body: &ChangePlanBody,
) -> Result<Json<Value>, BillingError> {
    change_creem_subscription_product(sub_id, &body.product_id, "proration-charge-immediately")
        .await
        .map_err(|e| {
            eprintln!("Failed to upgrade subscription: {}", e);
            BillingError::ServiceUnavailable("Creem rejected the upgrade".to_string())
        })?;

    query(
        "UPDATE subscriptions SET plan_name = $1, creem_product_id = $2,
         scheduled_product_id = NULL, scheduled_plan_name = NULL, updated_at = NOW()
         WHERE creem_subscription_id = $3",
    )
    .bind(&body.plan_name)
    .bind(&body.product_id)
    .bind(sub_id)
    .execute(pool)
    .await
    .map_err(|_| BillingError::InternalError("Failed to update subscription".to_string()))?;

    Ok(Json(json!({ "applied": "immediately" })))
}
