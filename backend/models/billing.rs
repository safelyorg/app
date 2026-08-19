use serde::Deserialize;
use serde_json::{Value, from_value};

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
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateCheckoutResponse {
    pub id: String,
    pub checkout_url: String,
}

/// The handful of fields we actually need from a subscription entity,
/// regardless of whether it arrived as the event's top-level object
/// (subscription.* events) or nested inside a checkout object
/// (checkout.completed). Everything else in Creem's real payload is
/// ignored - we only pull out what we act on.
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

/// Pulls the subscription entity out of a raw event, regardless of
/// which of the two real shapes it arrived in.
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
