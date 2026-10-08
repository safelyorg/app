mod common;

use crate::common::{
    TestPaidPlan, TestSubscriptionOptions, auth_headers_for, cleanup_test_subscription,
    cleanup_test_user, compute_creem_signature, create_test_user, get_subscription_status_text,
    insert_active_subscription, insert_subscription_with_scans, insert_test_subscription,
    insert_test_subscription_full, load_env_once, test_pool,
};
use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, HeaderValue},
};
use backend::{
    errors::{
        auth::AuthError,
        billing::{BillingError, WebhookError},
    },
    handlers::billing::{
        ChangePlanBody, CreateCheckoutBody, cancel_subscription_handler, change_plan_handler,
        create_checkout_handler, creem_webhook, get_product_ids, get_subscription_status,
    },
    models::billing::{ParsedCustomer, ParsedMetadata, ParsedProduct, ParsedSubscription},
    services::{
        auth::find_or_create_user_by_email,
        billing::{
            BillingInterval, CreateCheckoutError, FREE_MONTHLY_SCANS, ScanLimitError,
            apply_scheduled_downgrade_if_due, apply_upgrade, cancel_with_creem,
            change_creem_subscription_product, check_and_increment_scan_usage, create_checkout,
            extract_metadata_user_id, extract_subscription, fetch_subscriber_email, get_scan_usage,
            handle_subscription_granted, handle_subscription_lost, handle_subscription_past_due,
            handle_subscription_update, is_new_billing_period, mark_event_processed_if_new,
            scan_limit_for_plan, upsert_subscription, verify_and_parse_webhook,
            verify_creem_signature,
        },
        email::{
            send_payment_failed_email, send_subscription_canceled_email,
            send_subscription_ended_email,
        },
    },
};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use hmac::{Hmac, KeyInit, Mac};
use reqwest::StatusCode;
use serde_json::json;
use serial_test::serial;
use sha2::Sha256;
use sqlx::{Pool, Postgres, query, query_as, query_scalar};
use std::env::{remove_var, set_var, var};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

// The real Creem product IDs from .env. Checkout and change-plan only
// accept these 4 products, so tests that reach those checks use them.
fn team_product_id() -> String {
    load_env_once();
    var("CREEM_TEAM_PRODUCT_ID").expect("expected CREEM_TEAM_PRODUCT_ID to be set in .env")
}

fn enterprise_product_id() -> String {
    load_env_once();
    var("CREEM_ENTERPRISE_PRODUCT_ID")
        .expect("expected CREEM_ENTERPRISE_PRODUCT_ID to be set in .env")
}

/// The yearly product IDs. If .env doesn't have them yet, a test value
/// is set for this test run only (the yearly code just needs an ID it
/// recognises). Only call this from #[serial] tests.
fn yearly_product_ids() -> (String, String) {
    load_env_once();
    let get_or_set = |name: &str, fallback: &str| -> String {
        var(name).unwrap_or_else(|_| {
            unsafe {
                set_var(name, fallback);
            }
            fallback.to_string()
        })
    };
    (
        get_or_set("CREEM_TEAM_YEARLY_PRODUCT_ID", "prod_test_team_yearly"),
        get_or_set(
            "CREEM_ENTERPRISE_YEARLY_PRODUCT_ID",
            "prod_test_enterprise_yearly",
        ),
    )
}

/// A subscription as Creem sends it in a webhook.
fn creem_subscription(
    sub_id: &str,
    user_id: Uuid,
    product_id: &str,
    product_name: &str,
    billing_period: Option<&str>,
    period_end: &str,
) -> ParsedSubscription {
    ParsedSubscription {
        id: sub_id.to_string(),
        status: "active".to_string(),
        current_period_end_date: Some(period_end.to_string()),
        canceled_at: None,
        product: ParsedProduct {
            id: product_id.to_string(),
            name: product_name.to_string(),
            billing_period: billing_period.map(|p| p.to_string()),
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "test@example.com".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some(user_id.to_string()),
        }),
    }
}

/// (plan_name, billing_interval, creem_product_id, scheduled_product_id,
/// scans_used_this_period, status) of one subscription row.
async fn subscription_row(
    pool: &Pool<Postgres>,
    sub_id: &str,
) -> (String, String, String, Option<String>, i32, String) {
    query_as(
        "SELECT plan_name, billing_interval, creem_product_id, scheduled_product_id,
                scans_used_this_period, status::text
         FROM subscriptions WHERE creem_subscription_id = $1",
    )
    .bind(sub_id)
    .fetch_one(pool)
    .await
    .expect("expected the subscription row to exist")
}

/// The UTC date of the user's sign-up plus `months` months.
async fn sign_up_date_plus_months(pool: &Pool<Postgres>, user_id: Uuid, months: i32) -> NaiveDate {
    query_scalar(
        "SELECT ((created_at + make_interval(months => $2)) AT TIME ZONE 'UTC')::date
         FROM users WHERE id = $1",
    )
    .bind(user_id)
    .bind(months)
    .fetch_one(pool)
    .await
    .expect("expected the user to exist")
}

// Checkout Handler Tests
#[tokio::test]
#[serial]
async fn checkout_handler_success() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };
    load_env_once();

    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/checkouts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "fake_checkout_id_123",
            "checkout_url": "https://creem.io/test/checkout/fake_product_id/ch_fake123"
        })))
        .mount(&mock_server)
        .await;

    let original_base_url = var("CREEM_API_BASE_URL").ok();
    unsafe {
        set_var("CREEM_API_BASE_URL", mock_server.uri());
    }

    let pool = test_pool().await;
    let email = "checkout_handler@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let checkout_body = CreateCheckoutBody {
        product_id: team_product_id(),
    };

    let result = create_checkout_handler(State(pool.clone()), headers, Json(checkout_body))
        .await
        .expect("expected to create the checkout");

    let checkout_url = result["checkout_url"]
        .as_str()
        .expect("expected checkout_url to be a real string");

    assert!(
        checkout_url.contains("creem.io"),
        "expected a genuine-looking Creem checkout URL, got: {}",
        checkout_url
    );

    unsafe {
        match original_base_url {
            Some(url) => set_var("CREEM_API_BASE_URL", url),
            None => remove_var("CREEM_API_BASE_URL"),
        }
    }

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn checkout_handler_unauthorized() {
    load_env_once();
    let pool = test_pool().await;
    let headers = HeaderMap::new();

    let checkout_body = CreateCheckoutBody {
        product_id: "prod_6qDjyvwKbCZvWTgIztzqz4".to_string(),
    };

    let result = create_checkout_handler(State(pool), headers, Json(checkout_body)).await;

    match result {
        Err(BillingError::Unauthorized) => {}
        Err(other) => panic!("expected Unauthorized, got a different error: {:?}", other),
        Ok(_) => panic!("expected an unauthenticated request to be rejected, but it succeeded"),
    }
}

