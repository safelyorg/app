use crate::models::billing::{CreemWebhookEvent, extract_subscription};
use crate::services::auth::extract_user_id;
use crate::services::billing::{
    CreateCheckoutError, create_checkout, upsert_subscription, verify_creem_signature,
};
use crate::services::email::{send_payment_failed_email, send_subscription_ended_email};
use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use sqlx::{Pool, Postgres};
use std::env::var;
use uuid::Uuid;

#[derive(Debug, serde::Deserialize)]
pub struct CreateCheckoutBody {
    pub product_id: String,
}

/// POST /api/v1/billing/checkout
pub async fn create_checkout_handler(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(body): Json<CreateCheckoutBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let checkout = create_checkout(&body.product_id, user_id)
        .await
        .map_err(|e| {
            match e {
                CreateCheckoutError::MissingApiKey => eprintln!("CREEM_API_KEY not set"),
                CreateCheckoutError::RequestFailed(msg) => {
                    eprintln!("Creem checkout request failed: {}", msg)
                }
                CreateCheckoutError::CreemRejected(msg) => {
                    eprintln!("Creem rejected checkout: {}", msg)
                }
            }
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(
        serde_json::json!({ "checkout_url": checkout.checkout_url }),
    ))
}

/// POST /api/v1/webhooks/creem
///
/// Real event handling, per the design we worked through together:
/// - checkout.completed: logged only, never acted on (fires once per
///   initial signup; every real lifecycle change comes through the
///   subscription.* events instead)
/// - subscription.active / .trialing / .paid: grant/maintain access
/// - subscription.past_due: send ONE warning email, access unchanged
///   (Creem is still silently retrying in the background)
/// - subscription.paused / .expired / .canceled: revoke access. Only
///   .canceled also sends the final "subscription ended" email, since
///   that's the one genuinely final state - paused/expired can still
///   potentially recover.
pub async fn creem_webhook(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    let secret = var("CREEM_WEBHOOK_SECRET").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let signature = headers
        .get("creem-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let raw_body = std::str::from_utf8(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    if !verify_creem_signature(raw_body, signature, &secret) {
        eprintln!("Creem webhook: signature verification FAILED");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let event: CreemWebhookEvent =
        serde_json::from_str(raw_body).map_err(|_| StatusCode::BAD_REQUEST)?;

    eprintln!("Creem webhook: {} received", event.event_type);

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
                    let status = match event.event_type.as_str() {
                        "subscription.trialing" => "trialing",
                        _ => "active",
                    };
                    if let Err(e) = upsert_subscription(&pool, user_id, &parsed, status).await {
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
                if event.event_type == "subscription.canceled" {
                    if let Err(e) = send_subscription_ended_email(&parsed.customer.email).await {
                        eprintln!("Failed to send subscription-ended email: {:?}", e);
                    }
                }
            }
        }

        other => {
            eprintln!("Unhandled Creem event type: {}", other);
        }
    }

    Ok(StatusCode::OK)
}

pub async fn cancel_subscription_handler(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let sub_id: Option<String> = sqlx::query_scalar(
        "SELECT creem_subscription_id FROM subscriptions
         WHERE user_id = $1 AND status IN ('active', 'trialing', 'past_due')
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let sub_id = sub_id.ok_or(StatusCode::NOT_FOUND)?;

    let api_key = std::env::var("CREEM_API_KEY").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let client = reqwest::Client::new();
    let creem_base_url = std::env::var("CREEM_API_BASE_URL")
        .unwrap_or_else(|_| "https://test-api.creem.io".to_string());
    let response = client
        .post(format!(
            "{}/v1/subscriptions/{}/cancel",
            creem_base_url, sub_id
        ))
        .header("x-api-key", api_key)
        .send()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !response.status().is_success() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // Update our own database directly, right now, in this same request -
    // we don't need to wait for the async webhook to tell us something we
    // already know for certain, since we're the ones who just caused it.
    sqlx::query(
        "UPDATE subscriptions SET status = 'canceled'::subscription_status, canceled_at = NOW(), updated_at = NOW()
         WHERE creem_subscription_id = $1",
    )
    .bind(&sub_id)
    .execute(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "success": true })))
}

pub async fn get_subscription_status(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let row: Option<(String, String, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
        "SELECT plan_name, status::text, current_period_end FROM subscriptions
         WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match row {
        Some((plan_name, status, current_period_end)) => Ok(Json(serde_json::json!({
            "plan_name": plan_name,
            "status": status,
            "current_period_end": current_period_end,
        }))),
        None => Ok(Json(serde_json::json!({
            "plan_name": null,
            "status": null,
            "current_period_end": null,
        }))),
    }
}
