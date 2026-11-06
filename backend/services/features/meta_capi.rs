//! Meta Conversions API: tells Meta (Facebook) about a payment from the
//! server, the moment Creem's webhook confirms it. A browser Pixel would
//! only see a payment when the buyer comes back to the dashboard without
//! an ad blocker; this never misses one, and it also sends renewals.
//!
//! How a payment is matched to the ad the buyer came from:
//! - When checkout starts, the browser's Meta cookies (_fbp, _fbc) and
//!   its user agent go to Creem in the checkout's metadata, next to the
//!   Safely user ID (billing.rs create_checkout_for). Creem sends that
//!   metadata back in every webhook, so nothing new is stored in
//!   Safely's database.
//! - The buyer's email and Safely user ID are sent hashed (SHA-256), as
//!   Meta asks; never in plain text.
//! - event_id is "order_<Creem order id>". The first payment comes in
//!   both checkout.completed and subscription.paid with the same order id,
//!   so Meta counts it once; each renewal is a new order.
//! - Only the server sends "Purchase". The dashboard sends "Subscribe"
//!   (dashboard ts/init.ts): Creem's address after a subscription checkout
//!   has no order id, so a browser Purchase could not be matched to this
//!   one and every payment would be counted twice.
//!
//! .env (names only):
//! - META_CAPI_TOKEN: the access token from Events Manager. Missing =
//!   nothing is sent (localhost, tests).
//! - META_PIXEL_ID: optional, defaults to Safely's Pixel.
//! - META_TEST_EVENT_CODE: optional, only while testing (Events Manager >
//!   Test events); remove it in production.

use reqwest::Client;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::env::var;
use std::time::Duration;

/// Safely's Pixel (the same one the website and dashboard load).
const DEFAULT_PIXEL_ID: &str = "1490328886384904";
const GRAPH_API: &str = "https://graph.facebook.com/v21.0";

/// The browser details saved with a checkout (in Creem's metadata).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AdClick {
    /// The _fbp cookie: this browser, for Meta.
    pub fbp: Option<String>,
    /// The _fbc cookie: the Meta ad click this visit came from.
    pub fbc: Option<String>,
    pub user_agent: Option<String>,
}

impl AdClick {
    /// Reads the Meta cookies and the user agent from a request to
    /// Safely's API (the dashboard's checkout call). Same site, so the
    /// browser sends safely.sh's own cookies with it.
    pub fn from_headers(headers: &axum::http::HeaderMap) -> Self {
        let cookies = headers
            .get(axum::http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        let cookie = |name: &str| {
            cookies
                .split(';')
                .filter_map(|c| c.trim().split_once('='))
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.trim().to_string())
                .filter(|v| !v.is_empty() && v.len() <= 500)
        };
        AdClick {
            fbp: cookie("_fbp"),
            fbc: cookie("_fbc"),
            user_agent: headers
                .get(axum::http::header::USER_AGENT)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.chars().take(400).collect())
                .filter(|v: &String| !v.is_empty()),
        }
    }

    /// Added to a checkout's metadata next to safely_user_id.
    pub fn add_to_metadata(&self, metadata: &mut Value) {
        for (key, value) in [
            ("fbp", &self.fbp),
            ("fbc", &self.fbc),
            ("ua", &self.user_agent),
        ] {
            if let Some(v) = value {
                metadata[key] = json!(v);
            }
        }
    }

    /// Read back from a webhook's metadata.
    pub fn from_metadata(metadata: &Value) -> Self {
        let get = |key: &str| {
            metadata
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|v| !v.is_empty())
        };
        AdClick {
            fbp: get("fbp"),
            fbc: get("fbc"),
            user_agent: get("ua"),
        }
    }
}

/// One payment, as Creem reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Payment {
    pub order_id: String,
    /// In the currency's main unit (Creem sends cents: 20000 -> 200.0).
    pub value: f64,
    pub currency: String,
}