#[tokio::test]
#[serial]
async fn checkout_handler_rejects_a_product_that_is_not_one_of_the_4_plans() {
    let pool = test_pool().await;
    let email = "checkout_rejected@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let checkout_body = CreateCheckoutBody {
        product_id: "definitely_not_a_real_product_id".to_string(),
    };

    let result = create_checkout_handler(State(pool.clone()), headers, Json(checkout_body)).await;

    match result {
        Err(BillingError::InvalidRequest(_)) => {}
        Err(other) => panic!(
            "expected InvalidRequest (unknown plan), got a different error: {:?}",
            other
        ),
        Ok(_) => panic!("expected a fake product_id to be rejected, but it succeeded"),
    }

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn checkout_handler_blocks_a_second_plan_with_the_same_billing() {
    // Already on Team monthly: buying another monthly plan must go
    // through change-plan, never a second checkout (no double billing).
    let pool = test_pool().await;
    let email = "checkout_second_plan@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;
    insert_test_subscription(&pool, user.id, "sub_checkout_second_001", "Team", "active").await;

    let result = create_checkout_handler(
        State(pool.clone()),
        headers,
        Json(CreateCheckoutBody {
            product_id: enterprise_product_id(),
        }),
    )
    .await;

    match result {
        Err(BillingError::InvalidRequest(msg)) => {
            assert!(msg.contains("change plan"), "got: {}", msg)
        }
        other => panic!("expected InvalidRequest, got: {:?}", other.map(|j| j.0)),
    }

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn checkout_handler_blocks_yearly_to_monthly() {
    let pool = test_pool().await;
    let email = "checkout_yearly_to_monthly@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;
    insert_subscription_with_scans(
        &pool,
        user.id,
        "sub_checkout_y2m_001",
        TestPaidPlan {
            plan_name: "Team",
            status: "active",
            billing_interval: "year",
            scans_used: 0,
            bought_days_ago: 10,
        },
    )
    .await;

    let result = create_checkout_handler(
        State(pool.clone()),
        headers,
        Json(CreateCheckoutBody {
            product_id: team_product_id(),
        }),
    )
    .await;

    match result {
        Err(BillingError::InvalidRequest(msg)) => {
            assert!(msg.contains("monthly"), "got: {}", msg)
        }
        other => panic!("expected InvalidRequest, got: {:?}", other.map(|j| j.0)),
    }

    cleanup_test_user(&pool, email).await;
}

// Create Checkout Tests
#[tokio::test]
#[serial]
async fn create_checkout_success() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    // A genuine, local, fake server standing in for Creem
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/checkouts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "fake_checkout_id_123",
            "checkout_url": "https://fake-checkout-url.test/abc123"
        })))
        .mount(&mock_server)
        .await;

    let original_base_url = var("CREEM_API_BASE_URL").ok();
    unsafe {
        set_var("CREEM_API_BASE_URL", mock_server.uri());
    }

    let pool = test_pool().await;
    let email = "create_checkout_mocked_test@example.com";
    cleanup_test_user(&pool, email).await;
    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

    let result = create_checkout("prod_6qDjyvwKbCZvWTgIztzqz4", user.id)
        .await
        .expect("expected the mocked checkout call to succeed");

    assert_eq!(result.checkout_url, "https://fake-checkout-url.test/abc123");

    unsafe {
        match original_base_url {
            Some(url) => set_var("CREEM_API_BASE_URL", url),
            None => remove_var("CREEM_API_BASE_URL"),
        }
    }
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn create_checkout_creem_rejected() {
    let pool = test_pool().await;
    let email = "checkout_creem_rejected@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let result = create_checkout("definitely_not_a_real_product_id", user.id).await;

    match result {
        Err(CreateCheckoutError::CreemRejected(_)) => {}
        Err(other) => panic!("expected CreemRejected, got a different error: {:?}", other),
        Ok(_) => panic!("expected Creem to reject a fake product_id, but it succeeded"),
    }

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn create_checkout_missing_api_key() {
    load_env_once();
    let original_key = var("CREEM_API_KEY").ok();
    unsafe {
        remove_var("CREEM_API_KEY");
    }

    let fake_user_id = Uuid::new_v4();
    let result = create_checkout("prod_6qDjyvwKbCZvWTgIztzqz4", fake_user_id).await;

    match result {
        Err(CreateCheckoutError::MissingApiKey) => {}
        Err(other) => panic!("expected MissingApiKey, got a different error: {:?}", other),
        Ok(_) => panic!("expected the checkout to fail without a real API key, but it succeeded"),
    }

    unsafe {
        if let Some(key) = original_key {
            set_var("CREEM_API_KEY", key);
        }
    }
}

#[tokio::test]
#[serial]
async fn create_checkout_request_failed() {
    load_env_once();
    let original_base_url = var("CREEM_API_BASE_URL").ok();

    unsafe {
        set_var(
            "CREEM_API_BASE_URL",
            "http://this-domain-genuinely-does-not-exist-12345.invalid",
        );
    }

    let fake_user_id = Uuid::new_v4();
    let result = create_checkout("prod_6qDjyvwKbCZvWTgIztzqz4", fake_user_id).await;

    match result {
        Err(CreateCheckoutError::RequestFailed(_)) => {}
        Err(other) => panic!("expected RequestFailed, got a different error: {:?}", other),
        Ok(_) => panic!("expected the request to genuinely fail, but it succeeded"),
    }

    unsafe {
        match original_base_url {
            Some(url) => set_var("CREEM_API_BASE_URL", url),
            None => remove_var("CREEM_API_BASE_URL"),
        }
    }
}

// Creem Webhook Tests
#[tokio::test]
#[serial]
async fn creem_webhook_verification_failure_propagates() {
    load_env_once();

    let pool = test_pool().await;

    let mut headers = HeaderMap::new();
    headers.insert(
        "creem-signature",
        HeaderValue::from_str("clearly-wrong-signature-that-cannot-match")
            .expect("expected to insert the header value"),
    );

    let body = Bytes::from(
        r#"{"id":"evt_test","eventType":"refund.created","created_at":1700000000,"object":{}}"#,
    );

    let result = creem_webhook(State(pool), headers, body).await;

    match result {
        Err(WebhookError::InvalidSignature) => {}
        Err(other) => panic!(
            "expected InvalidSignature, got a different error: {:?}",
            other
        ),
        Ok(_) => {
            panic!("expected the whole webhook to be rejected on a bad signature, but it succeeded")
        }
    }
}

#[tokio::test]
#[serial]
async fn creem_webhook_success_refund_created() {
    load_env_once();

    let pool = test_pool().await;

    let secret = var("CREEM_WEBHOOK_SECRET")
        .expect("expected CREEM_WEBHOOK_SECRET to be genuinely set for this test");

    let raw_body = r#"{"id":"evt_refund_test_001","eventType":"refund.created","created_at":1700000000,"object":{}}"#;

    let real_signature = compute_creem_signature(&secret, raw_body);

    let mut headers = HeaderMap::new();
    headers.insert(
        "creem-signature",
        HeaderValue::from_str(&real_signature).expect("expected to insert the header value"),
    );

    let body = Bytes::from(raw_body);

    let result = creem_webhook(State(pool), headers, body)
        .await
        .expect("expected the whole webhook to succeed end-to-end");

    assert_eq!(result, StatusCode::OK);
}

#[tokio::test]
#[serial]
async fn creem_webhook_unrecognized_event_type_still_ok() {
    load_env_once();

    let pool = test_pool().await;

    let secret = var("CREEM_WEBHOOK_SECRET")
        .expect("expected CREEM_WEBHOOK_SECRET to be genuinely set for this test");

    let raw_body = r#"{"id":"evt_unknown_test_001","eventType":"some.future.event","created_at":1700000000,"object":{}}"#;

    let real_signature = compute_creem_signature(&secret, raw_body);

    let mut headers = HeaderMap::new();
    headers.insert(
        "creem-signature",
        HeaderValue::from_str(&real_signature).expect("expected to insert the header value"),
    );

    let body = Bytes::from(raw_body);

    let result = creem_webhook(State(pool), headers, body)
        .await
        .expect("expected an unrecognized event type to still succeed, not fail");

    assert_eq!(result, StatusCode::OK);
}

#[tokio::test]
#[serial]
async fn creem_webhook_inner_handler_failure_still_returns_ok() {
    load_env_once();
    let pool = test_pool().await;

    let secret = var("CREEM_WEBHOOK_SECRET")
        .expect("expected CREEM_WEBHOOK_SECRET to be genuinely set for this test");

    let sub_id = "sub_webhook_inner_failure_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let fake_user_id = Uuid::new_v4();

    let raw_body = format!(
        r#"{{"id":"evt_inner_failure_{}","eventType":"subscription.paid","created_at":1700000000,"object":{{"id":"{}","status":"active","current_period_end_date":null,"canceled_at":null,"product":{{"id":"prod_test","name":"Team"}},"customer":{{"id":"cust_test","email":"test@example.com"}},"metadata":{{"safely_user_id":"{}"}}}}}}"#,
        fake_user_id, sub_id, fake_user_id
    );

    let real_signature = compute_creem_signature(&secret, &raw_body);

    let mut headers = HeaderMap::new();
    headers.insert(
        "creem-signature",
        HeaderValue::from_str(&real_signature).expect("expected to insert the header value"),
    );

    let body = Bytes::from(raw_body);

    let result = creem_webhook(State(pool.clone()), headers, body)
        .await
        .expect("expected the whole webhook to still return Ok, despite the inner failure");

    assert_eq!(result, StatusCode::OK);

    let saved_row: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

    assert!(
        saved_row.is_none(),
        "expected NO subscription row to exist, confirming the inner failure was genuine"
    );
}

// Verify and Parse Webhook Tests
#[tokio::test]
#[serial]
async fn verify_and_parse_webhook_secret_missing() {
    load_env_once();

    let original_secret = var("CREEM_WEBHOOK_SECRET").ok();
    unsafe {
        remove_var("CREEM_WEBHOOK_SECRET");
    }

    let headers = HeaderMap::new();
    let body = Bytes::from("{}");

    let result = verify_and_parse_webhook(&headers, &body).await;

    match result {
        Err(WebhookError::Misconfigured(_)) => {}
        Err(other) => panic!("expected Misconfigured, got a different error: {:?}", other),
        Ok(_) => panic!("expected verification to fail without a real secret, but it succeeded"),
    }

    unsafe {
        if let Some(secret) = original_secret {
            set_var("CREEM_WEBHOOK_SECRET", secret);
        }
    }
}

#[tokio::test]
#[serial]
async fn verify_and_parse_webhook_missing_signature() {
    load_env_once();

    let headers = HeaderMap::new();
    let body = Bytes::from("{}");

    let result = verify_and_parse_webhook(&headers, &body).await;

    match result {
        Err(WebhookError::MissingSignature) => {}
        Err(other) => panic!(
            "expected MissingSignature, got a different error: {:?}",
            other
        ),
        Ok(_) => {
            panic!("expected verification to fail without a signature header, but it succeeded")
        }
    }
}

#[tokio::test]
#[serial]
async fn verify_and_parse_webhook_invalid_body() {
    load_env_once();

    let mut headers = HeaderMap::new();
    headers.insert(
        "creem-signature",
        HeaderValue::from_str("some-signature-value").expect("expected to insert the header value"),
    );

    // invalid UTF-8
    let body = Bytes::from(vec![0xFF, 0xFE, 0xFD]);

    let result = verify_and_parse_webhook(&headers, &body).await;
    match result {
        Err(WebhookError::InvalidBody) => {}
        Err(other) => panic!("expected InvalidBody, got a different error: {:?}", other),
        Ok(_) => panic!("expected verification to fail on invalid UTF-8, but it succeeded"),
    }
}

#[tokio::test]
#[serial]
async fn verify_and_parse_webhook_invalid_signature() {
    load_env_once();

    let mut headers = HeaderMap::new();
    headers.insert(
        "creem-signature",
        HeaderValue::from_str("clearly-wrong-signature-that-cannot-match")
            .expect("expected to insert the header value"),
    );
    let body = Bytes::from(r#"{"id":"evt_test","event_type":"test.event","object":{}}"#);

    let result = verify_and_parse_webhook(&headers, &body).await;
    match result {
        Err(WebhookError::InvalidSignature) => {}
        Err(other) => panic!(
            "expected InvalidSignature, got a different error: {:?}",
            other
        ),
        Ok(_) => {
            panic!("expected verification to fail on a mismatched signature, but it succeeded")
        }
    }
}

#[tokio::test]
#[serial]
async fn verify_and_parse_webhook_invalid_payload() {
    load_env_once();

    let secret = var("CREEM_WEBHOOK_SECRET")
        .expect("expected CREEM_WEBHOOK_SECRET to be genuinely set for this test");
    let raw_body = "not valid json{{{";
    let real_signature = compute_creem_signature(&secret, raw_body);

    let mut headers = HeaderMap::new();
    headers.insert(
        "creem-signature",
        HeaderValue::from_str(&real_signature).expect("expected to insert the header value"),
    );

    let body = Bytes::from(raw_body);

    let result = verify_and_parse_webhook(&headers, &body).await;
    match result {
        Err(WebhookError::InvalidPayload(_)) => {}
        Err(other) => panic!(
            "expected InvalidPayload, got a different error: {:?}",
            other
        ),
        Ok(_) => panic!("expected parsing to fail on malformed JSON, but it succeeded"),
    }
}

#[tokio::test]
#[serial]
async fn verify_and_parse_webhook_success() {
    load_env_once();

    let secret = var("CREEM_WEBHOOK_SECRET")
        .expect("expected CREEM_WEBHOOK_SECRET to be genuinely set for this test");
    let raw_body = r#"{"id":"evt_test_success_001","eventType":"checkout.completed","created_at":1700000000,"object":{}}"#;
    let real_signature = compute_creem_signature(&secret, raw_body);

    let mut headers = HeaderMap::new();
    headers.insert(
        "creem-signature",
        HeaderValue::from_str(&real_signature).expect("expected to insert the header value"),
    );

    let body = Bytes::from(raw_body);

    let event = verify_and_parse_webhook(&headers, &body)
        .await
        .expect("expected verification and parsing to genuinely succeed");

    assert_eq!(event.id, "evt_test_success_001");
    assert_eq!(event.event_type, "checkout.completed");
}

// Verify Creem Signature Tests
#[test]
fn verify_creem_signature_genuine_match() {
    let secret = "test_secret_001";
    let body = r#"{"id":"evt_test","eventType":"refund.created"}"#;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("expected to build a real HMAC instance");

    mac.update(body.as_bytes());
    let real_signature = hex::encode(mac.finalize().into_bytes());

    let result = verify_creem_signature(body, &real_signature, secret);
    assert!(
        result,
        "expected a genuinely correct signature to verify successfully"
    );
}

#[test]
fn verify_creem_signature_genuine_mismatch() {
    let secret = "test_secret_001";
    let body = r#"{"id":"evt_test","eventType":"refund.created"}"#;

    let result = verify_creem_signature(body, "clearly_wrong_signature_00000", secret);

    assert!(
        !result,
        "expected a genuinely wrong signature to fail verification"
    );
}

#[test]
fn verify_creem_signature_different_body_fails() {
    let secret = "test_secret_001";
    let original_body = r#"{"id":"evt_test","eventType":"refund.created"}"#;
    let tampered_body = r#"{"id":"evt_test","eventType":"subscription.canceled"}"#;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("expected to build a real HMAC instance");

    mac.update(original_body.as_bytes());
    let signature_for_original = hex::encode(mac.finalize().into_bytes());

    let result = verify_creem_signature(tampered_body, &signature_for_original, secret);
    assert!(
        !result,
        "expected a signature computed for one body to fail against a genuinely different body"
    );
}

#[test]
fn verify_creem_signature_different_secret_fails() {
    let body = r#"{"id":"evt_test","eventType":"refund.created"}"#;
    let real_secret = "the_real_secret_001";
    let wrong_secret = "a_completely_different_secret_002";

    let mut mac = HmacSha256::new_from_slice(real_secret.as_bytes())
        .expect("expected to build a real HMAC instance");

    mac.update(body.as_bytes());
    let real_signature = hex::encode(mac.finalize().into_bytes());

    let result = verify_creem_signature(body, &real_signature, wrong_secret);
    assert!(
        !result,
        "expected verification to fail when checked against the WRONG secret"
    );
}

#[test]
fn verify_creem_signature_case_sensitive() {
    let secret = "test_secret_001";
    let body = r#"{"id":"evt_test","eventType":"refund.created"}"#;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("expected to build a real HMAC instance");

    mac.update(body.as_bytes());
    let real_signature = hex::encode(mac.finalize().into_bytes());
    let uppercased_signature = real_signature.to_uppercase();

    let result = verify_creem_signature(body, &uppercased_signature, secret);
    assert!(
        !result,
        "expected an uppercased version of a correct signature to fail, since comparison is case-sensitive"
    );
}

// Extract Subscription Tests
#[test]
fn extract_subscription_subscription_shape_success() {
    let object = serde_json::json!({
        "id": "sub_test_001",
        "status": "active",
        "current_period_end_date": null,
        "canceled_at": null,
        "product": { "id": "prod_test", "name": "Team" },
        "customer": { "id": "cust_test", "email": "test@example.com" },
        "metadata": null
    });

    let result = extract_subscription("subscription.paid", &object);
    let parsed = result.expect("expected the subscription to be extracted successfully");
    assert_eq!(parsed.id, "sub_test_001");
    assert_eq!(parsed.status, "active");
    assert_eq!(
        parsed.product.billing_period, None,
        "expected a product without billing_period to still parse"
    );
}

#[test]
fn extract_subscription_reads_the_product_billing_period() {
    let object = serde_json::json!({
        "id": "sub_test_yearly_001",
        "status": "active",
        "current_period_end_date": null,
        "canceled_at": null,
        "product": { "id": "prod_test", "name": "Team", "billing_period": "every-year" },
        "customer": { "id": "cust_test", "email": "test@example.com" },
        "metadata": null
    });

    let parsed = extract_subscription("subscription.paid", &object)
        .expect("expected the subscription to be extracted successfully");
    assert_eq!(parsed.product.billing_period.as_deref(), Some("every-year"));
}

#[test]
fn extract_subscription_checkout_completed_shape_success() {
    let object = json!({
        "subscription": {
            "id": "sub_nested_001",
            "status": "active",
            "current_period_end_date": null,
            "canceled_at": null,
            "product": { "id": "prod_test", "name": "Team" },
            "customer": { "id": "cust_test", "email": "test@example.com" },
            "metadata": null
        }
    });

    let result = extract_subscription("checkout.completed", &object);
    let parsed = result.expect("expected the NESTED subscription to be extracted successfully");

    assert_eq!(parsed.id, "sub_nested_001");
    assert_eq!(parsed.status, "active");
}

#[test]
fn extract_subscription_checkout_completed_missing_nested_field() {
    let object = serde_json::json!({
        "id": "evt_test_001"
    });

    let result = extract_subscription("checkout.completed", &object);
    assert!(
        result.is_none(),
        "expected None when checkout.completed genuinely lacks a nested subscription field"
    );
}

#[test]
fn extract_subscription_unrecognized_event_type() {
    let object = serde_json::json!({
        "id": "evt_test_001"
    });

    let result = extract_subscription("refund.created", &object);

    assert!(
        result.is_none(),
        "expected None for an event type that's neither shape"
    );
}

#[test]
fn extract_subscription_malformed_data_returns_none() {
    let object = json!({
        "id": "sub_malformed_001",
        "status": "active"
    });

    let result = extract_subscription("subscription.paid", &object);
    assert!(
        result.is_none(),
        "expected None when the data doesn't genuinely match ParsedSubscription's real shape"
    );
}

// Handle Subscription Granted Tests
#[tokio::test]
async fn subscription_granted_success() {
    let pool = test_pool().await;

    let email = "subscription_granted_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let sub_id = "sub_granted_test_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "active".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: email.to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some(user.id.to_string()),
        }),
    };

    handle_subscription_granted(&pool, &parsed).await;
    let saved_status = get_subscription_status_text(&pool, sub_id).await;

    assert_eq!(
        saved_status,
        Some("active".to_string()),
        "expected a real subscription row to exist with status 'active'"
    );

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn subscription_granted_missing_user_id() {
    let pool = test_pool().await;

    let sub_id = "sub_granted_missing_user_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "active".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "test@example.com".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: None,
        }),
    };

    handle_subscription_granted(&pool, &parsed).await;

    let saved_row: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

    assert!(
        saved_row.is_none(),
        "expected NO subscription row to be created when safely_user_id is missing"
    );
}

