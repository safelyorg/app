use crate::models::billing::ParsedSubscription;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use sqlx::{Pool, Postgres};

type HmacSha256 = Hmac<Sha256>;

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
    let computed = hex::encode(mac.finalize().into_bytes());
    computed == received_signature
}

use crate::models::billing::{CreateCheckoutRequest, CreateCheckoutResponse};
use reqwest::Client;
use std::env::var;
use uuid::Uuid;

pub enum CreateCheckoutError {
    MissingApiKey,
    RequestFailed(String),
    CreemRejected(String),
}

/// Creates a real Creem checkout session for the given product,
/// attaching the person's actual Safely user ID via metadata - this is
/// what lets every future webhook event tied to this subscription
/// (checkout.completed, subscription.paid, subscription.canceled, ...)
/// reliably identify which account to update, rather than guessing by
/// email.
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

/// Creates or updates a subscription row for a given user, keyed by
/// Creem's own subscription ID. Since this same function handles every
/// real lifecycle state (active, past_due, canceled, ...), it's called
/// from every event type that touches a subscription - each call just
/// passes in whatever status that specific event represents.
pub async fn upsert_subscription(
    pool: &Pool<Postgres>,
    user_id: Uuid,
    parsed: &ParsedSubscription,
    status: &str,
) -> Result<(), sqlx::Error> {
    let current_period_end = parsed
        .current_period_end_date
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));
    let canceled_at = parsed
        .canceled_at
        .as_ref()
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    sqlx::query(
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