/// The payment in a Creem webhook: "checkout.completed" has it in
/// object.order, "subscription.paid" in object.last_transaction (with the
/// order id in its "order" field). The value is what was paid without
/// tax, so a discount code lowers it.
pub fn payment_in(event_type: &str, object: &Value) -> Option<Payment> {
    let (paid, order_id) = match event_type {
        "checkout.completed" => {
            let order = object.get("order")?;
            (order, order.get("id")?.as_str()?.to_string())
        }
        "subscription.paid" => {
            let tx = object.get("last_transaction")?;
            let order = tx
                .get("order")
                .and_then(|o| o.as_str().or_else(|| o.get("id").and_then(Value::as_str)))?;
            (tx, order.to_string())
        }
        _ => return None,
    };
    if paid
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|s| s != "paid")
    {
        return None;
    }
    // What the customer really paid, without tax: amount_paid - tax_amount
    // (Creem's "amount" is the price before a discount code). Falls back
    // to "amount" when those two are missing.
    let field = |name: &str| paid.get(name).and_then(Value::as_f64);
    let cents = match (field("amount_paid"), field("tax_amount")) {
        (Some(paid_total), Some(tax)) => paid_total - tax,
        _ => field("amount")?,
    };
    let currency = paid
        .get("currency")
        .and_then(Value::as_str)
        .unwrap_or("USD")
        .to_uppercase();
    (cents > 0.0 && !order_id.is_empty()).then(|| Payment {
        order_id,
        value: cents / 100.0,
        currency,
    })
}