#[tokio::test]
async fn subscription_granted_upsert_fails() {
    let pool = test_pool().await;

    let sub_id = "sub_granted_upsert_fails_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let fake_user_id = Uuid::new_v4();
    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "active".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "test@example.com".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some(fake_user_id.to_string()),
        }),
    };

    handle_subscription_granted(&pool, &parsed).await;

    let saved_row: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

    assert!(
        saved_row.is_none(),
        "expected NO subscription row to exist, since the foreign key genuinely failed"
    );
}

#[tokio::test]
#[serial]
async fn switching_to_yearly_ends_the_old_monthly_plan() {
    // Monthly -> yearly is a new Creem subscription. Once the yearly one
    // is active, the old monthly one is canceled, so nobody pays twice.
    let pool = test_pool().await;
    let email = "switch_to_yearly_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let (team_yearly_id, _) = yearly_product_ids();

    let old_monthly = "sub_switch_old_monthly_001";
    let new_yearly = "sub_switch_new_yearly_001";
    cleanup_test_subscription(&pool, old_monthly).await;
    cleanup_test_subscription(&pool, new_yearly).await;
    insert_test_subscription(&pool, user.id, old_monthly, "Team", "active").await;

    let parsed = creem_subscription(
        new_yearly,
        user.id,
        &team_yearly_id,
        "Team",
        Some("every-year"),
        "2027-10-06T00:00:00Z",
    );
    handle_subscription_granted(&pool, &parsed).await;

    let (plan, interval, _, _, used, status) = subscription_row(&pool, new_yearly).await;
    assert_eq!((plan.as_str(), interval.as_str()), ("Team", "year"));
    assert_eq!((used, status.as_str()), (0, "active"));

    assert_eq!(
        get_subscription_status_text(&pool, old_monthly).await,
        Some("canceled".to_string()),
        "expected the old monthly plan to be ended once the yearly one is active"
    );

    cleanup_test_user(&pool, email).await;
}

// Extract Metadata User ID Tests
#[test]
fn extract_metadata_user_id_missing_metadata() {
    let parsed = ParsedSubscription {
        id: "sub_test_001".to_string(),
        status: "active".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "test@example.com".to_string(),
        },
        metadata: None,
    };

    let result = extract_metadata_user_id(&parsed);
    assert!(
        result.is_none(),
        "expected None when metadata itself is missing"
    );
}

#[test]
fn extract_metadata_user_id_missing_safely_user_id() {
    let parsed = ParsedSubscription {
        id: "sub_test_001".to_string(),
        status: "active".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "test@example.com".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: None,
        }),
    };

    let result = extract_metadata_user_id(&parsed);
    assert!(
        result.is_none(),
        "expected None when safely_user_id itself is missing"
    );
}

#[test]
fn extract_metadata_user_id_invalid_uuid() {
    let parsed = ParsedSubscription {
        id: "sub_test_001".to_string(),
        status: "active".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "test@example.com".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some("this-is-genuinely-not-a-uuid".to_string()),
        }),
    };

    let result = extract_metadata_user_id(&parsed);
    assert!(
        result.is_none(),
        "expected None when the string isn't a genuinely valid UUID"
    );
}

#[test]
fn extract_metadata_user_id_success() {
    let real_uuid = Uuid::new_v4();

    let parsed = ParsedSubscription {
        id: "sub_test_001".to_string(),
        status: "active".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "test@example.com".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some(real_uuid.to_string()),
        }),
    };

    let result = extract_metadata_user_id(&parsed);
    assert_eq!(result, Some(real_uuid), "expected the exact same UUID back");
}

// Upsert Subscription
#[tokio::test]
async fn upsert_subscription_creates_new_row_with_full_data() {
    let pool = test_pool().await;
    let email = "upsert_sub_create_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let sub_id = "sub_upsert_create_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "active".to_string(),
        current_period_end_date: Some("2026-12-31T23:59:59Z".to_string()),
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: email.to_string(),
        },
        metadata: None,
    };

    upsert_subscription(&pool, user.id, &parsed, "active")
        .await
        .expect("expected the upsert to succeed");

    let (saved_status, saved_period_end): (String, Option<DateTime<Utc>>) = query_as(
        "SELECT status::text, current_period_end FROM subscriptions WHERE creem_subscription_id = $1",
    )
    .bind(sub_id)
    .fetch_one(&pool)
    .await
    .expect("expected the row to exist");

    assert_eq!(saved_status, "active");
    assert!(
        saved_period_end.is_some(),
        "expected the period end to be genuinely parsed and saved"
    );

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .ok();
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn upsert_subscription_update_unconditionally_overwrites() {
    let pool = test_pool().await;
    let email = "upsert_sub_overwrite_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let sub_id = "sub_upsert_overwrite_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "active".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: email.to_string(),
        },
        metadata: None,
    };

    upsert_subscription(&pool, user.id, &parsed, "active")
        .await
        .expect("expected the first upsert to succeed");

    upsert_subscription(&pool, user.id, &parsed, "canceled")
        .await
        .expect("expected the second upsert to succeed");

    let saved_status: String =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_one(&pool)
            .await
            .expect("expected the row to exist");

    assert_eq!(
        saved_status, "canceled",
        "expected the status to be UNCONDITIONALLY overwritten, with no preservation logic"
    );

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .ok();

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn upsert_subscription_missing_period_end_saves_as_null() {
    let pool = test_pool().await;
    let email = "upsert_sub_missing_period_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let sub_id = "sub_upsert_missing_period_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "active".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: email.to_string(),
        },
        metadata: None,
    };

    upsert_subscription(&pool, user.id, &parsed, "active")
        .await
        .expect("expected the upsert to succeed");

    let saved_period_end: Option<DateTime<Utc>> = query_scalar(
        "SELECT current_period_end FROM subscriptions WHERE creem_subscription_id = $1",
    )
    .bind(sub_id)
    .fetch_one(&pool)
    .await
    .expect("expected the row to exist");

    assert!(
        saved_period_end.is_none(),
        "expected NULL when current_period_end_date is None"
    );

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .ok();

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn upsert_subscription_malformed_period_end_saves_as_null() {
    let pool = test_pool().await;
    let email = "upsert_sub_malformed_period_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let sub_id = "sub_upsert_malformed_period_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "active".to_string(),
        current_period_end_date: Some("not-a-real-date".to_string()),
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: email.to_string(),
        },
        metadata: None,
    };

    upsert_subscription(&pool, user.id, &parsed, "active")
        .await
        .expect("expected the upsert to succeed DESPITE the malformed date");

    let saved_period_end: Option<DateTime<Utc>> = query_scalar(
        "SELECT current_period_end FROM subscriptions WHERE creem_subscription_id = $1",
    )
    .bind(sub_id)
    .fetch_one(&pool)
    .await
    .expect("expected the row to exist");

    assert!(
        saved_period_end.is_none(),
        "expected a malformed date to quietly become NULL, not cause an error"
    );

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .ok();
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn upsert_subscription_fails_for_nonexistent_user() {
    let pool = test_pool().await;
    let sub_id = "sub_upsert_fake_user_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let fake_user_id = Uuid::new_v4();
    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "active".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "test@example.com".to_string(),
        },
        metadata: None,
    };

    let result = upsert_subscription(&pool, fake_user_id, &parsed, "active").await;
    assert!(
        result.is_err(),
        "expected a genuine foreign-key failure for a user that doesn't exist"
    );
}

