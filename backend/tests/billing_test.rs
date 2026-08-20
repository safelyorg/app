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
        CreateCheckoutBody, cancel_subscription_handler, cancel_with_creem,
        create_checkout_handler, creem_webhook, extract_metadata_user_id, fetch_subscriber_email,
        get_subscription_status, handle_subscription_granted, handle_subscription_lost,
        handle_subscription_past_due, handle_subscription_update, verify_and_parse_webhook,
    },
    models::billing::{ParsedCustomer, ParsedMetadata, ParsedProduct, ParsedSubscription},
    services::{
        auth::{create_session, find_or_create_user_by_email},
        billing::{CreateCheckoutError, create_checkout, upsert_subscription},
    },
};
use chrono::{Duration, Utc};
use hmac::{Hmac, KeyInit, Mac};
use reqwest::StatusCode;
use serde_json::json;
use serial_test::serial;
use sha2::Sha256;
use sqlx::{query, query_scalar};
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

#[tokio::test]
async fn subscription_granted_success() {
    let pool = test_pool().await;

    let email = "subscription_granted_test@example.com";
    cleanup_test_user(&pool, email).await;
    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

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
        },
        customer: ParsedCustomer {
            id: "cust_test".to_string(),
            email: email.to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some(user.id.to_string()), // the REAL user's ID
        }),
    };

    handle_subscription_granted(&pool, &parsed).await;

    let saved_status: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

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
async fn subscription_past_due_success() {
    let pool = test_pool().await;
    let email = "past_due_test_user@example.com";
    cleanup_test_user(&pool, email).await;

    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

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

    let saved_status: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

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
    cleanup_test_user(&pool, email).await;

    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

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

    let saved_status: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

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

#[tokio::test]
async fn subscription_lost_paused() {
    let pool = test_pool().await;
    let email = "subscription_lost_paused_test@example.com";
    cleanup_test_user(&pool, email).await;

    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

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

    let saved_status: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

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
    cleanup_test_user(&pool, email).await;

    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

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

    let saved_status: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

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
    cleanup_test_user(&pool, email).await;

    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

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

    let saved_status: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

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
    cleanup_test_user(&pool, email).await;

    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

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

    let saved_status: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

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

#[tokio::test]
async fn subscription_update_success() {
    let pool = test_pool().await;
    let email = "subscription_update_test@example.com";
    cleanup_test_user(&pool, email).await;

    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

    let sub_id = "sub_update_test_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let parsed = ParsedSubscription {
        id: sub_id.to_string(),
        status: "trialing".to_string(),
        current_period_end_date: None,
        canceled_at: None,
        product: ParsedProduct {
            id: "prod_test".to_string(),
            name: "Team".to_string(),
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

    let saved_status: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

    assert_eq!(
        saved_status,
        Some("trialing".to_string()),
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
        status: "trialing".to_string(),
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
        status: "trialing".to_string(),
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

#[tokio::test]
#[serial]
async fn creem_webhook_verification_failure_propagates() {
    dotenvy::dotenv().ok();

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
    dotenvy::dotenv().ok();

    let pool = test_pool().await;

    let secret = var("CREEM_WEBHOOK_SECRET")
        .expect("expected CREEM_WEBHOOK_SECRET to be genuinely set for this test");

    let raw_body = r#"{"id":"evt_refund_test_001","eventType":"refund.created","created_at":1700000000,"object":{}}"#;

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

    let result = creem_webhook(State(pool), headers, body)
        .await
        .expect("expected the whole webhook to succeed end-to-end");

    assert_eq!(result, StatusCode::OK);
}

#[tokio::test]
#[serial]
async fn creem_webhook_unrecognized_event_type_still_ok() {
    dotenvy::dotenv().ok();

    let pool = test_pool().await;

    let secret = var("CREEM_WEBHOOK_SECRET")
        .expect("expected CREEM_WEBHOOK_SECRET to be genuinely set for this test");

    let raw_body = r#"{"id":"evt_unknown_test_001","eventType":"some.future.event","created_at":1700000000,"object":{}}"#;

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

    let result = creem_webhook(State(pool), headers, body)
        .await
        .expect("expected an unrecognized event type to still succeed, not fail");

    assert_eq!(result, StatusCode::OK);
}

#[tokio::test]
#[serial]
async fn creem_webhook_inner_handler_failure_still_returns_ok() {
    dotenvy::dotenv().ok();

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
        r#"{{"id":"evt_inner_failure_001","eventType":"subscription.paid","created_at":1700000000,"object":{{"id":"{}","status":"active","current_period_end_date":null,"canceled_at":null,"product":{{"id":"prod_test","name":"Team"}},"customer":{{"id":"cust_test","email":"test@example.com"}},"metadata":{{"safely_user_id":"{}"}}}}}}"#,
        sub_id, fake_user_id
    );

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

    let sub_id = "sub_creem_rejects_fake_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    query(
        "INSERT INTO subscriptions (id, user_id, creem_subscription_id, creem_customer_id, creem_product_id, plan_name, status, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, 'active'::subscription_status, NOW(), NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(user.id)
    .bind(sub_id)
    .bind("cust_fake_test_001")
    .bind("prod_fake_test_001")
    .bind("Team")
    .execute(&pool)
    .await
    .expect("expected to pre-seed the fake-but-local subscription");

    let result = cancel_subscription_handler(State(pool.clone()), headers).await;

    match result {
        Err(BillingError::ServiceUnavailable(_)) => {}
        Err(other) => panic!(
            "expected ServiceUnavailable, got a different error: {:?}",
            other
        ),
        Ok(_) => panic!("expected Creem to reject the cancellation, but it succeeded"),
    }

    let saved_status: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

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

#[tokio::test]
#[serial]
async fn cancel_with_creem_missing_api_key() {
    dotenvy::dotenv().ok();

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
    dotenvy::dotenv().ok();

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

#[tokio::test]
async fn fetch_subscriber_email_found() {
    let pool = test_pool().await;
    let email = "fetch_subscriber_email_test@example.com";
    cleanup_test_user(&pool, email).await;

    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

    let sub_id = "sub_fetch_email_found_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    query(
        "INSERT INTO subscriptions (id, user_id, creem_subscription_id, creem_customer_id, creem_product_id, plan_name, status, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'active'::subscription_status, NOW(), NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(user.id)
    .bind(sub_id)
    .bind("cust_fake_test_001")
    .bind("prod_fake_test_001")
    .bind("Team")
    .execute(&pool)
    .await
    .expect("expected to pre-seed the subscription");

    let result = fetch_subscriber_email(&pool, sub_id).await;

    assert_eq!(
        result,
        Some(email.to_string()),
        "expected the real subscriber's email to be found"
    );

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn fetch_subscriber_email_not_found() {
    let pool = test_pool().await;

    let sub_id = "sub_fetch_email_not_found_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let result = fetch_subscriber_email(&pool, sub_id).await;

    assert!(
        result.is_none(),
        "expected None when no subscription matches this sub_id"
    );
}

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
async fn get_subscription_status_no_subscription() {
    let pool = test_pool().await;
    let email = "get_status_no_sub_test@example.com";
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

    let result = get_subscription_status(State(pool.clone()), headers)
        .await
        .expect("expected the request itself to succeed, even with no subscription")
        .0;

    assert_eq!(
        result,
        json!({
            "plan_name": null,
            "status": null,
            "current_period_end": null,
            "scheduled_plan_name": null,
        }),
        "expected the exact, hardcoded all-null response"
    );

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_subscription_status_no_scheduled_downgrade() {
    let pool = test_pool().await;
    let email = "get_status_no_downgrade_test@example.com";

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

    let sub_id = "sub_status_no_downgrade_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let period_end = Utc::now() + Duration::days(20);

    query(
        "INSERT INTO subscriptions (id, user_id, creem_subscription_id, creem_customer_id, creem_product_id, plan_name, status, current_period_end, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'active'::subscription_status, $7, NOW(), NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(user.id)
    .bind(sub_id)
    .bind("cust_fake_test_001")
    .bind("prod_fake_test_001")
    .bind("Team")
    .bind(period_end)
    .execute(&pool)
    .await
    .expect("expected to pre-seed the subscription");

    let result = get_subscription_status(State(pool.clone()), headers)
        .await
        .expect("expected the request to succeed");

    assert_eq!(result["plan_name"], json!("Team"));
    assert_eq!(result["status"], json!("active"));
    assert_eq!(result["scheduled_plan_name"], json!(null));

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_subscription_status_downgrade_not_yet_due() {
    let pool = test_pool().await;
    let email = "get_status_downgrade_not_due_test@example.com";
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

    let sub_id = "sub_status_downgrade_not_due_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let period_end = Utc::now() + Duration::days(20);

    query(
        "INSERT INTO subscriptions (
            id, user_id, creem_subscription_id, creem_customer_id, creem_product_id,
            plan_name, status, current_period_end, scheduled_product_id, scheduled_plan_name,
            created_at, updated_at
        )
         VALUES ($1, $2, $3, $4, $5, $6, 'active'::subscription_status, $7, $8, $9, NOW(), NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(user.id)
    .bind(sub_id)
    .bind("cust_fake_test_001")
    .bind("prod_enterprise_current")
    .bind("Enterprise")
    .bind(period_end)
    .bind("prod_team_scheduled")
    .bind("Team")
    .execute(&pool)
    .await
    .expect("expected to pre-seed the subscription");

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
async fn get_subscription_status_downgrade_due_but_creem_rejects() {
    let pool = test_pool().await;
    let email = "get_status_downgrade_due_test@example.com";
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

    let sub_id = "sub_status_downgrade_due_fake_001";
    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let period_end = Utc::now() - Duration::days(1);

    query(
        "INSERT INTO subscriptions (
            id, user_id, creem_subscription_id, creem_customer_id, creem_product_id,
            plan_name, status, current_period_end, scheduled_product_id, scheduled_plan_name,
            created_at, updated_at
        )
         VALUES ($1, $2, $3, $4, $5, $6, 'active'::subscription_status, $7, $8, $9, NOW(), NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(user.id)
    .bind(sub_id)
    .bind("cust_fake_test_001")
    .bind("prod_enterprise_current")
    .bind("Enterprise")
    .bind(period_end)
    .bind("prod_team_scheduled")
    .bind("Team")
    .execute(&pool)
    .await
    .expect("expected to pre-seed the subscription");

    let result = get_subscription_status(State(pool.clone()), headers)
        .await
        .expect("expected the request to still succeed, even though Creem rejected the downgrade");

    assert_eq!(
        result["plan_name"],
        json!("Enterprise"),
        "expected the ORIGINAL plan to still be reported, since the downgrade genuinely failed"
    );
    assert_eq!(result["status"], json!("active"));
    assert_eq!(
        result["scheduled_plan_name"],
        json!("Team"),
        "expected the schedule to remain, since it was never successfully applied"
    );

    query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");

    cleanup_test_user(&pool, email).await;
}
