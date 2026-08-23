use axum::Json;
use backend::{
    errors::auth::AuthError,
    handlers::subscribe::{NewsletterSubscribeRequest, newsletter_subscribe},
    services::email::subscribe_to_newsletter,
};
use dotenvy::var;
use serde_json::json;
use serial_test::serial;
use std::env::{remove_var, set_var};

#[tokio::test]
async fn newsletter_subscribe_bad_request_invalid_email() {
    let request = NewsletterSubscribeRequest {
        email: "not-an-email".to_string(),
    };

    let result = newsletter_subscribe(Json(request)).await;

    match result {
        Err(AuthError::BadRequest) => {}
        Err(other) => panic!("expected BadRequest, got a different error: {:?}", other),
        Ok(_) => panic!("expected an invalid email to be rejected, but it succeeded"),
    }
}

#[tokio::test]
async fn newsletter_subscribe_success() {
    dotenvy::dotenv().ok();

    let request = NewsletterSubscribeRequest {
        email: "newsletter_test_user@example.com".to_string(),
    };

    let result = newsletter_subscribe(Json(request))
        .await
        .expect("expected the subscription to succeed")
        .0;

    assert_eq!(result["success"], json!(true));
    assert_eq!(
        result["message"],
        json!("You're subscribed! Check your inbox to confirm.")
    );
}

#[tokio::test]
#[serial]
async fn subscribe_to_newsletter_missing_api_key() {
    dotenvy::dotenv().ok();
    let original_key = var("KIT_API_KEY").ok();
    unsafe {
        remove_var("KIT_API_KEY");
    }

    let result = subscribe_to_newsletter("newsletter_test_missing_key@example.com").await;

    match result {
        Err(AuthError::InternalServerError(_)) => {}
        Err(other) => panic!(
            "expected InternalServerError, got a different error: {:?}",
            other
        ),
        Ok(_) => {
            panic!("expected the subscription to fail without a real API key, but it succeeded")
        }
    }

    unsafe {
        if let Some(key) = original_key {
            set_var("KIT_API_KEY", key);
        }
    }
}

#[tokio::test]
#[serial]
async fn subscribe_to_newsletter_kit_rejects_wrong_key() {
    dotenvy::dotenv().ok();
    let original_key = var("KIT_API_KEY").ok();
    unsafe {
        set_var("KIT_API_KEY", "genuinely_wrong_api_key_00000");
    }

    let result = subscribe_to_newsletter("newsletter_test_wrong_key@example.com").await;

    match result {
        Err(AuthError::InternalServerError(_)) => {}
        Err(other) => panic!(
            "expected InternalServerError, got a different error: {:?}",
            other
        ),
        Ok(_) => panic!("expected Kit to reject a wrong API key, but it succeeded"),
    }

    unsafe {
        match original_key {
            Some(key) => set_var("KIT_API_KEY", key),
            None => remove_var("KIT_API_KEY"),
        }
    }
}

#[tokio::test]
#[serial]
async fn subscribe_to_newsletter_success() {
    dotenvy::dotenv().ok();

    let result = subscribe_to_newsletter("newsletter_test_service_success@example.com").await;

    assert!(
        result.is_ok(),
        "expected the subscription to genuinely succeed, got: {:?}",
        result
    );
}