#[tokio::test]
#[serial]
async fn upsert_subscription_saves_a_yearly_product_as_team_on_yearly_billing() {
    let pool = test_pool().await;
    let email = "upsert_yearly_product_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let (team_yearly_id, _) = yearly_product_ids();

    let sub_id = "sub_upsert_yearly_001";
    cleanup_test_subscription(&pool, sub_id).await;

    // The product name doesn't matter: our own product ID decides.
    let parsed = creem_subscription(
        sub_id,
        user.id,
        &team_yearly_id,
        "Some other name",
        None,
        "2027-10-06T00:00:00Z",
    );
    upsert_subscription(&pool, user.id, &parsed, "active")
        .await
        .expect("expected the upsert to succeed");

    let (plan, interval, product, _, _, _) = subscription_row(&pool, sub_id).await;
    assert_eq!(plan, "Team");
    assert_eq!(interval, "year");
    assert_eq!(product, team_yearly_id);

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn upsert_subscription_falls_back_to_the_product_name_and_billing_period() {
    // A product ID that isn't in .env: the name and billing period
    // still give the right plan, so it never gets 0 scans by mistake.
    let pool = test_pool().await;
    let email = "upsert_name_fallback_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let sub_id = "sub_upsert_name_fallback_001";
    cleanup_test_subscription(&pool, sub_id).await;

    let parsed = creem_subscription(
        sub_id,
        user.id,
        "prod_not_in_env",
        "Safely Enterprise Yearly",
        Some("every-year"),
        "2027-10-06T00:00:00Z",
    );
    upsert_subscription(&pool, user.id, &parsed, "active")
        .await
        .expect("expected the upsert to succeed");

    let (plan, interval, _, _, _, _) = subscription_row(&pool, sub_id).await;
    assert_eq!((plan.as_str(), interval.as_str()), ("Enterprise", "year"));

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn upsert_subscription_resets_scans_only_when_a_new_period_starts() {
    let pool = test_pool().await;
    let email = "upsert_renewal_reset_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let sub_id = "sub_upsert_renewal_reset_001";
    cleanup_test_subscription(&pool, sub_id).await;

    let first = creem_subscription(
        sub_id,
        user.id,
        "prod_test",
        "Team",
        None,
        "2026-11-06T00:00:00Z",
    );
    upsert_subscription(&pool, user.id, &first, "active")
        .await
        .expect("expected the first upsert to succeed");
    query("UPDATE subscriptions SET scans_used_this_period = 300 WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected to set the scan count");

    // Same period sent again: the count stays.
    upsert_subscription(&pool, user.id, &first, "active")
        .await
        .expect("expected the repeat upsert to succeed");
    assert_eq!(subscription_row(&pool, sub_id).await.4, 300);

    // Renewal (period end moved forward): back to 0.
    let renewed = creem_subscription(
        sub_id,
        user.id,
        "prod_test",
        "Team",
        None,
        "2026-12-06T00:00:00Z",
    );
    upsert_subscription(&pool, user.id, &renewed, "active")
        .await
        .expect("expected the renewal upsert to succeed");
    assert_eq!(subscription_row(&pool, sub_id).await.4, 0);

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn upsert_subscription_keeps_the_paid_plan_until_a_scheduled_downgrade_renews() {
    let pool = test_pool().await;
    let email = "upsert_scheduled_downgrade_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let (team_yearly_id, enterprise_yearly_id) = yearly_product_ids();

    let sub_id = "sub_upsert_scheduled_downgrade_001";
    cleanup_test_subscription(&pool, sub_id).await;

    let enterprise = creem_subscription(
        sub_id,
        user.id,
        &enterprise_yearly_id,
        "Enterprise",
        None,
        "2027-01-01T00:00:00Z",
    );
    upsert_subscription(&pool, user.id, &enterprise, "active")
        .await
        .expect("expected the first upsert to succeed");
    query(
        "UPDATE subscriptions SET scheduled_product_id = $1, scheduled_plan_name = 'Team',
         scans_used_this_period = 40 WHERE creem_subscription_id = $2",
    )
    .bind(&team_yearly_id)
    .bind(sub_id)
    .execute(&pool)
    .await
    .expect("expected to schedule the downgrade");

    // Creem already switched the product (same period): still Enterprise.
    let switched = creem_subscription(
        sub_id,
        user.id,
        &team_yearly_id,
        "Team",
        None,
        "2027-01-01T00:00:00Z",
    );
    upsert_subscription(&pool, user.id, &switched, "active")
        .await
        .expect("expected the update upsert to succeed");
    let (plan, _, product, scheduled, used, _) = subscription_row(&pool, sub_id).await;
    assert_eq!(plan, "Enterprise");
    assert_eq!(product, enterprise_yearly_id);
    assert_eq!(scheduled.as_deref(), Some(team_yearly_id.as_str()));
    assert_eq!(used, 40);

    // The renewal arrives: now it's Team, the schedule is cleared.
    let renewed = creem_subscription(
        sub_id,
        user.id,
        &team_yearly_id,
        "Team",
        None,
        "2028-01-01T00:00:00Z",
    );
    upsert_subscription(&pool, user.id, &renewed, "active")
        .await
        .expect("expected the renewal upsert to succeed");
    let (plan, _, product, scheduled, used, _) = subscription_row(&pool, sub_id).await;
    assert_eq!(plan, "Team");
    assert_eq!(product, team_yearly_id);
    assert_eq!(scheduled, None);
    assert_eq!(used, 0);

    cleanup_test_user(&pool, email).await;
}

// Handle Subscription Past Due Tests
#[tokio::test]
async fn subscription_past_due_success() {
    let pool = test_pool().await;
    let email = "past_due_test_user@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let sub_id = "sub_past_due_test_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "past_due".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "delivered@resend.dev".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some(user.id.to_string()),
        }),
    };

    handle_subscription_past_due(&pool, &parsed).await;
    let saved_status = get_subscription_status_text(&pool, sub_id).await;

    assert_eq!(
        saved_status,
        Some("past_due".to_string()),
        "expected a real subscription row to exist with status 'past_due'"
    );

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn subscription_past_due_missing_user_id() {
    let pool = test_pool().await;

    let sub_id = "sub_past_due_missing_user_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "past_due".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "delivered@resend.dev".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: None,
        }),
    };

    handle_subscription_past_due(&pool, &parsed).await;

    let saved_row: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

    assert!(
        saved_row.is_none(),
        "expected NO subscription row to be created when safely_user_id is missing"
    );
}

#[tokio::test]
async fn subscription_past_due_upsert_fails() {
    let pool = test_pool().await;

    let sub_id = "sub_past_due_upsert_fails_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let fake_user_id = Uuid::new_v4();

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "past_due".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "delivered@resend.dev".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some(fake_user_id.to_string()),
        }),
    };

    handle_subscription_past_due(&pool, &parsed).await;

    let saved_row: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

    assert!(
        saved_row.is_none(),
        "expected NO subscription row to exist, since the foreign key genuinely failed"
    );
}

#[tokio::test]
async fn subscription_past_due_email_fails_but_upsert_still_succeeds() {
    let pool = test_pool().await;
    let email = "past_due_email_fails_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let sub_id = "sub_past_due_email_fails_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "past_due".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "genuinely_blocked@example.com".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some(user.id.to_string()),
        }),
    };

    handle_subscription_past_due(&pool, &parsed).await;
    let saved_status = get_subscription_status_text(&pool, sub_id).await;

    assert_eq!(
        saved_status,
        Some("past_due".to_string()),
        "expected the subscription to still be upserted, even though the email genuinely failed"
    );

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}

// Send Payment Failed Email Tests
#[tokio::test]
#[serial]
async fn send_payment_failed_email_missing_api_key() {
    load_env_once();
    let original_key = var("RESEND_API_KEY").ok();
    unsafe {
        remove_var("RESEND_API_KEY");
    }

    let result = send_payment_failed_email(
        "delivered@resend.dev",
        "http://localhost:3000/dashboard/?manage_billing=1",
    )
    .await;

    match result {
        Err(AuthError::InternalServerError(_)) => {}
        Err(other) => panic!(
            "expected InternalServerError, got a different error: {:?}",
            other
        ),
        Ok(_) => panic!("expected the email to fail without a real API key, but it succeeded"),
    }

    unsafe {
        if let Some(key) = original_key {
            set_var("RESEND_API_KEY", key);
        }
    }
}

#[tokio::test]
#[serial]
async fn send_payment_failed_email_succeeds() {
    load_env_once();

    let result = send_payment_failed_email(
        "delivered@resend.dev",
        "http://localhost:3000/dashboard/?manage_billing=1",
    )
    .await;

    assert!(
        result.is_ok(),
        "expected the payment-failed email to send successfully, got: {:?}",
        result
    );
}

#[tokio::test]
#[serial]
async fn send_payment_failed_email_fails_for_a_blocked_domain() {
    load_env_once();

    let result = send_payment_failed_email(
        "test_user@example.com",
        "http://localhost:3000/dashboard/?manage_billing=1",
    )
    .await;

    match result {
        Err(AuthError::InternalServerError(message)) => {
            assert!(
                message.contains("Invalid `to` field"),
                "expected Resend's specific rejection message, got: {}",
                message
            )
        }
        other => panic!("expected an InternalServerError, got: {:?}", other),
    }
}

// Handle Subscription Lost Tests
#[tokio::test]
async fn subscription_lost_paused() {
    let pool = test_pool().await;
    let email = "subscription_lost_paused_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let sub_id = "sub_lost_paused_test_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "active".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: email.to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some(user.id.to_string()),
        }),
    };

    let event_type = "subscription.paused";
    handle_subscription_lost(&pool, &parsed, event_type).await;
    let saved_status = get_subscription_status_text(&pool, sub_id).await;

    assert_eq!(
        saved_status,
        Some("paused".to_string()),
        "expected a real subscription row to exist with status 'paused'"
    );

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn subscription_lost_expired() {
    let pool = test_pool().await;
    let email = "subscription_lost_expired_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let sub_id = "sub_lost_expired_test_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "active".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: email.to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some(user.id.to_string()),
        }),
    };

    let event_type = "subscription.expired";
    handle_subscription_lost(&pool, &parsed, event_type).await;
    let saved_status = get_subscription_status_text(&pool, sub_id).await;

    assert_eq!(
        saved_status,
        Some("expired".to_string()),
        "expected a real subscription row to exist with status 'expired'"
    );

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn subscription_lost_canceled_genuinely_new() {
    let pool = test_pool().await;
    let email = "subscription_lost_canceled_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let sub_id = "sub_lost_canceled_new_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "active".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "delivered@resend.dev".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some(user.id.to_string()),
        }),
    };

    let event_type = "subscription.canceled";
    handle_subscription_lost(&pool, &parsed, event_type).await;
    let saved_status = get_subscription_status_text(&pool, sub_id).await;

    assert_eq!(
        saved_status,
        Some("canceled".to_string()),
        "expected a real subscription row to exist with status 'canceled'"
    );

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn subscription_lost_canceled_already_canceled() {
    let pool = test_pool().await;
    let email = "subscription_lost_already_canceled_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let sub_id = "sub_lost_already_canceled_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "canceled".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "delivered@resend.dev".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some(user.id.to_string()),
        }),
    };

    upsert_subscription(&pool, user.id, &parsed, "canceled")
        .await
        .expect("expected to pre-seed the already-canceled subscription");

    let event_type = "subscription.canceled";
    handle_subscription_lost(&pool, &parsed, event_type).await;
    let saved_status = get_subscription_status_text(&pool, sub_id).await;

    assert_eq!(
        saved_status,
        Some("canceled".to_string()),
        "expected the subscription to genuinely remain 'canceled'"
    );

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn subscription_lost_missing_user_id() {
    let pool = test_pool().await;

    let sub_id = "sub_lost_missing_user_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "active".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "delivered@resend.dev".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: None,
        }),
    };

    let event_type = "subscription.paused";
    handle_subscription_lost(&pool, &parsed, event_type).await;

    let saved_row: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

    assert!(
        saved_row.is_none(),
        "expected NO subscription row to be created when safely_user_id is missing"
    );
}

#[tokio::test]
async fn subscription_lost_upsert_fails() {
    let pool = test_pool().await;

    let sub_id = "sub_lost_upsert_fails_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let fake_user_id = Uuid::new_v4();

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "active".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "delivered@resend.dev".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some(fake_user_id.to_string()),
        }),
    };

    let event_type = "subscription.expired";

    handle_subscription_lost(&pool, &parsed, event_type).await;

    let saved_row: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

    assert!(
        saved_row.is_none(),
        "expected NO subscription row to exist, since the foreign key genuinely failed"
    );
}

// Send Subscripton Ended Email Tests
#[tokio::test]
#[serial]
async fn send_subscription_ended_email_missing_api_key() {
    load_env_once();
    let original_key = var("RESEND_API_KEY").ok();
    unsafe {
        remove_var("RESEND_API_KEY");
    }

    let result = send_subscription_ended_email("delivered@resend.dev").await;
    match result {
        Err(AuthError::InternalServerError(_)) => {}
        Err(other) => panic!(
            "expected InternalServerError, got a different error: {:?}",
            other
        ),
        Ok(_) => panic!("expected the email to fail without a real API key, but it succeeded"),
    }

    unsafe {
        if let Some(key) = original_key {
            set_var("RESEND_API_KEY", key);
        }
    }
}

#[tokio::test]
#[serial]
async fn send_subscription_ended_email_missing_base_url() {
    load_env_once();
    let original_url = var("PUBLIC_BASE_URL").ok();
    unsafe {
        remove_var("PUBLIC_BASE_URL");
    }

    let result = send_subscription_ended_email("delivered@resend.dev").await;
    match result {
        Err(AuthError::InternalServerError(_)) => {}
        Err(other) => panic!(
            "expected InternalServerError, got a different error: {:?}",
            other
        ),
        Ok(_) => panic!("expected the email to fail without PUBLIC_BASE_URL, but it succeeded"),
    }

    unsafe {
        if let Some(url) = original_url {
            set_var("PUBLIC_BASE_URL", url);
        }
    }
}

