// "Manage card" (handlers/billing.rs manage_card_handler): a link to the
// person's Creem customer portal, where they change the card for their
// plan and extra seats. Only for someone who pays for a plan. Creem is a
// fake server (wiremock).

mod common;

use crate::common::{
    auth_headers_for, cleanup_test_user, create_test_user, insert_test_subscription, test_pool,
};
use axum::extract::State;
use backend::errors::billing::BillingError;
use backend::handlers::billing::manage_card_handler;
use serde_json::json;
use serial_test::serial;
use sqlx::query;
use std::env::{remove_var, set_var, var};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, method, path},
};

const OWNER: &str = "mc-owner@manage-card-test.safely.sh";
const FREE: &str = "mc-free@manage-card-test.safely.sh";
const SUB: &str = "sub_manage_card_test";

#[tokio::test]
#[serial]
async fn manage_card_opens_the_creem_portal() {
    let pool = test_pool().await;
    let _ = query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(SUB)
        .execute(&pool)
        .await;
    cleanup_test_user(&pool, OWNER).await;
    cleanup_test_user(&pool, FREE).await;

    let creem = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/customers/billing"))
        .and(body_json(json!({ "customer_id": "cust_fake_test_001" })))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "customer_portal_link": "https://creem.io/portal/test" })),
        )
        .expect(1)
        .mount(&creem)
        .await;
    let saved_base = var("CREEM_API_BASE_URL").ok();
    let saved_key = var("CREEM_API_KEY").ok();
    unsafe {
        set_var("CREEM_API_BASE_URL", creem.uri());
        set_var("CREEM_API_KEY", "test_key");
    }

    let (owner, _) = create_test_user(&pool, OWNER).await;
    let (free, _) = create_test_user(&pool, FREE).await;
    insert_test_subscription(&pool, owner.id, SUB, "Team", "active").await;

    let headers = auth_headers_for(&pool, owner.id).await;
    let answer = manage_card_handler(State(pool.clone()), headers)
        .await
        .expect("the portal link");
    assert_eq!(answer["url"], "https://creem.io/portal/test");

    // Nobody who never paid has a card here.
    let headers = auth_headers_for(&pool, free.id).await;
    assert!(matches!(
        manage_card_handler(State(pool.clone()), headers).await,
        Err(BillingError::NotFound(_))
    ));

    unsafe {
        match saved_base {
            Some(v) => set_var("CREEM_API_BASE_URL", v),
            None => remove_var("CREEM_API_BASE_URL"),
        }
        match saved_key {
            Some(v) => set_var("CREEM_API_KEY", v),
            None => remove_var("CREEM_API_KEY"),
        }
    }
    let _ = query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(SUB)
        .execute(&pool)
        .await;
    cleanup_test_user(&pool, OWNER).await;
    cleanup_test_user(&pool, FREE).await;
}
