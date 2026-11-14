// Cancel at the end of the paid period (Creem mode "scheduled"):
// the plan and the team keep working until the end date, "Keep my plan"
// undoes it, and Creem's subscription.canceled at the end moves everyone
// to Free. Creem is a fake server (wiremock); no email is sent
// (RESEND_API_KEY is removed for the test).

mod common;

use crate::common::{
    auth_headers_for, cleanup_test_user, create_test_user, insert_test_subscription, test_pool,
};
use axum::{Json, extract::State};
use backend::errors::billing::BillingError;
use backend::handlers::billing::{
    ChangePlanBody, cancel_subscription_handler, change_plan_handler, get_subscription_status,
    resume_subscription_handler,
};
use backend::models::billing::{ParsedCustomer, ParsedMetadata, ParsedProduct, ParsedSubscription};
use backend::services::features::billing::{handle_subscription_lost, handle_subscription_update};
use backend::services::features::results::sees_full_results;
use backend::services::features::team::{invite, plan_holder};
use chrono::{Duration, Utc};
use serial_test::serial;
use sqlx::{query, query_scalar};
use std::env::{remove_var, set_var, var};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, method, path},
};

const OWNER: &str = "cpe-owner@cancel-test.safely.sh";
const MEMBER: &str = "cpe-member@cancel-test.safely.sh";
const SUB: &str = "sub_cancel_period_end_test";

fn parsed(status: &str, user: uuid::Uuid, ends: &str) -> ParsedSubscription {
    ParsedSubscription {
        id: SUB.to_string(),
        status: status.to_string(),
        current_period_end_date: Some(ends.to_string()),
        canceled_at: None,
        product: ParsedProduct {
            id: var("CREEM_TEAM_PRODUCT_ID").unwrap_or_else(|_| "prod_team".into()),
            name: "Team".to_string(),
            billing_period: Some("every-month".to_string()),
        },
        customer: ParsedCustomer {
            id: "cust_cpe".to_string(),
            email: OWNER.to_string(),
        },
        metadata: Some(ParsedMetadata {
            safely_user_id: Some(user.to_string()),
        }),
    }
}