#[tokio::test]
#[serial]
async fn send_subscription_ended_email_succeeds() {
    load_env_once();

    let result = send_subscription_ended_email("delivered@resend.dev").await;
    assert!(
        result.is_ok(),
        "expected the subscription-ended email to send successfully, got: {:?}",
        result
    );
}

#[tokio::test]
#[serial]
async fn send_subscription_ended_email_fails_for_a_blocked_domain() {
    load_env_once();

    let result = send_subscription_ended_email("test_user@example.com").await;
    match result {
        Err(AuthError::InternalServerError(message)) => {
            assert!(
                message.contains("Invalid `to` field"),
                "expected Resend's specific rejection message, got: {}",
                message
            )
        }
        other => panic!("expected an InternalServerError, got: {:?}", other),
    }
}

// Handle Subscription Update Tests
#[tokio::test]
async fn subscription_update_success() {
    let pool = test_pool().await;
    let email = "subscription_update_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let sub_id = "sub_update_test_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "past_due".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: email.to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some(user.id.to_string()),
        }),
    };

    handle_subscription_update(&pool, &parsed).await;
    let saved_status = get_subscription_status_text(&pool, sub_id).await;

    assert_eq!(
        saved_status,
        Some("past_due".to_string()),
        "expected the exact status from parsed.status to be saved, unchanged"
    );

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn subscription_update_missing_user_id() {
    let pool = test_pool().await;

    let sub_id = "sub_update_missing_user_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "active".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "test@example.com".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: None,
        }),
    };

    handle_subscription_update(&pool, &parsed).await;

    let saved_row: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

    assert!(
        saved_row.is_none(),
        "expected NO subscription row to be created when safely_user_id is missing"
    );
}

#[tokio::test]
async fn subscription_update_upsert_fails() {
    let pool = test_pool().await;

    let sub_id = "sub_update_upsert_fails_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let fake_user_id = Uuid::new_v4();

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "active".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
            billing_period: None,
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "test@example.com".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some(fake_user_id.to_string()),
        }),
    };

    handle_subscription_update(&pool, &parsed).await;

    let saved_row: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

    assert!(
        saved_row.is_none(),
        "expected NO subscription row to exist, since the foreign key genuinely failed"
    );
}

// Cancel Subscription Handler Tests
#[tokio::test]
async fn cancel_subscription_unauthorized() {
    let pool = test_pool().await;

    let headers = HeaderMap::new();

    let result = cancel_subscription_handler(State(pool), headers).await;
    match result {
        Err(BillingError::Unauthorized) => {}
        Err(other) => panic!("expected Unauthorized, got a different error: {:?}", other),
        Ok(_) => panic!("expected an unauthenticated request to be rejected, but it succeeded"),
    }
}