fn sha256_hex(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// The Purchase event for Meta, as JSON (kept apart from sending so it
/// can be tested).
pub fn purchase_event(
    payment: &Payment,
    email: Option<&str>,
    user_id: Option<&str>,
    ad: &AdClick,
    event_time: i64,
    page_url: &str,
) -> Value {
    let mut user_data = json!({});
    if let Some(e) = email
        .map(|e| e.trim().to_lowercase())
        .filter(|e| e.contains('@'))
    {
        user_data["em"] = json!([sha256_hex(&e)]);
    }
    if let Some(id) = user_id.filter(|id| !id.is_empty()) {
        user_data["external_id"] = json!([sha256_hex(id)]);
    }
    if let Some(v) = &ad.fbp {
        user_data["fbp"] = json!(v);
    }
    if let Some(v) = &ad.fbc {
        user_data["fbc"] = json!(v);
    }
    if let Some(v) = &ad.user_agent {
        user_data["client_user_agent"] = json!(v);
    }
    json!({
        "event_name": "Purchase",
        "event_time": event_time,
        "event_id": format!("order_{}", payment.order_id),
        "action_source": "website",
        "event_source_url": page_url,
        "user_data": user_data,
        "custom_data": { "value": payment.value, "currency": payment.currency },
    })
}

/// Sends one Purchase to Meta. Never fails the webhook: a problem is only
/// logged (Creem must still get its 200, or it would send the event again).
pub async fn send_purchase(
    payment: Payment,
    email: Option<String>,
    user_id: Option<String>,
    ad: AdClick,
) {
    let Ok(token) = var("META_CAPI_TOKEN") else {
        return;
    };
    if token.trim().is_empty() {
        return;
    }
    let pixel = var("META_PIXEL_ID").unwrap_or_else(|_| DEFAULT_PIXEL_ID.to_string());
    let page = format!(
        "{}/dashboard/",
        var("PUBLIC_BASE_URL").unwrap_or_else(|_| "https://safely.sh".to_string())
    );
    let event = purchase_event(
        &payment,
        email.as_deref(),
        user_id.as_deref(),
        &ad,
        chrono::Utc::now().timestamp(),
        &page,
    );
    // The token goes in the body (never in the address, so it is not in
    // any log of the request).
    let mut body = json!({ "data": [event], "access_token": token.trim() });
    if let Ok(code) = var("META_TEST_EVENT_CODE")
        && !code.trim().is_empty()
    {
        body["test_event_code"] = json!(code.trim());
    }
    let Ok(client) = Client::builder().timeout(Duration::from_secs(15)).build() else {
        return;
    };
    let answer = client
        .post(format!("{GRAPH_API}/{pixel}/events"))
        .json(&body)
        .send()
        .await;
    match answer {
        Ok(r) if r.status().is_success() => eprintln!(
            "Safely: Meta Purchase sent (order {}, {} {})",
            payment.order_id, payment.value, payment.currency
        ),
        Ok(r) => {
            let status = r.status();
            let text = r.text().await.unwrap_or_default();
            eprintln!(
                "Safely: Meta refused the Purchase for order {} ({status}): {}",
                payment.order_id,
                text.chars().take(300).collect::<String>()
            );
        }
        Err(e) => eprintln!(
            "Safely: Meta could not be reached for order {}: {}",
            payment.order_id,
            e.without_url()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn the_meta_cookies_and_user_agent_are_read_from_the_request() {
        let mut h = HeaderMap::new();
        h.insert(
            "cookie",
            "safely_lang=en; _fbp=fb.1.1793287271522.391408334588489191; _fbc=fb.1.1793287271000.AbCdEf"
                .parse()
                .unwrap(),
        );
        h.insert("user-agent", "Mozilla/5.0 Test".parse().unwrap());
        let ad = AdClick::from_headers(&h);
        assert_eq!(
            ad.fbp.as_deref(),
            Some("fb.1.1793287271522.391408334588489191")
        );
        assert_eq!(ad.fbc.as_deref(), Some("fb.1.1793287271000.AbCdEf"));
        assert_eq!(ad.user_agent.as_deref(), Some("Mozilla/5.0 Test"));
        // Through Creem's metadata and back.
        let mut metadata = json!({ "safely_user_id": "u1" });
        ad.add_to_metadata(&mut metadata);
        assert_eq!(metadata["safely_user_id"], "u1");
        assert_eq!(AdClick::from_metadata(&metadata), ad);
        // No cookies (ad blocker): nothing added.
        let mut empty = json!({ "safely_user_id": "u1" });
        AdClick::default().add_to_metadata(&mut empty);
        assert_eq!(empty, json!({ "safely_user_id": "u1" }));
    }

    #[test]
    fn the_payment_is_read_from_both_creem_events() {
        // Creem's own examples (docs.creem.io, webhooks).
        let checkout = json!({
            "order": { "id": "ord_4aDwWXjMLpes4Kj4XqNnUA", "amount": 20000, "amount_paid": 24200, "tax_amount": 4200,
                       "currency": "usd", "status": "paid" }
        });
        let first = payment_in("checkout.completed", &checkout).unwrap();
        assert_eq!(
            first,
            Payment {
                order_id: "ord_4aDwWXjMLpes4Kj4XqNnUA".into(),
                value: 200.0,
                currency: "USD".into()
            }
        );
        let renewal = json!({
            "last_transaction": { "id": "tran_1", "amount": 20000, "amount_paid": 24200, "tax_amount": 4200,
                                  "currency": "USD", "status": "paid", "order": "ord_4aDwWXjMLpes4Kj4XqNnUA" }
        });
        // The first payment comes in both events with the same order id:
        // the same event_id, so Meta counts it once.
        assert_eq!(payment_in("subscription.paid", &renewal).unwrap(), first);
        // A discount code: Creem's "amount" stays the full price; the
        // value is what was paid without tax (1936 - 336 = 1600 -> 16.0).
        let discounted = json!({
            "order": { "id": "ord_d", "amount": 2000, "amount_paid": 1936, "tax_amount": 336,
                       "discount_amount": 400, "currency": "USD", "status": "paid" }
        });
        assert_eq!(
            payment_in("checkout.completed", &discounted).unwrap().value,
            16.0
        );
        // Not paid, or another event: nothing.
        let unpaid = json!({ "order": { "id": "ord_x", "amount": 20000, "status": "pending" } });
        assert_eq!(payment_in("checkout.completed", &unpaid), None);
        assert_eq!(payment_in("subscription.canceled", &renewal), None);
        assert_eq!(payment_in("checkout.completed", &json!({})), None);
    }

    #[test]
    fn the_purchase_event_hashes_personal_data() {
        let payment = Payment {
            order_id: "ord_1".into(),
            value: 200.0,
            currency: "USD".into(),
        };
        let ad = AdClick {
            fbp: Some("fb.1.2.3".into()),
            fbc: None,
            user_agent: Some("UA".into()),
        };
        let e = purchase_event(
            &payment,
            Some(" Me@Example.com "),
            Some("user-1"),
            &ad,
            1_800_000_000,
            "https://safely.sh/dashboard/",
        );
        assert_eq!(e["event_name"], "Purchase");
        assert_eq!(e["event_id"], "order_ord_1");
        assert_eq!(e["action_source"], "website");
        assert_eq!(
            e["custom_data"],
            json!({ "value": 200.0, "currency": "USD" })
        );
        // sha256("me@example.com")
        assert_eq!(
            e["user_data"]["em"][0],
            "8c2a47d3bdb8d3096a6479f53eac3b724291db5f1c31611100f675be5537329d"
        );
        assert_eq!(e["user_data"]["external_id"][0], sha256_hex("user-1"));
        assert_eq!(e["user_data"]["fbp"], "fb.1.2.3");
        assert_eq!(e["user_data"]["client_user_agent"], "UA");
        assert!(e["user_data"].get("fbc").is_none());
        // The plain email never leaves Safely.
        assert!(!e.to_string().contains("example.com"));
    }
}
