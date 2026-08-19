mod common;

use crate::common::{cleanup_test_user, test_pool};
use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, HeaderValue},
};
use backend::{
    errors::billing::{BillingError, WebhookError},
    handlers::billing::{
        CreateCheckoutBody, create_checkout_handler, extract_metadata_user_id,
        verify_and_parse_webhook,
    },
    models::billing::{ParsedCustomer, ParsedMetadata, ParsedProduct, ParsedSubscription},
    services::{
        auth::{create_session, find_or_create_user_by_email},
        billing::{CreateCheckoutError, create_checkout},
    },
};
use hmac::{Hmac, KeyInit, Mac};
use serial_test::serial;
use sha2::Sha256;
use std::env::{remove_var, set_var, var};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[tokio::test]
#[serial]
async fn checkout_handler_success() {
    dotenvy::dotenv().ok();
    let pool = test_pool().await;
    let email = "checkout_handler@example.com";
    cleanup_test_user(&pool, email).await;

    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

    let real_session_token = create_session(&pool, user.id)
        .await
        .expect("expected to create a real session");

    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", real_session_token))
            .expect("expected to insert the header value"),
    );

    let checkout_body = CreateCheckoutBody {
        product_id: "prod_6qDjyvwKbCZvWTgIztzqz4".to_string(),
    };

    let result = create_checkout_handler(State(pool.clone()), headers, Json(checkout_body))
        .await
        .expect("expected to create the checkout");

    let checkout_url = result["checkout_url"]
        .as_str()
        .expect("expected checkout_url to be a real string");

    assert!(
        checkout_url.contains("creem.io"),
        "expected a genuine Creem checkout URL, got: {}",
        checkout_url
    );

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn checkout_handler_unauthorized() {
    dotenvy::dotenv().ok();
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
async fn checkout_handler_creem_rejects_invalid_product() {
    let pool = test_pool().await;
    let email = "checkout_rejected@example.com";
    cleanup_test_user(&pool, email).await;

    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

    let real_session_token = create_session(&pool, user.id)
        .await
        .expect("expected to create a real session");

    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", real_session_token))
            .expect("expected to insert the header value"),
    );

    let checkout_body = CreateCheckoutBody {
        product_id: "definitely_not_a_real_product_id".to_string(),
    };

    let result = create_checkout_handler(State(pool.clone()), headers, Json(checkout_body)).await;

    match result {
        Err(BillingError::InvalidRequest(_)) => {}
        Err(other) => panic!(
            "expected InvalidRequest (Creem rejection), got a different error: {:?}",
            other
        ),
        Ok(_) => panic!("expected Creem to reject a fake product_id, but it succeeded"),
    }

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn create_checkout_success() {
    let pool = test_pool().await;
    let email = "checkout_success@example.com";
    cleanup_test_user(&pool, email).await;

    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

    let result = create_checkout("prod_6qDjyvwKbCZvWTgIztzqz4", user.id)
        .await
        .expect("expected the checkout to be created successfully");

    assert!(
        result.checkout_url.contains("creem.io"),
        "expected a genuine Creem checkout URL, got: {}",
        result.checkout_url
    );

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn create_checkout_creem_rejected() {
    let pool = test_pool().await;
    let email = "checkout_creem_rejected@example.com";
    cleanup_test_user(&pool, email).await;

    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

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
    dotenvy::dotenv().ok();
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
    dotenvy::dotenv().ok();
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

#[tokio::test]
#[serial]
async fn creem_webhook_secret_missing() {
    dotenvy::dotenv().ok();

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
async fn creem_webhook_missing_signature() {
    dotenvy::dotenv().ok();

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
async fn creem_webhook_invalid_body() {
    dotenvy::dotenv().ok();

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
async fn creem_webhook_invalid_signature() {
    dotenvy::dotenv().ok();

    let mut headers = HeaderMap::new();
    headers.insert(
        "creem-signature",
        HeaderValue::from_str("clearly-wrong-signature-that-cannot-match")
            .expect("expected to insert the header value"),
    );

    // Valid UTF-8, valid-looking JSON
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
async fn creem_webhook_invalid_payload() {
    dotenvy::dotenv().ok();

    let secret = var("CREEM_WEBHOOK_SECRET")
        .expect("expected CREEM_WEBHOOK_SECRET to be genuinely set for this test");

    let raw_body = "not valid json{{{";

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("expected to build a real HMAC instance");
    mac.update(raw_body.as_bytes());

    let real_signature = hex::encode(mac.finalize().into_bytes());

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
async fn creem_webhook_success() {
    dotenvy::dotenv().ok();

    let secret = var("CREEM_WEBHOOK_SECRET")
        .expect("expected CREEM_WEBHOOK_SECRET to be genuinely set for this test");

    let raw_body = r#"{"id":"evt_test_success_001","eventType":"checkout.completed","created_at":1700000000,"object":{}}"#;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("expected to build a real HMAC instance");
    mac.update(raw_body.as_bytes());
    let real_signature = hex::encode(mac.finalize().into_bytes());

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
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "test@example.com".to_string(),
        },
        metadata: None, // <-- genuinely absent
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
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "test@example.com".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: None, // <-- present, but empty
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
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: "test@example.com".to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some("this-is-genuinely-not-a-uuid".to_string()), // <-- malformed
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