#[tokio::test]
#[serial]
async fn the_plan_works_until_the_end_of_the_period() {
    let pool = test_pool().await;
    let _ = query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(SUB)
        .execute(&pool)
        .await;
    let _ = query("DELETE FROM team_members WHERE email LIKE '%@cancel-test.safely.sh'")
        .execute(&pool)
        .await;
    cleanup_test_user(&pool, OWNER).await;
    cleanup_test_user(&pool, MEMBER).await;

    let creem = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/subscriptions/{SUB}/cancel")))
        .and(body_json(
            serde_json::json!({ "mode": "scheduled", "onExecute": "cancel" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .expect(1)
        .mount(&creem)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/subscriptions/{SUB}/resume")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .expect(1)
        .mount(&creem)
        .await;
    let saved_base = var("CREEM_API_BASE_URL").ok();
    let saved_key = var("CREEM_API_KEY").ok();
    let saved_resend = var("RESEND_API_KEY").ok();
    unsafe {
        set_var("CREEM_API_BASE_URL", creem.uri());
        set_var("CREEM_API_KEY", "test_key");
        remove_var("RESEND_API_KEY");
    }

    let (owner, _) = create_test_user(&pool, OWNER).await;
    let (member, _) = create_test_user(&pool, MEMBER).await;
    insert_test_subscription(&pool, owner.id, SUB, "Team", "active").await;
    let period_end = Utc::now() + Duration::days(20);
    query("UPDATE subscriptions SET current_period_end = $2 WHERE creem_subscription_id = $1")
        .bind(SUB)
        .bind(period_end)
        .execute(&pool)
        .await
        .unwrap();
    invite(&pool, owner.id, MEMBER).await.unwrap();

    // Cancel: still active, ending on the period's last day.
    let headers = auth_headers_for(&pool, owner.id).await;
    let answer = cancel_subscription_handler(State(pool.clone()), headers.clone())
        .await
        .expect("cancel");
    assert!(answer["cancel_at"].is_string());
    let status: String =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(SUB)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "active");
    // The owner and the member keep the full plan until then.
    assert!(sees_full_results(&pool, owner.id).await);
    assert_eq!(plan_holder(&pool, member.id).await, owner.id);

    // The dashboard shows the end date, to the owner and to the member.
    let shown = get_subscription_status(State(pool.clone()), headers.clone())
        .await
        .unwrap();
    assert!(shown["cancel_at"].is_string());
    let member_headers = auth_headers_for(&pool, member.id).await;
    let shown = get_subscription_status(State(pool.clone()), member_headers)
        .await
        .unwrap();
    assert!(shown["shared_plan"]["cancel_at"].is_string());

    // Cancel again: nothing new is sent to Creem (expect(1) above).
    let _ = cancel_subscription_handler(State(pool.clone()), headers.clone())
        .await
        .unwrap();

    // No plan change while it is ending.
    let change = change_plan_handler(
        State(pool.clone()),
        headers.clone(),
        Json(ChangePlanBody {
            product_id: var("CREEM_ENTERPRISE_PRODUCT_ID").unwrap_or_default(),
        }),
    )
    .await;
    assert!(matches!(change, Err(BillingError::Conflict(_))));

    // Keep my plan.
    let _ = resume_subscription_handler(State(pool.clone()), headers.clone())
        .await
        .expect("resume");
    let ending: bool = query_scalar(
        "SELECT cancel_at IS NOT NULL FROM subscriptions WHERE creem_subscription_id = $1",
    )
    .bind(SUB)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!ending);

    // Creem's own messages: canceled for the end of the period (from its
    // customer portal), then the real end.
    let ends = period_end.to_rfc3339();
    handle_subscription_update(&pool, &parsed("scheduled_cancel", owner.id, &ends)).await;
    let (status, ending): (String, bool) = sqlx::query_as(
        "SELECT status::text, cancel_at IS NOT NULL FROM subscriptions WHERE creem_subscription_id = $1",
    )
    .bind(SUB)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((status.as_str(), ending), ("active", true));

    handle_subscription_lost(
        &pool,
        &parsed("canceled", owner.id, &ends),
        "subscription.canceled",
    )
    .await;
    assert!(!sees_full_results(&pool, owner.id).await);
    // The member moves to Free too (uses their own account again).
    assert_eq!(plan_holder(&pool, member.id).await, member.id);

    unsafe {
        match saved_base {
            Some(v) => set_var("CREEM_API_BASE_URL", v),
            None => remove_var("CREEM_API_BASE_URL"),
        }
        match saved_key {
            Some(v) => set_var("CREEM_API_KEY", v),
            None => remove_var("CREEM_API_KEY"),
        }
        if let Some(v) = saved_resend {
            set_var("RESEND_API_KEY", v);
        }
    }
    let _ = query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(SUB)
        .execute(&pool)
        .await;
    let _ = query("DELETE FROM team_members WHERE email LIKE '%@cancel-test.safely.sh'")
        .execute(&pool)
        .await;
    cleanup_test_user(&pool, OWNER).await;
    cleanup_test_user(&pool, MEMBER).await;
}

#[test]
fn the_emails_say_when_the_plan_ends() {
    let tera = backend::services::features::templates::get_tera();
    let mut c = tera::Context::new();
    c.insert("dashboard_url", "https://safely.sh/dashboard/");
    c.insert("logo_url", "https://safely.sh/images/white_logo.png");
    c.insert("ends_on", &Some("Dec 13, 2026"));
    c.insert("members", &1);
    let owner = tera.render("subscription_canceled_email.html", &c).unwrap();
    assert!(owner.contains("stays active until"));
    assert!(owner.contains("Dec 13, 2026"));
    assert!(owner.contains("for you and your team"));
    // Ended at once (no date): the old text.
    c.insert("ends_on", &None::<String>);
    let ended = tera.render("subscription_canceled_email.html", &c).unwrap();
    assert!(ended.contains("Premium access has ended"));

    let mut m = tera::Context::new();
    m.insert("dashboard_url", "https://safely.sh/dashboard/");
    m.insert("logo_url", "https://safely.sh/images/white_logo.png");
    m.insert("owner_email", "boss@company.com");
    m.insert("plan", "Team");
    m.insert("ends_on", "Dec 13, 2026");
    let member = tera.render("team_plan_ending_email.html", &m).unwrap();
    assert!(member.contains("boss@company.com"));
    assert!(member.contains("Dec 13, 2026"));
    assert!(member.contains("Free plan"));
}
