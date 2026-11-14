use serde::Deserialize;
use serde_json::Value;

/// The raw shape of any Creem webhook event, before we look at what
/// kind of event it actually is. `object` is deliberately left as a
/// generic JSON value for now - each specific event type has a
/// different shape inside it, and we only need to parse the parts we
/// actually act on, once we get to that stage.
#[derive(Debug, Deserialize)]
pub struct CreemWebhookEvent {
    pub id: String,
    #[serde(rename = "eventType")]
    pub event_type: String,
    pub created_at: i64,
    pub object: Value,
}

#[derive(Debug, serde::Serialize)]
pub struct CreateCheckoutRequest {
    pub product_id: String,
    pub success_url: String,
    pub metadata: Value,
    /// {"email": ...}: the Safely email, filled in and locked on the
    /// payment page, so every payment belongs to one Creem customer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCheckoutResponse {
    pub id: String,
    pub checkout_url: String,
}

#[derive(Debug, Deserialize)]
pub struct ParsedSubscription {
    pub id: String,
    pub status: String,
    pub current_period_end_date: Option<String>,
    pub canceled_at: Option<Value>,
    pub product: ParsedProduct,
    pub customer: ParsedCustomer,
    pub metadata: Option<ParsedMetadata>,
}

#[derive(Debug, Deserialize)]
pub struct ParsedProduct {
    pub id: String,
    pub name: String,
    /// Creem's billing period, e.g. "every-month" or "every-year". Only
    /// used when the product ID isn't one of ours in .env.
    #[serde(default)]
    pub billing_period: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ParsedCustomer {
    pub id: String,
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct ParsedMetadata {
    pub safely_user_id: Option<String>,
}