#[tokio::test]
async fn cancel_subscription_not_found() {
    let pool = test_pool().await;
    let email = "cancel_not_found_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;
    let result = cancel_subscription_handler(State(pool.clone()), headers).await;

    match result {
        Err(BillingError::NotFound(_)) => {}
        Err(other) => panic!("expected NotFound, got a different error: {:?}", other),
        Ok(_) => {
            panic!("expected cancellation to fail with no subscription to cancel, but it succeeded")
        }
    }

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn cancel_subscription_creem_rejects() {
    let pool = test_pool().await;
    let email = "cancel_creem_rejects_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let sub_id = "sub_creem_rejects_fake_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    insert_test_subscription(&pool, user.id, sub_id, "Team", "active").await;

    let result = cancel_subscription_handler(State(pool.clone()), headers).await;
    match result {
        Err(BillingError::ServiceUnavailable(_)) => {}
        Err(other) => panic!(
            "expected ServiceUnavailable, got a different error: {:?}",
            other
        ),
        Ok(_) => panic!("expected Creem to reject the cancellation, but it succeeded"),
    }

    let saved_status = get_subscription_status_text(&pool, sub_id).await;
    assert_eq!(
        saved_status,
        Some("active".to_string()),
        "expected the status to remain 'active', unchanged, since the cancellation genuinely failed"
    );

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}

// Cancel with Creem Tests
#[tokio::test]
#[serial]
async fn cancel_with_creem_missing_api_key() {
    load_env_once();

    let original_key = var("CREEM_API_KEY").ok();
    unsafe {
        remove_var("CREEM_API_KEY");
    }

    let result = cancel_with_creem("sub_does_not_matter_here").await;
    match result {
        Err(BillingError::InternalError(_)) => {}
        Err(other) => panic!("expected InternalError, got a different error: {:?}", other),
        Ok(_) => panic!("expected cancellation to fail without a real API key, but it succeeded"),
    }

    unsafe {
        if let Some(key) = original_key {
            set_var("CREEM_API_KEY", key);
        }
    }
}

#[tokio::test]
#[serial]
async fn cancel_with_creem_request_failed() {
    load_env_once();

    let original_base_url = var("CREEM_API_BASE_URL").ok();

    unsafe {
        set_var(
            "CREEM_API_BASE_URL",
            "http://this-domain-genuinely-does-not-exist-12345.invalid",
        );
    }

    let result = cancel_with_creem("sub_does_not_matter_here").await;

    match result {
        Err(BillingError::ServiceUnavailable(msg)) => {
            assert_eq!(
                msg, "Could not reach Creem",
                "expected the 'could not reach' message specifically, got: {}",
                msg
            );
        }
        Err(other) => panic!(
            "expected ServiceUnavailable, got a different error: {:?}",
            other
        ),
        Ok(_) => panic!("expected the request to genuinely fail, but it succeeded"),
    }

    unsafe {
        match original_base_url {
            Some(url) => set_var("CREEM_API_BASE_URL", url),
            None => remove_var("CREEM_API_BASE_URL"),
        }
    }
}

#[tokio::test]
#[serial]
async fn cancel_with_creem_rejected() {
    load_env_once();
    let result = cancel_with_creem("sub_definitely_does_not_exist_on_creem").await;

    match result {
        Err(BillingError::ServiceUnavailable(msg)) => {
            assert_eq!(
                msg, "Creem rejected the cancellation",
                "expected the 'rejected' message specifically, got: {}",
                msg
            );
        }
        Err(other) => panic!(
            "expected ServiceUnavailable, got a different error: {:?}",
            other
        ),
        Ok(_) => {
            panic!("expected Creem to genuinely reject a fake subscription ID, but it succeeded")
        }
    }
}

// Fetch Subscriber Email Tests
#[tokio::test]
async fn fetch_subscriber_email_found() {
    let pool = test_pool().await;
    let email = "fetch_subscriber_email_scenario_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let sub_id = "sub_fetch_email_scenario_001";
    cleanup_test_subscription(&pool, sub_id).await;
    insert_test_subscription(&pool, user.id, sub_id, "Team", "active").await;

    let result = fetch_subscriber_email(&pool, sub_id).await;
    assert_eq!(
        result,
        Some(email.to_string()),
        "expected the real subscriber's email to be found"
    );

    cleanup_test_subscription(&pool, sub_id).await;
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn fetch_subscriber_email_not_found() {
    let pool = test_pool().await;
    let sub_id = "sub_fetch_email_scenario_not_found_001";
    cleanup_test_subscription(&pool, sub_id).await;

    let result = fetch_subscriber_email(&pool, sub_id).await;
    assert!(
        result.is_none(),
        "expected None when no subscription matches this sub_id"
    );
}

#[tokio::test]
async fn fetch_subscriber_email_database_error_still_returns_none() {
    let pool = test_pool().await;
    pool.close().await;

    let result = fetch_subscriber_email(&pool, "doesnt_matter").await;
    assert!(
        result.is_none(),
        "expected None even on a genuine database error, since this function never hard-fails"
    );
}

// Send Subscription Cancel Email Tests
#[tokio::test]
#[serial]
async fn send_subscription_canceled_email_missing_api_key() {
    load_env_once();
    let original_key = var("RESEND_API_KEY").ok();
    unsafe {
        remove_var("RESEND_API_KEY");
    }

    let result = send_subscription_canceled_email("delivered@resend.dev").await;
    match result {
        Err(AuthError::InternalServerError(_)) => {}
        Err(other) => panic!(
            "expected InternalServerError, got a different error: {:?}",
            other
        ),
        Ok(_) => panic!("expected the email to fail without a real API key, but it succeeded"),
    }

    unsafe {
        if let Some(key) = original_key {
            set_var("RESEND_API_KEY", key);
        }
    }
}

#[tokio::test]
#[serial]
async fn send_subscription_canceled_email_missing_base_url() {
    load_env_once();

    let original_url = var("PUBLIC_BASE_URL").ok();
    unsafe {
        remove_var("PUBLIC_BASE_URL");
    }

    let result = send_subscription_canceled_email("delivered@resend.dev").await;
    match result {
        Err(AuthError::InternalServerError(_)) => {}
        Err(other) => panic!(
            "expected InternalServerError, got a different error: {:?}",
            other
        ),
        Ok(_) => panic!("expected the email to fail without PUBLIC_BASE_URL, but it succeeded"),
    }

    unsafe {
        if let Some(url) = original_url {
            set_var("PUBLIC_BASE_URL", url);
        }
    }
}

#[tokio::test]
#[serial]
async fn send_subscription_canceled_email_succeeds() {
    load_env_once();

    let result = send_subscription_canceled_email("delivered@resend.dev").await;
    assert!(
        result.is_ok(),
        "expected the subscription-canceled email to send successfully, got: {:?}",
        result
    );
}

#[tokio::test]
#[serial]
async fn send_subscription_canceled_email_fails_for_a_blocked_domain() {
    load_env_once();

    let result = send_subscription_canceled_email("test_user@example.com").await;
    match result {
        Err(AuthError::InternalServerError(message)) => {
            assert!(
                message.contains("Invalid `to` field"),
                "expected Resend's specific rejection message, got: {}",
                message
            )
        }
        other => panic!("expected an InternalServerError, got: {:?}", other),
    }
}

// Get Subscription Status
#[tokio::test]
async fn get_subscription_status_unauthorized() {
    let pool = test_pool().await;
    let headers = HeaderMap::new();

    let result = get_subscription_status(State(pool), headers).await;
    match result {
        Err(BillingError::Unauthorized) => {}
        Err(other) => panic!("expected Unauthorized, got a different error: {:?}", other),
        Ok(_) => panic!("expected an unauthenticated request to be rejected, but it succeeded"),
    }
}

#[tokio::test]
async fn get_subscription_status_no_subscription_reports_the_free_plan() {
    let pool = test_pool().await;
    let email = "get_status_no_sub_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;
    let result = get_subscription_status(State(pool.clone()), headers)
        .await
        .expect("expected the request itself to succeed, even with no subscription")
        .0;

    assert_eq!(result["plan_name"], json!(null));
    assert_eq!(result["billing_interval"], json!(null));
    assert_eq!(result["status"], json!(null));
    assert_eq!(result["current_period_end"], json!(null));
    assert_eq!(result["scheduled_plan_name"], json!(null));

    let usage = &result["usage"];
    assert_eq!(usage["plan"], json!("Free"));
    assert_eq!(usage["interval"], json!(null));
    assert_eq!(usage["used"], json!(0));
    assert_eq!(usage["limit"], json!(FREE_MONTHLY_SCANS));
    let expected_reset = sign_up_date_plus_months(&pool, user.id, 1).await;
    assert_eq!(
        usage["resets_on"],
        json!(expected_reset.to_string()),
        "expected Free scans to come back one month after sign-up, not on the 1st"
    );

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_subscription_status_no_scheduled_downgrade() {
    let pool = test_pool().await;
    let email = "get_status_no_downgrade_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let sub_id = "sub_status_no_downgrade_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let period_end = Utc::now() + Duration::days(20);

    insert_test_subscription_full(
        &pool,
        user.id,
        sub_id,
        "Team",
        "active",
        TestSubscriptionOptions {
            current_period_end: Some(period_end),
            scheduled_product_id: None,
            scheduled_plan_name: None,
        },
    )
    .await;

    let result = get_subscription_status(State(pool.clone()), headers)
        .await
        .expect("expected the request to succeed");

    assert_eq!(result["plan_name"], json!("Team"));
    assert_eq!(result["billing_interval"], json!("month"));
    assert_eq!(result["status"], json!("active"));
    assert_eq!(result["scheduled_plan_name"], json!(null));
    assert_eq!(result["usage"]["plan"], json!("Team"));
    assert_eq!(result["usage"]["limit"], json!(750));

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_subscription_status_reports_a_yearly_plan() {
    let pool = test_pool().await;
    let email = "get_status_yearly_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    insert_subscription_with_scans(
        &pool,
        user.id,
        "sub_status_yearly_001",
        TestPaidPlan {
            plan_name: "Enterprise",
            status: "active",
            billing_interval: "year",
            scans_used: 12,
            bought_days_ago: 3,
        },
    )
    .await;

    let result = get_subscription_status(State(pool.clone()), headers)
        .await
        .expect("expected the request to succeed");

    assert_eq!(result["plan_name"], json!("Enterprise"));
    assert_eq!(result["billing_interval"], json!("year"));
    assert_eq!(result["usage"]["interval"], json!("year"));
    assert_eq!(result["usage"]["used"], json!(12));
    assert_eq!(result["usage"]["limit"], json!(null));

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_subscription_status_downgrade_not_yet_due() {
    let pool = test_pool().await;
    let email = "get_status_downgrade_not_due_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let sub_id = "sub_status_downgrade_not_due_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let period_end = Utc::now() + Duration::days(20);

    insert_test_subscription_full(
        &pool,
        user.id,
        sub_id,
        "Enterprise",
        "active",
        TestSubscriptionOptions {
            current_period_end: Some(period_end),
            scheduled_product_id: Some("prod_team_scheduled"),
            scheduled_plan_name: Some("Team"),
        },
    )
    .await;

    let result = get_subscription_status(State(pool.clone()), headers)
        .await
        .expect("expected the request to succeed");

    assert_eq!(result["plan_name"], json!("Enterprise"));
    assert_eq!(result["status"], json!("active"));
    assert_eq!(result["scheduled_plan_name"], json!("Team"));

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_subscription_status_applies_a_due_downgrade_when_the_renewal_is_late() {
    // The paid period has ended but the renewal webhook hasn't arrived
    // yet: the status check switches to the scheduled plan itself.
    let pool = test_pool().await;
    let email = "get_status_downgrade_due_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let sub_id = "sub_status_downgrade_due_fake_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let period_end = Utc::now() - Duration::days(1);

    insert_test_subscription_full(
        &pool,
        user.id,
        sub_id,
        "Enterprise",
        "active",
        TestSubscriptionOptions {
            current_period_end: Some(period_end),
            scheduled_product_id: Some("prod_team_scheduled"),
            scheduled_plan_name: Some("Team"),
        },
    )
    .await;

    let result = get_subscription_status(State(pool.clone()), headers)
        .await
        .expect("expected the request to succeed");

    assert_eq!(
        result["plan_name"],
        json!("Team"),
        "expected the scheduled plan to be applied once the paid period has ended"
    );
    assert_eq!(result["status"], json!("active"));
    assert_eq!(result["scheduled_plan_name"], json!(null));

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}

// Apply Scheduled Downgrade If Due Tests
#[tokio::test]
async fn apply_scheduled_downgrade_missing_product_id() {
    let pool = test_pool().await;

    let result = apply_scheduled_downgrade_if_due(
        &pool,
        "sub_does_not_matter",
        None,
        Some("Team"),
        Some(Utc::now() - Duration::days(1)),
    )
    .await;

    assert!(
        result.is_none(),
        "expected None when scheduled_product_id is missing"
    );
}

#[tokio::test]
async fn apply_scheduled_downgrade_missing_plan_name() {
    let pool = test_pool().await;

    let result = apply_scheduled_downgrade_if_due(
        &pool,
        "sub_does_not_matter",
        Some("prod_team"),
        None,
        Some(Utc::now() - Duration::days(1)),
    )
    .await;

    assert!(
        result.is_none(),
        "expected None when scheduled_plan_name is missing"
    );
}

#[tokio::test]
async fn apply_scheduled_downgrade_missing_period_end() {
    let pool = test_pool().await;

    let result = apply_scheduled_downgrade_if_due(
        &pool,
        "sub_does_not_matter",
        Some("prod_team"),
        Some("Team"),
        None,
    )
    .await;

    assert!(
        result.is_none(),
        "expected None when current_period_end is missing"
    );
}

#[tokio::test]
async fn apply_scheduled_downgrade_not_yet_due() {
    let pool = test_pool().await;

    let result = apply_scheduled_downgrade_if_due(
        &pool,
        "sub_does_not_matter",
        Some("prod_team"),
        Some("Team"),
        Some(Utc::now() + Duration::days(20)),
    )
    .await;

    assert!(
        result.is_none(),
        "expected None when the deferred period hasn't ended yet"
    );
}

#[tokio::test]
async fn apply_scheduled_downgrade_switches_the_plan_locally_once_due() {
    // Creem was already told when the downgrade was scheduled, so this
    // only updates our own row - no Creem call that could fail.
    let pool = test_pool().await;
    let email = "apply_downgrade_due_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let sub_id = "sub_apply_downgrade_fake_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let period_end = Utc::now() - Duration::days(1);

    insert_test_subscription_full(
        &pool,
        user.id,
        sub_id,
        "Enterprise",
        "active",
        TestSubscriptionOptions {
            current_period_end: Some(period_end),
            scheduled_product_id: Some("prod_team_scheduled"),
            scheduled_plan_name: Some("Team"),
        },
    )
    .await;

    let result = apply_scheduled_downgrade_if_due(
        &pool,
        sub_id,
        Some("prod_team_scheduled"),
        Some("Team"),
        Some(period_end),
    )
    .await;

    assert_eq!(result, Some("Team".to_string()));

    let (plan_name, product_id, scheduled_plan_name): (String, String, Option<String>) = query_as(
        "SELECT plan_name, creem_product_id, scheduled_plan_name
             FROM subscriptions WHERE creem_subscription_id = $1",
    )
    .bind(sub_id)
    .fetch_one(&pool)
    .await
    .expect("expected the query itself to succeed");

    assert_eq!(plan_name, "Team");
    assert_eq!(product_id, "prod_team_scheduled");
    assert_eq!(
        scheduled_plan_name, None,
        "expected the schedule to be cleared"
    );

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}

// Change Creem Subscription Product Tests
#[tokio::test]
#[serial]
async fn change_creem_subscription_product_missing_api_key() {
    load_env_once();
    let original_key = var("CREEM_API_KEY").ok();
    unsafe {
        remove_var("CREEM_API_KEY");
    }

    let result = change_creem_subscription_product(
        "sub_doesnt_matter",
        "prod_doesnt_matter",
        "proration-charge-immediately",
    )
    .await;

    assert!(
        result.is_err(),
        "expected the request to fail without a real API key"
    );

    unsafe {
        if let Some(key) = original_key {
            set_var("CREEM_API_KEY", key);
        }
    }
}

#[tokio::test]
#[serial]
async fn change_creem_subscription_product_request_failed() {
    load_env_once();
    let original_base_url = var("CREEM_API_BASE_URL").ok();
    unsafe {
        set_var(
            "CREEM_API_BASE_URL",
            "http://this-domain-genuinely-does-not-exist-12345.invalid",
        );
    }

    let result = change_creem_subscription_product(
        "sub_doesnt_matter",
        "prod_doesnt_matter",
        "proration-charge-immediately",
    )
    .await;

    assert!(
        result.is_err(),
        "expected the request to genuinely fail against an unreachable host"
    );

    unsafe {
        match original_base_url {
            Some(url) => set_var("CREEM_API_BASE_URL", url),
            None => remove_var("CREEM_API_BASE_URL"),
        }
    }
}

#[tokio::test]
#[serial]
async fn change_creem_subscription_product_creem_rejects() {
    load_env_once();

    let result = change_creem_subscription_product(
        "sub_definitely_does_not_exist_on_creem",
        "prod_doesnt_matter",
        "proration-charge-immediately",
    )
    .await;

    match result {
        Err(message) => {
            assert!(
                message.contains("Creem rejected plan change"),
                "expected the specific rejection message, got: {}",
                message
            );
        }
        Ok(_) => {
            panic!("expected Creem to genuinely reject a fake subscription ID, but it succeeded")
        }
    }
}

// Change Plan Handler Tests
#[tokio::test]
async fn change_plan_unauthorized() {
    let pool = test_pool().await;
    let headers = HeaderMap::new();

    let body = ChangePlanBody {
        product_id: "prod_does_not_matter".to_string(),
    };

    let result = change_plan_handler(State(pool), headers, Json(body)).await;

    match result {
        Err(BillingError::Unauthorized) => {}
        Err(other) => panic!("expected Unauthorized, got a different error: {:?}", other),
        Ok(_) => panic!("expected an unauthenticated request to be rejected, but it succeeded"),
    }
}

#[tokio::test]
#[serial]
async fn change_plan_rejects_a_product_that_is_not_one_of_the_4_plans() {
    let pool = test_pool().await;
    let email = "change_plan_unknown_product_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let body = ChangePlanBody {
        product_id: "prod_does_not_exist".to_string(),
    };

    let result = change_plan_handler(State(pool.clone()), headers, Json(body)).await;

    match result {
        Err(BillingError::InvalidRequest(_)) => {}
        other => panic!("expected InvalidRequest, got: {:?}", other.map(|j| j.0)),
    }

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn change_plan_not_found() {
    let pool = test_pool().await;
    let email = "change_plan_not_found_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let body = ChangePlanBody {
        product_id: enterprise_product_id(),
    };

    let result = change_plan_handler(State(pool.clone()), headers, Json(body)).await;

    match result {
        Err(BillingError::NotFound(_)) => {}
        Err(other) => panic!("expected NotFound, got a different error: {:?}", other),
        Ok(_) => {
            panic!("expected the change to fail with no subscription to modify, but it succeeded")
        }
    }

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn change_plan_invalid_request() {
    let pool = test_pool().await;
    let email = "change_plan_invalid_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let sub_id = "sub_change_plan_invalid_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    insert_test_subscription(&pool, user.id, sub_id, "Team", "active").await;

    // Team monthly -> Team monthly: already on that plan.
    let body = ChangePlanBody {
        product_id: team_product_id(),
    };

    let result = change_plan_handler(State(pool.clone()), headers, Json(body)).await;

    match result {
        Err(BillingError::InvalidRequest(_)) => {}
        Err(other) => panic!(
            "expected InvalidRequest, got a different error: {:?}",
            other
        ),
        Ok(_) => panic!("expected a nonsensical plan change to be rejected, but it succeeded"),
    }

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn change_plan_monthly_to_yearly_goes_through_checkout_instead() {
    let pool = test_pool().await;
    let email = "change_plan_monthly_to_yearly_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;
    let (team_yearly_id, _) = yearly_product_ids();

    insert_test_subscription(&pool, user.id, "sub_change_plan_m2y_001", "Team", "active").await;

    let result = change_plan_handler(
        State(pool.clone()),
        headers,
        Json(ChangePlanBody {
            product_id: team_yearly_id,
        }),
    )
    .await;

    match result {
        Err(BillingError::InvalidRequest(msg)) => {
            assert!(msg.contains("checkout"), "got: {}", msg)
        }
        other => panic!("expected InvalidRequest, got: {:?}", other.map(|j| j.0)),
    }

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn change_plan_downgrade_success() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };
    load_env_once();

    let pool = test_pool().await;
    let email = "change_plan_downgrade_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let sub_id = "sub_change_plan_downgrade_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    insert_test_subscription(&pool, user.id, sub_id, "Enterprise", "active").await;

    // A local, fake Creem that accepts the plan change.
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/subscriptions/{}/upgrade", sub_id)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": sub_id })))
        .expect(1)
        .mount(&mock_server)
        .await;
    let original_base_url = var("CREEM_API_BASE_URL").ok();
    unsafe {
        set_var("CREEM_API_BASE_URL", mock_server.uri());
    }

    let team_id = team_product_id();
    let result = change_plan_handler(
        State(pool.clone()),
        headers,
        Json(ChangePlanBody {
            product_id: team_id.clone(),
        }),
    )
    .await;

    unsafe {
        match original_base_url {
            Some(url) => set_var("CREEM_API_BASE_URL", url),
            None => remove_var("CREEM_API_BASE_URL"),
        }
    }

    let result = result.expect("expected the downgrade to be scheduled successfully");
    assert_eq!(result.0, json!({ "applied": "scheduled" }));
    mock_server.verify().await;

    let (plan_name, scheduled_product_id, scheduled_plan_name): (
        String,
        Option<String>,
        Option<String>,
    ) = query_as(
        "SELECT plan_name, scheduled_product_id, scheduled_plan_name
         FROM subscriptions WHERE creem_subscription_id = $1",
    )
    .bind(sub_id)
    .fetch_one(&pool)
    .await
    .expect("expected the query itself to succeed");

    assert_eq!(
        plan_name, "Enterprise",
        "expected the CURRENT plan to remain Enterprise - the downgrade is only scheduled"
    );
    assert_eq!(scheduled_product_id, Some(team_id));
    assert_eq!(scheduled_plan_name, Some("Team".to_string()));

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn change_plan_downgrade_creem_rejects_and_nothing_stays_scheduled() {
    let pool = test_pool().await;
    let email = "change_plan_downgrade_rejected_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let sub_id = "sub_change_plan_downgrade_fake_001";
    cleanup_test_subscription(&pool, sub_id).await;
    insert_test_subscription(&pool, user.id, sub_id, "Enterprise", "active").await;

    let result = change_plan_handler(
        State(pool.clone()),
        headers,
        Json(ChangePlanBody {
            product_id: team_product_id(),
        }),
    )
    .await;

    match result {
        Err(BillingError::ServiceUnavailable(_)) => {}
        other => panic!("expected ServiceUnavailable, got: {:?}", other.map(|j| j.0)),
    }

    let (plan, _, _, scheduled, _, _) = subscription_row(&pool, sub_id).await;
    assert_eq!(plan, "Enterprise");
    assert_eq!(
        scheduled, None,
        "expected the schedule to be undone when Creem rejects the change"
    );

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn change_plan_upgrade_creem_rejects() {
    let pool = test_pool().await;
    let email = "change_plan_upgrade_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let sub_id = "sub_change_plan_upgrade_fake_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    insert_test_subscription(&pool, user.id, sub_id, "Team", "active").await;

    let body = ChangePlanBody {
        product_id: enterprise_product_id(),
    };

    let result = change_plan_handler(State(pool.clone()), headers, Json(body)).await;

    match result {
        Err(BillingError::ServiceUnavailable(_)) => {}
        Err(other) => panic!(
            "expected ServiceUnavailable, got a different error: {:?}",
            other
        ),
        Ok(_) => panic!("expected Creem to reject the upgrade, but it succeeded"),
    }

    let plan_name: String =
        query_scalar("SELECT plan_name FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_one(&pool)
            .await
            .expect("expected the query itself to succeed");

    assert_eq!(
        plan_name, "Team",
        "expected the plan to remain 'Team', unchanged, since the upgrade genuinely failed"
    );

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}

// Apply Upgrade Tests
#[tokio::test]
async fn apply_upgrade_creem_rejects() {
    let pool = test_pool().await;
    let email = "apply_upgrade_rejects_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let sub_id = "sub_apply_upgrade_fake_001";
    cleanup_test_subscription(&pool, sub_id).await;
    insert_test_subscription(&pool, user.id, sub_id, "Team", "active").await;

    let result = apply_upgrade(
        &pool,
        sub_id,
        "prod_enterprise_target",
        "Enterprise",
        BillingInterval::Month,
    )
    .await;
    match result {
        Err(BillingError::ServiceUnavailable(_)) => {}
        Err(other) => panic!("expected ServiceUnavailable, got: {:?}", other),
        Ok(_) => panic!("expected Creem to reject the upgrade, but it succeeded"),
    }

    let plan_name: String =
        query_scalar("SELECT plan_name FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_one(&pool)
            .await
            .expect("expected the query itself to succeed");

    assert_eq!(plan_name, "Team", "expected the plan to remain unchanged");

    cleanup_test_subscription(&pool, sub_id).await;
    cleanup_test_user(&pool, email).await;
}

// Get Product IDs Tests
#[tokio::test]
#[serial]
async fn get_product_ids_success() {
    load_env_once();

    let expected_team_id = var("CREEM_TEAM_PRODUCT_ID")
        .expect("expected CREEM_TEAM_PRODUCT_ID to be genuinely set for this test");
    let expected_enterprise_id = var("CREEM_ENTERPRISE_PRODUCT_ID")
        .expect("expected CREEM_ENTERPRISE_PRODUCT_ID to be genuinely set for this test");
    // Yearly IDs are optional: null until they're in .env.
    let expected_team_yearly = var("CREEM_TEAM_YEARLY_PRODUCT_ID").ok();
    let expected_enterprise_yearly = var("CREEM_ENTERPRISE_YEARLY_PRODUCT_ID").ok();

    let result = get_product_ids()
        .await
        .expect("expected the request to succeed with both monthly IDs present")
        .0;

    assert_eq!(
        result,
        json!({
            "Team": expected_team_id,
            "TeamYearly": expected_team_yearly,
            "Enterprise": expected_enterprise_id,
            "EnterpriseYearly": expected_enterprise_yearly,
        }),
        "expected the real, actual product IDs to be returned"
    );
}

#[tokio::test]
#[serial]
async fn get_product_ids_missing_team_id() {
    load_env_once();

    let original_team_id = var("CREEM_TEAM_PRODUCT_ID").ok();
    unsafe {
        remove_var("CREEM_TEAM_PRODUCT_ID");
    }

    let result = get_product_ids().await;

    match result {
        Err(BillingError::InternalError(_)) => {}
        Err(other) => panic!("expected InternalError, got a different error: {:?}", other),
        Ok(_) => {
            panic!("expected the request to fail without CREEM_TEAM_PRODUCT_ID, but it succeeded")
        }
    }

    unsafe {
        if let Some(id) = original_team_id {
            set_var("CREEM_TEAM_PRODUCT_ID", id);
        }
    }
}

#[tokio::test]
#[serial]
async fn get_product_ids_missing_enterprise_id() {
    load_env_once();

    let original_enterprise_id = var("CREEM_ENTERPRISE_PRODUCT_ID").ok();
    unsafe {
        remove_var("CREEM_ENTERPRISE_PRODUCT_ID");
    }

    let result = get_product_ids().await;

    match result {
        Err(BillingError::InternalError(_)) => {}
        Err(other) => panic!("expected InternalError, got a different error: {:?}", other),
        Ok(_) => panic!(
            "expected the request to fail without CREEM_ENTERPRISE_PRODUCT_ID, but it succeeded"
        ),
    }

    unsafe {
        if let Some(id) = original_enterprise_id {
            set_var("CREEM_ENTERPRISE_PRODUCT_ID", id);
        }
    }
}

// Mark Event Processed If New Tests
#[tokio::test]
async fn mark_event_processed_returns_true_for_a_genuinely_new_event() {
    let pool = test_pool().await;
    let event_id = "evt_genuinely_new_001";
    query("DELETE FROM webhook_events_processed WHERE event_id = $1")
        .bind(event_id)
        .execute(&pool)
        .await
        .ok();

    let result = mark_event_processed_if_new(&pool, event_id).await;
    assert!(
        result,
        "expected true for a genuinely new, never-before-seen event ID"
    );

    query("DELETE FROM webhook_events_processed WHERE event_id = $1")
        .bind(event_id)
        .execute(&pool)
        .await
        .ok();
}

#[tokio::test]
async fn mark_event_processed_returns_false_for_a_genuinely_repeated_event() {
    let pool = test_pool().await;
    let event_id = "evt_genuinely_repeated_001";
    query("DELETE FROM webhook_events_processed WHERE event_id = $1")
        .bind(event_id)
        .execute(&pool)
        .await
        .ok();

    let first_result = mark_event_processed_if_new(&pool, event_id).await;
    assert!(first_result, "expected the first call to genuinely be new");

    let second_result = mark_event_processed_if_new(&pool, event_id).await;
    assert!(
        !second_result,
        "expected the SAME event ID, seen a second time, to correctly report false"
    );

    query("DELETE FROM webhook_events_processed WHERE event_id = $1")
        .bind(event_id)
        .execute(&pool)
        .await
        .ok();
}

#[tokio::test]
async fn mark_event_processed_correctly_writes_a_real_row_to_the_database() {
    let pool = test_pool().await;
    let event_id = "evt_real_row_check_001";
    query("DELETE FROM webhook_events_processed WHERE event_id = $1")
        .bind(event_id)
        .execute(&pool)
        .await
        .ok();

    mark_event_processed_if_new(&pool, event_id).await;

    let row_exists: Option<String> =
        query_scalar("SELECT event_id FROM webhook_events_processed WHERE event_id = $1")
            .bind(event_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

    assert_eq!(
        row_exists,
        Some(event_id.to_string()),
        "expected a real, permanent row to genuinely exist in the database"
    );

    query("DELETE FROM webhook_events_processed WHERE event_id = $1")
        .bind(event_id)
        .execute(&pool)
        .await
        .ok();
}

#[tokio::test]
async fn mark_event_processed_treats_different_event_ids_as_genuinely_independent() {
    let pool = test_pool().await;
    let event_a = "evt_independent_a_001";
    let event_b = "evt_independent_b_001";
    query("DELETE FROM webhook_events_processed WHERE event_id IN ($1, $2)")
        .bind(event_a)
        .bind(event_b)
        .execute(&pool)
        .await
        .ok();

    let result_a = mark_event_processed_if_new(&pool, event_a).await;
    let result_b = mark_event_processed_if_new(&pool, event_b).await;

    assert!(result_a, "expected event A to be genuinely new");
    assert!(
        result_b,
        "expected event B to ALSO be genuinely new, unaffected by event A already being recorded"
    );

    query("DELETE FROM webhook_events_processed WHERE event_id IN ($1, $2)")
        .bind(event_a)
        .bind(event_b)
        .execute(&pool)
        .await
        .ok();
}

#[tokio::test]
async fn mark_event_processed_correctly_handles_two_genuinely_simultaneous_attempts() {
    let pool = test_pool().await;
    let event_id = "evt_concurrent_race_001";
    query("DELETE FROM webhook_events_processed WHERE event_id = $1")
        .bind(event_id)
        .execute(&pool)
        .await
        .ok();

    let pool_a = pool.clone();
    let pool_b = pool.clone();
    let event_id_a = event_id.to_string();
    let event_id_b = event_id.to_string();

    let (result_a, result_b) = tokio::join!(
        mark_event_processed_if_new(&pool_a, &event_id_a),
        mark_event_processed_if_new(&pool_b, &event_id_b)
    );

    let true_count = [result_a, result_b].iter().filter(|&&r| r).count();
    assert_eq!(
        true_count, 1,
        "expected EXACTLY one of the two genuinely simultaneous attempts to succeed, got: a={}, b={}",
        result_a, result_b
    );

    query("DELETE FROM webhook_events_processed WHERE event_id = $1")
        .bind(event_id)
        .execute(&pool)
        .await
        .ok();
}

// Scan Limit For Plan Tests
#[test]
fn team_plan_gets_750_scans_a_month() {
    assert_eq!(scan_limit_for_plan("Team"), Some(750));
}

#[test]
fn enterprise_plan_is_unlimited() {
    assert_eq!(scan_limit_for_plan("Enterprise"), None);
}

#[test]
fn an_unrecognized_plan_name_gets_zero_scans_rather_than_silently_being_unlimited() {
    assert_eq!(scan_limit_for_plan("SomeFuturePlan"), Some(0));
}

#[test]
fn the_free_plan_is_10_scans_a_month() {
    assert_eq!(FREE_MONTHLY_SCANS, 10);
}

fn dt(offset_days: i64) -> DateTime<Utc> {
    Utc::now() + Duration::days(offset_days)
}

#[test]
fn a_genuinely_brand_new_subscription_with_no_existing_row_always_counts_as_a_new_period() {
    assert!(is_new_billing_period(None, Some(dt(30))));
}

#[test]
fn a_genuinely_later_period_end_is_a_real_new_period() {
    let existing = Some(Some(dt(0)));
    let incoming = dt(30);
    assert!(is_new_billing_period(existing, Some(incoming)));
}

#[test]
fn the_same_period_end_repeated_is_never_treated_as_a_new_period() {
    let same = dt(0);
    assert!(!is_new_billing_period(Some(Some(same)), Some(same)));
}

#[test]
fn an_earlier_incoming_period_end_is_never_treated_as_a_new_period() {
    // Guards against an out-of-order or replayed webhook resetting
    // usage backwards.
    let existing = Some(Some(dt(30)));
    let incoming = dt(0);
    assert!(!is_new_billing_period(existing, Some(incoming)));
}

#[test]
fn a_genuinely_missing_incoming_period_end_is_never_treated_as_a_new_period() {
    let existing = Some(Some(dt(0)));
    assert!(!is_new_billing_period(existing, None));
}

#[test]
fn an_existing_row_with_a_genuinely_missing_stored_period_end_is_never_treated_as_a_new_period() {
    let existing = Some(None);
    assert!(!is_new_billing_period(existing, Some(dt(30))));
}

// Free Plan Scan Tests
#[tokio::test]
async fn free_plan_allows_exactly_10_scans_even_when_they_arrive_at_once() {
    let pool = test_pool().await;
    let email = "free_plan_concurrency_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let mut handles = Vec::new();
    for _ in 0..150 {
        let pool = pool.clone();
        let user_id = user.id;
        handles.push(tokio::spawn(async move {
            check_and_increment_scan_usage(&pool, user_id).await.is_ok()
        }));
    }
    let mut allowed = 0;
    for handle in handles {
        if handle.await.expect("expected the task to finish") {
            allowed += 1;
        }
    }
    assert_eq!(
        allowed, 10,
        "expected exactly 10 of 150 scans to be allowed"
    );

    let usage = get_scan_usage(&pool, user.id).await;
    assert_eq!(usage["plan"], json!("Free"));
    assert_eq!(usage["used"], json!(10));

    let expected_reset = sign_up_date_plus_months(&pool, user.id, 1).await;
    match check_and_increment_scan_usage(&pool, user.id).await {
        Err(ScanLimitError::FreeLimitReached { limit, resets_on }) => {
            assert_eq!(limit, 10);
            assert_eq!(
                resets_on, expected_reset,
                "expected the scans to come back one month after sign-up"
            );
        }
        other => panic!("expected FreeLimitReached, got: {:?}", other),
    }

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn free_scans_come_back_on_the_sign_up_day_not_the_first() {
    let pool = test_pool().await;
    let email = "free_plan_sign_up_day_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    query("UPDATE users SET created_at = NOW() - interval '40 days' WHERE id = $1")
        .bind(user.id)
        .execute(&pool)
        .await
        .expect("expected to move the sign-up date back");

    // All 10 used in the current Free month (which started on the
    // first monthly anniversary of the sign-up).
    query(
        "INSERT INTO free_scan_usage (user_id, period_start, scans_used)
         SELECT id, created_at + interval '1 month', 10 FROM users WHERE id = $1",
    )
    .bind(user.id)
    .execute(&pool)
    .await
    .expect("expected to use up the free scans");

    assert!(
        check_and_increment_scan_usage(&pool, user.id)
            .await
            .is_err(),
        "expected the 101st scan this month to be refused"
    );
    assert_eq!(
        get_scan_usage(&pool, user.id).await["resets_on"],
        json!(
            sign_up_date_plus_months(&pool, user.id, 2)
                .await
                .to_string()
        ),
        "expected the reset on the sign-up day of next month"
    );

    // The count belongs to the previous Free month -> a new month began.
    query(
        "UPDATE free_scan_usage SET period_start = period_start - interval '1 month'
         WHERE user_id = $1",
    )
    .bind(user.id)
    .execute(&pool)
    .await
    .expect("expected to move the count back a month");

    assert_eq!(get_scan_usage(&pool, user.id).await["used"], json!(0));
    assert!(check_and_increment_scan_usage(&pool, user.id).await.is_ok());
    assert_eq!(get_scan_usage(&pool, user.id).await["used"], json!(1));

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn a_past_due_subscription_falls_back_to_the_free_plan() {
    let pool = test_pool().await;
    let email = "past_due_uses_free_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    insert_test_subscription(&pool, user.id, "sub_past_due_free_001", "Team", "past_due").await;

    assert!(check_and_increment_scan_usage(&pool, user.id).await.is_ok());
    let usage = get_scan_usage(&pool, user.id).await;
    assert_eq!(usage["plan"], json!("Free"));
    assert_eq!(usage["used"], json!(1));

    cleanup_test_user(&pool, email).await;
}

// Paid Plan Scan Tests
#[tokio::test]
async fn team_monthly_stops_at_750_scans() {
    let pool = test_pool().await;
    let email = "team_monthly_limit_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    insert_subscription_with_scans(
        &pool,
        user.id,
        "sub_team_monthly_limit_001",
        TestPaidPlan {
            plan_name: "Team",
            status: "active",
            billing_interval: "month",
            scans_used: 749,
            bought_days_ago: 10,
        },
    )
    .await;

    assert!(check_and_increment_scan_usage(&pool, user.id).await.is_ok());
    match check_and_increment_scan_usage(&pool, user.id).await {
        Err(ScanLimitError::LimitReached { limit }) => assert_eq!(limit, 750),
        other => panic!("expected LimitReached, got: {:?}", other),
    }

    let free_rows: i64 = query_scalar("SELECT COUNT(*) FROM free_scan_usage WHERE user_id = $1")
        .bind(user.id)
        .fetch_one(&pool)
        .await
        .expect("expected the count to succeed");
    assert_eq!(free_rows, 0, "expected a paid plan never to use Free scans");

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn enterprise_is_unlimited() {
    let pool = test_pool().await;
    let email = "enterprise_unlimited_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    insert_subscription_with_scans(
        &pool,
        user.id,
        "sub_enterprise_unlimited_001",
        TestPaidPlan {
            plan_name: "Enterprise",
            status: "active",
            billing_interval: "month",
            scans_used: 100_000,
            bought_days_ago: 10,
        },
    )
    .await;

    assert!(check_and_increment_scan_usage(&pool, user.id).await.is_ok());
    assert_eq!(get_scan_usage(&pool, user.id).await["limit"], json!(null));

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn a_monthly_plan_does_not_reset_by_itself_only_on_renewal() {
    let pool = test_pool().await;
    let email = "monthly_no_self_reset_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let sub_id = "sub_monthly_no_self_reset_001";
    insert_subscription_with_scans(
        &pool,
        user.id,
        sub_id,
        TestPaidPlan {
            plan_name: "Team",
            status: "active",
            billing_interval: "month",
            scans_used: 750,
            bought_days_ago: 10,
        },
    )
    .await;
    query(
        "UPDATE subscriptions SET scan_period_start = scan_period_start - interval '1 month'
         WHERE creem_subscription_id = $1",
    )
    .bind(sub_id)
    .execute(&pool)
    .await
    .expect("expected to move the count back a month");

    assert!(
        check_and_increment_scan_usage(&pool, user.id)
            .await
            .is_err(),
        "expected a monthly plan to wait for its renewal webhook"
    );

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn yearly_team_gets_its_750_scans_back_every_month() {
    let pool = test_pool().await;
    let email = "yearly_team_monthly_reset_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let sub_id = "sub_yearly_team_reset_001";
    insert_subscription_with_scans(
        &pool,
        user.id,
        sub_id,
        TestPaidPlan {
            plan_name: "Team",
            status: "active",
            billing_interval: "year",
            scans_used: 750,
            bought_days_ago: 40,
        },
    )
    .await;

    // 750 used in the current scan month: refused.
    assert!(
        check_and_increment_scan_usage(&pool, user.id)
            .await
            .is_err()
    );

    // The count belongs to last scan month: a new month began.
    query(
        "UPDATE subscriptions SET scan_period_start = scan_period_start - interval '1 month'
         WHERE creem_subscription_id = $1",
    )
    .bind(sub_id)
    .execute(&pool)
    .await
    .expect("expected to move the count back a month");

    let usage = get_scan_usage(&pool, user.id).await;
    assert_eq!(usage["used"], json!(0));
    assert_eq!(usage["interval"], json!("year"));

    assert!(check_and_increment_scan_usage(&pool, user.id).await.is_ok());

    let (used, on_the_purchase_day): (i32, bool) = query_as(
        "SELECT scans_used_this_period,
                scan_period_start = scan_anchor + make_interval(months => 1)
         FROM subscriptions WHERE creem_subscription_id = $1",
    )
    .bind(sub_id)
    .fetch_one(&pool)
    .await
    .expect("expected the row to exist");
    assert_eq!(
        used, 1,
        "expected the new scan month to start counting at 1"
    );
    assert!(
        on_the_purchase_day,
        "expected the new scan month to start on the purchase day of the month"
    );

    cleanup_test_user(&pool, email).await;
}

// Insert Active Subscription Tests
#[tokio::test]
async fn insert_active_subscription_creates_a_real_row_thats_genuinely_active_with_zero_scans_used()
{
    let pool = test_pool().await;
    let email = "insert_active_subscription_create_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let sub_id = format!("test_sub_{}", user.id);
    cleanup_test_subscription(&pool, &sub_id).await;

    insert_active_subscription(&pool, user.id, "Team").await;

    let (plan_name, status, scans_used_this_period): (String, String, i32) = query_as(
        "SELECT plan_name, status::text, scans_used_this_period
         FROM subscriptions WHERE creem_subscription_id = $1",
    )
    .bind(&sub_id)
    .fetch_one(&pool)
    .await
    .expect("expected a real row to exist");

    assert_eq!(plan_name, "Team");
    assert_eq!(status, "active");
    assert_eq!(
        scans_used_this_period, 0,
        "expected a genuinely fresh subscription to start with zero scans used"
    );

    cleanup_test_subscription(&pool, &sub_id).await;
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn insert_active_subscription_ties_the_row_to_the_real_user_it_was_inserted_for() {
    let pool = test_pool().await;
    let email = "insert_active_subscription_user_link_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let sub_id = format!("test_sub_{}", user.id);
    cleanup_test_subscription(&pool, &sub_id).await;

    insert_active_subscription(&pool, user.id, "Enterprise").await;

    let saved_user_id: Uuid =
        query_scalar("SELECT user_id FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(&sub_id)
            .fetch_one(&pool)
            .await
            .expect("expected the row to exist");

    assert_eq!(
        saved_user_id, user.id,
        "expected the inserted row to be genuinely tied to the user it was created for"
    );

    cleanup_test_subscription(&pool, &sub_id).await;
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn insert_active_subscription_is_a_genuine_no_op_the_second_time_for_the_same_user() {
    let pool = test_pool().await;
    let email = "insert_active_subscription_conflict_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let sub_id = format!("test_sub_{}", user.id);
    cleanup_test_subscription(&pool, &sub_id).await;

    insert_active_subscription(&pool, user.id, "Team").await;
    insert_active_subscription(&pool, user.id, "Enterprise").await;

    let plan_name: String =
        query_scalar("SELECT plan_name FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(&sub_id)
            .fetch_one(&pool)
            .await
            .expect("expected the row to exist");

    assert_eq!(
        plan_name, "Team",
        "expected the ON CONFLICT DO NOTHING to leave the first row genuinely untouched"
    );

    let row_count: i64 =
        query_scalar("SELECT COUNT(*) FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(&sub_id)
            .fetch_one(&pool)
            .await
            .expect("expected the count query to succeed");

    assert_eq!(
        row_count, 1,
        "expected exactly one row, not a duplicate, for the same deterministic subscription id"
    );

    cleanup_test_subscription(&pool, &sub_id).await;
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[should_panic(expected = "expected to insert a real test subscription")]
async fn insert_active_subscription_panics_for_a_genuinely_nonexistent_user() {
    let pool = test_pool().await;
    let fake_user_id = Uuid::new_v4();
    let sub_id = format!("test_sub_{}", fake_user_id);
    cleanup_test_subscription(&pool, &sub_id).await;

    insert_active_subscription(&pool, fake_user_id, "Team").await;
}
