// Extra seats (services/features/team.rs set_extra_seats): $25 a month
// each. Adding any number is one Creem checkout. Removing takes seats
// from the newest purchase first: a purchase whose seats all go is
// cancelled at once, otherwise its seat count is lowered with no
// proration. Never a refund. The 3 plan seats always stay. Creem is a
// fake server (wiremock).

mod common;

use crate::common::{cleanup_test_user, create_test_user, insert_test_subscription, test_pool};
use backend::services::features::team::{
    SeatChange, TeamError, handle_seat_webhook, set_extra_seats, team_view,
};
use serde_json::{Value, json};
use serial_test::serial;
use sqlx::{query, query_scalar};
use std::env::{remove_var, set_var, var};
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, method, path, query_param},
};

const OWNER: &str = "sr-owner@seat-remove-test.safely.sh";
const PLAN: &str = "sub_seat_remove_plan";
const PRODUCT: &str = "prod_seat_remove_test";

/// One seat purchase with `units` seats.
fn purchase(id: &str, owner: Uuid, units: i64) -> Value {
    json!({
        "id": id,
        "status": "active",
        "product": { "id": PRODUCT },
        "items": [{ "id": format!("item_{id}"), "product_id": PRODUCT, "price_id": "pprice_1", "units": units }],
        "metadata": { "safely_user_id": owner.to_string() },
    })
}

async fn cleanup(pool: &sqlx::Pool<sqlx::Postgres>) {
    let _ = query("DELETE FROM team_extra_seats WHERE creem_subscription_id LIKE 'sub_sr_%'")
        .execute(pool)
        .await;
    let _ = query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(PLAN)
        .execute(pool)
        .await;
    cleanup_test_user(pool, OWNER).await;
}

async fn status(pool: &sqlx::Pool<sqlx::Postgres>, id: &str) -> (String, i32) {
    sqlx::query_as("SELECT status, seats FROM team_extra_seats WHERE creem_subscription_id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
#[serial]
async fn removing_seats_newest_first_without_a_refund() {
    let pool = test_pool().await;
    cleanup(&pool).await;
    let saved: Vec<(&str, Option<String>)> = [
        "CREEM_API_BASE_URL",
        "CREEM_API_KEY",
        "CREEM_EXTRA_SEAT_PRODUCT_ID",
        "CREEM_EXTRA_SEAT_YEARLY_PRODUCT_ID",
    ]
    .into_iter()
    .map(|k| (k, var(k).ok()))
    .collect();

    let (owner, _) = create_test_user(&pool, OWNER).await;
    let creem = MockServer::start().await;
    // Newest purchase: 3 seats. Remove 1 -> lowered to 2, no proration.
    Mock::given(method("GET"))
        .and(path("/v1/subscriptions"))
        .and(query_param("subscription_id", "sub_sr_new"))
        .respond_with(ResponseTemplate::new(200).set_body_json(purchase("sub_sr_new", owner.id, 3)))
        .mount(&creem)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/subscriptions/sub_sr_new"))
        .and(body_json(json!({
            "items": [{ "id": "item_sub_sr_new", "price_id": "pprice_1", "units": 2 }],
            "update_behavior": "proration-none",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&creem)
        .await;
    // Then remove 3: the newest purchase's 2 seats (cancelled) + 1 of the
    // older purchase's 1 seat (cancelled).
    for id in ["sub_sr_new", "sub_sr_old"] {
        Mock::given(method("POST"))
            .and(path(format!("/v1/subscriptions/{id}/cancel")))
            .and(body_json(json!({ "mode": "immediate" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(1)
            .mount(&creem)
            .await;
    }
    // Never a refund.
    Mock::given(method("POST"))
        .and(path("/v1/refunds"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&creem)
        .await;
    unsafe {
        set_var("CREEM_API_BASE_URL", creem.uri());
        set_var("CREEM_API_KEY", "test_key");
        set_var("CREEM_EXTRA_SEAT_PRODUCT_ID", PRODUCT);
        remove_var("CREEM_EXTRA_SEAT_YEARLY_PRODUCT_ID");
    }

    insert_test_subscription(&pool, owner.id, PLAN, "Team", "active").await;
    // First 1 seat, later 3 more at once: 3 + 4 = 7 seats.
    for (id, units) in [("sub_sr_old", 1), ("sub_sr_new", 3)] {
        assert!(
            handle_seat_webhook(&pool, "subscription.paid", &purchase(id, owner.id, units)).await
        );
        query("UPDATE team_extra_seats SET created_at = clock_timestamp() WHERE creem_subscription_id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    assert_eq!(team_view(&pool, owner.id).await.unwrap()["seat_limit"], 7);

    // Remove 1 (7 -> 6).
    assert_eq!(
        set_extra_seats(&pool, owner.id, 3).await,
        Ok(SeatChange::Removed(1))
    );
    assert_eq!(status(&pool, "sub_sr_new").await, ("active".to_string(), 2));

    // Remove 3 (6 -> 3): back to the 3 plan seats.
    assert_eq!(
        set_extra_seats(&pool, owner.id, 0).await,
        Ok(SeatChange::Removed(3))
    );
    assert_eq!(status(&pool, "sub_sr_new").await.0, "canceled");
    assert_eq!(status(&pool, "sub_sr_old").await.0, "canceled");
    let view = team_view(&pool, owner.id).await.unwrap();
    assert_eq!(view["extra_seats"], 0);
    assert_eq!(view["seat_limit"], 3);
    assert_eq!(
        set_extra_seats(&pool, owner.id, 0).await,
        Ok(SeatChange::Unchanged)
    );

    // A late Creem event never brings removed seats back.
    handle_seat_webhook(
        &pool,
        "subscription.update",
        &purchase("sub_sr_old", owner.id, 1),
    )
    .await;
    assert_eq!(team_view(&pool, owner.id).await.unwrap()["extra_seats"], 0);
    let active: i64 = query_scalar(
        "SELECT COUNT(*) FROM team_extra_seats WHERE owner_id = $1 AND status = 'active'",
    )
    .bind(owner.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active, 0);

    // While the plan itself is ending, no new seats.
    query("UPDATE subscriptions SET cancel_at = NOW() + INTERVAL '5 days' WHERE creem_subscription_id = $1")
        .bind(PLAN)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        set_extra_seats(&pool, owner.id, 2).await,
        Err(TeamError::PlanEnding)
    );

    unsafe {
        for (k, v) in saved {
            match v {
                Some(v) => set_var(k, v),
                None => remove_var(k),
            }
        }
    }
    cleanup(&pool).await;
}
