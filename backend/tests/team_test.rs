// Team plans (services/features/team.rs, handlers/team.rs): who can
// invite, how many seats each plan has, a member's scans counting on the
// owner's plan, and extra seats bought at Creem. Uses the real test
// database, like billing_test.rs.

mod common;

use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
    extract::State,
    http::{HeaderMap, HeaderValue, Request, StatusCode},
};
use backend::handlers::billing::creem_webhook;
use backend::routes::billing::billing_routes;
use backend::services::features::billing::{ScanCharge, get_scan_usage, reserve_scan};
use backend::services::features::team::{
    SeatChange, TeamError, handle_seat_webhook, invite, leave, leave_team_if_paying, plan_holder,
    remove, set_extra_seats, sync_extra_seats, team_view,
};
use common::{
    auth_headers_for, cleanup_test_user, compute_creem_signature, create_test_user,
    insert_test_subscription, load_env_once, test_pool,
};
use serde_json::{Value, json};
use serial_test::serial;
use sqlx::{Pool, Postgres, query, query_scalar};
use std::env::{remove_var, set_var, var};
use std::sync::Once;
use tower::ServiceExt;
use uuid::Uuid;

const SEAT_PRODUCT: &str = "prod_test_extra_seat";

static SETUP: Once = Once::new();

/// The extra seat product, and a Creem address that answers at once
/// with "not reachable" (no test talks to the real Creem).
fn setup() {
    load_env_once();
    SETUP.call_once(|| unsafe {
        set_var("CREEM_EXTRA_SEAT_PRODUCT_ID", SEAT_PRODUCT);
        remove_var("CREEM_EXTRA_SEAT_YEARLY_PRODUCT_ID");
        set_var("CREEM_API_BASE_URL", "http://127.0.0.1:9");
    });
}

struct Team {
    pool: Pool<Postgres>,
    owner: Uuid,
    emails: Vec<String>,
}

impl Team {
    /// An owner on `plan` ("Team", "Enterprise" or "" for Free).
    async fn new(tag: &str, plan: &str) -> Team {
        setup();
        let pool = test_pool().await;
        let owner_email = format!("team-owner-{tag}@team-test.safely.sh");
        let (owner, _) = create_test_user(&pool, &owner_email).await;
        if !plan.is_empty() {
            insert_test_subscription(&pool, owner.id, &format!("sub_team_{tag}"), plan, "active")
                .await;
        }
        Team {
            pool,
            owner: owner.id,
            emails: vec![owner_email],
        }
    }

    fn email(&mut self, name: &str) -> String {
        let email = format!("{name}@team-test.safely.sh");
        self.emails.push(email.clone());
        email
    }

    async fn cleanup(self) {
        for email in &self.emails {
            let _ = query("DELETE FROM team_members WHERE email = $1")
                .bind(email)
                .execute(&self.pool)
                .await;
            cleanup_test_user(&self.pool, email).await;
        }
    }
}

#[tokio::test]
#[serial]
async fn free_has_no_invites() {
    let mut t = Team::new("free", "").await;
    let ali = t.email("free-ali");
    assert_eq!(
        invite(&t.pool, t.owner, &ali).await,
        Err(TeamError::NeedsTeamPlan)
    );
    let view = team_view(&t.pool, t.owner).await.unwrap();
    assert_eq!(view["seat_limit"], 1);
    assert_eq!(view["plan"], "Free");
    t.cleanup().await;
}

#[tokio::test]
#[serial]
async fn team_has_three_seats_with_the_owner() {
    let mut t = Team::new("three", "Team").await;
    let (a, b, c) = (t.email("three-a"), t.email("three-b"), t.email("three-c"));
    assert_eq!(invite(&t.pool, t.owner, &a.to_uppercase()).await, Ok(()));
    assert_eq!(invite(&t.pool, t.owner, &b).await, Ok(()));
    assert_eq!(
        invite(&t.pool, t.owner, &c).await,
        Err(TeamError::NoSeatsLeft)
    );
    assert_eq!(
        invite(&t.pool, t.owner, &t.emails[0].clone()).await,
        Err(TeamError::OwnEmail)
    );
    assert_eq!(
        invite(&t.pool, t.owner, "not an email").await,
        Err(TeamError::InvalidEmail)
    );

    let view = team_view(&t.pool, t.owner).await.unwrap();
    assert_eq!(view["role"], "owner");
    assert_eq!(view["seat_limit"], 3);
    assert_eq!(view["seats_used"], 3);
    assert_eq!(view["members"][0]["email"], a, "saved in lower case");
    assert_eq!(view["members"][0]["joined"], false);
    assert_eq!(view["can_buy_seats"], true);
    t.cleanup().await;
}

#[tokio::test]
#[serial]
async fn a_member_uses_the_owners_plan_and_scans() {
    let mut t = Team::new("member", "Team").await;
    let ali = t.email("member-ali");
    invite(&t.pool, t.owner, &ali).await.unwrap();
    let (member, _) = create_test_user(&t.pool, &ali).await;

    assert_eq!(plan_holder(&t.pool, member.id).await, t.owner);
    let charge = reserve_scan(&t.pool, member.id).await.unwrap();
    assert!(matches!(charge, ScanCharge::Paid { .. }), "{charge:?}");
    let owner_used: i32 =
        query_scalar("SELECT scans_used_this_period FROM subscriptions WHERE user_id = $1")
            .bind(t.owner)
            .fetch_one(&t.pool)
            .await
            .unwrap();
    assert_eq!(
        owner_used, 1,
        "the member's scan counts on the owner's plan"
    );
    let usage = get_scan_usage(&t.pool, member.id).await;
    assert_eq!(usage["plan"], "Team");
    assert_eq!(usage["limit"], 750);

    let view = team_view(&t.pool, member.id).await.unwrap();
    assert_eq!(view["role"], "member");
    assert_eq!(view["has_seat"], true);
    assert_eq!(view["owner_email"], t.emails[0]);
    let owner_view = team_view(&t.pool, t.owner).await.unwrap();
    assert_eq!(owner_view["members"][0]["joined"], true);

    // Leaving: back on their own Free plan.
    assert_eq!(leave(&t.pool, member.id).await, Ok(()));
    assert_eq!(plan_holder(&t.pool, member.id).await, member.id);
    assert_eq!(get_scan_usage(&t.pool, member.id).await["plan"], "Free");
    assert_eq!(leave(&t.pool, member.id).await, Err(TeamError::NotFound));
    t.cleanup().await;
}

#[tokio::test]
#[serial]
async fn one_team_per_person_and_removing() {
    let mut one = Team::new("one", "Team").await;
    let mut two = Team::new("two", "Team").await;
    let ali = one.email("both-ali");
    two.emails.push(ali.clone());
    invite(&one.pool, one.owner, &ali).await.unwrap();
    assert_eq!(
        invite(&two.pool, two.owner, &ali).await,
        Err(TeamError::AlreadyInATeam)
    );
    let (member, _) = create_test_user(&one.pool, &ali).await;
    assert_eq!(plan_holder(&one.pool, member.id).await, one.owner);

    // The other owner cannot remove someone from a team that isn't theirs.
    assert_eq!(
        remove(&two.pool, two.owner, &ali).await,
        Err(TeamError::NotFound)
    );
    assert_eq!(remove(&one.pool, one.owner, &ali).await, Ok(()));
    assert_eq!(plan_holder(&one.pool, member.id).await, member.id);
    one.cleanup().await;
    two.cleanup().await;
}

#[tokio::test]
#[serial]
async fn enterprise_has_no_limit_and_a_member_with_own_plan_keeps_it() {
    let mut t = Team::new("ent", "Enterprise").await;
    for n in 0..6 {
        let email = t.email(&format!("ent-{n}"));
        assert_eq!(invite(&t.pool, t.owner, &email).await, Ok(()));
    }
    let view = team_view(&t.pool, t.owner).await.unwrap();
    assert_eq!(view["seat_limit"], Value::Null);
    assert_eq!(view["can_buy_seats"], false);

    // A member who pays for their own plan uses their own.
    let email = t.emails[1].clone();
    let (member, _) = create_test_user(&t.pool, &email).await;
    insert_test_subscription(&t.pool, member.id, "sub_team_ent_own", "Team", "active").await;
    assert_eq!(plan_holder(&t.pool, member.id).await, member.id);
    t.cleanup().await;
}

#[tokio::test]
#[serial]
async fn the_owners_plan_ending_ends_the_members_access() {
    let mut t = Team::new("ends", "Team").await;
    let ali = t.email("ends-ali");
    invite(&t.pool, t.owner, &ali).await.unwrap();
    let (member, _) = create_test_user(&t.pool, &ali).await;
    assert_eq!(plan_holder(&t.pool, member.id).await, t.owner);
    query("UPDATE subscriptions SET status = 'canceled' WHERE user_id = $1")
        .bind(t.owner)
        .execute(&t.pool)
        .await
        .unwrap();
    assert_eq!(plan_holder(&t.pool, member.id).await, member.id);
    let view = team_view(&t.pool, member.id).await.unwrap();
    assert_eq!(view["has_seat"], false);
    t.cleanup().await;
}

fn seat_event(event_type: &str, owner: Uuid, sub_id: &str, units: i64, status: &str) -> Value {
    json!({
        "id": sub_id,
        "status": status,
        "product": { "id": SEAT_PRODUCT, "name": "Team extra member" },
        "customer": { "id": "cust_seat", "email": "owner@example.com" },
        "items": [{ "id": "sitem_1", "product_id": SEAT_PRODUCT, "price_id": "pprice_1", "units": units }],
        "metadata": { "safely_user_id": owner.to_string() },
        "current_period_end_date": "2030-01-01T00:00:00Z",
        "_event": event_type,
    })
}

#[tokio::test]
#[serial]
async fn extra_seats_from_creem_add_seats_and_are_never_a_plan() {
    let mut t = Team::new("seats", "Team").await;
    let event = seat_event("subscription.paid", t.owner, "sub_seats_1", 2, "active");
    assert!(handle_seat_webhook(&t.pool, "subscription.paid", &event).await);
    let plans: i64 = query_scalar("SELECT COUNT(*) FROM subscriptions WHERE user_id = $1")
        .bind(t.owner)
        .fetch_one(&t.pool)
        .await
        .unwrap();
    assert_eq!(plans, 1, "the seats did not become a second plan");

    let view = team_view(&t.pool, t.owner).await.unwrap();
    assert_eq!(view["extra_seats"], 2);
    assert_eq!(view["seat_limit"], 5);
    for n in 0..4 {
        let email = t.email(&format!("seats-{n}"));
        assert_eq!(invite(&t.pool, t.owner, &email).await, Ok(()), "{n}");
    }
    let one_more = t.email("seats-extra");
    assert_eq!(
        invite(&t.pool, t.owner, &one_more).await,
        Err(TeamError::NoSeatsLeft)
    );
    // Fewer seats than people: remove people first.
    assert_eq!(
        set_extra_seats(&t.pool, t.owner, 1).await,
        Err(TeamError::RemovePeopleFirst)
    );
    assert_eq!(
        set_extra_seats(&t.pool, t.owner, 2).await,
        Ok(SeatChange::Unchanged)
    );
    assert_eq!(
        set_extra_seats(&t.pool, t.owner, 99).await,
        Err(TeamError::TooManySeats)
    );

    // The seats end at Creem: the 2 people invited first keep a seat.
    let ended = seat_event(
        "subscription.canceled",
        t.owner,
        "sub_seats_1",
        2,
        "canceled",
    );
    assert!(handle_seat_webhook(&t.pool, "subscription.canceled", &ended).await);
    let view = team_view(&t.pool, t.owner).await.unwrap();
    assert_eq!(view["seat_limit"], 3);
    let seats: Vec<bool> = view["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["has_seat"].as_bool().unwrap())
        .collect();
    assert_eq!(seats, vec![true, true, false, false]);
    t.cleanup().await;
}

#[tokio::test]
#[serial]
async fn seats_are_cancelled_when_the_team_plan_ends() {
    let t = Team::new("sync", "Team").await;
    let event = seat_event(
        "subscription.active",
        t.owner,
        "sub_seats_sync",
        1,
        "active",
    );
    handle_seat_webhook(&t.pool, "subscription.active", &event).await;
    sync_extra_seats(&t.pool, t.owner).await;
    let status: String = query_scalar("SELECT status FROM team_extra_seats WHERE owner_id = $1")
        .bind(t.owner)
        .fetch_one(&t.pool)
        .await
        .unwrap();
    assert_eq!(status, "active", "still on Team: the seats stay");

    query("UPDATE subscriptions SET plan_name = 'Enterprise' WHERE user_id = $1")
        .bind(t.owner)
        .execute(&t.pool)
        .await
        .unwrap();
    sync_extra_seats(&t.pool, t.owner).await;
    let status: String = query_scalar("SELECT status FROM team_extra_seats WHERE owner_id = $1")
        .bind(t.owner)
        .fetch_one(&t.pool)
        .await
        .unwrap();
    assert_eq!(status, "canceled", "Enterprise has no seat limit");
    t.cleanup().await;
}

#[tokio::test]
#[serial]
async fn buying_the_first_seats_needs_creem_and_a_team_plan() {
    let t = Team::new("buy", "Team").await;
    // The test Creem address does not answer: the checkout fails cleanly.
    assert_eq!(
        set_extra_seats(&t.pool, t.owner, 2).await,
        Err(TeamError::PaymentFailed)
    );
    assert_eq!(
        set_extra_seats(&t.pool, t.owner, 0).await,
        Ok(SeatChange::Unchanged)
    );
    t.cleanup().await;

    let ent = Team::new("buy-ent", "Enterprise").await;
    assert_eq!(
        set_extra_seats(&ent.pool, ent.owner, 1).await,
        Err(TeamError::NeedsTeamPlan)
    );
    ent.cleanup().await;
}

#[tokio::test]
#[serial]
async fn the_creem_webhook_sends_seat_events_to_team() {
    let t = Team::new("hook", "Team").await;
    let secret = var("CREEM_WEBHOOK_SECRET").expect("CREEM_WEBHOOK_SECRET in .env");
    let event_id = format!("evt_seat_{}", Uuid::now_v7());
    let object = seat_event("subscription.paid", t.owner, "sub_seats_hook", 3, "active");
    let raw = json!({
        "id": event_id,
        "eventType": "subscription.paid",
        "created_at": 1_700_000_000,
        "object": object,
    })
    .to_string();
    let mut headers = HeaderMap::new();
    headers.insert(
        "creem-signature",
        HeaderValue::from_str(&compute_creem_signature(&secret, &raw)).unwrap(),
    );
    let status = creem_webhook(State(t.pool.clone()), headers, Bytes::from(raw))
        .await
        .unwrap();
    assert_eq!(status, StatusCode::OK);
    let seats: i32 = query_scalar("SELECT seats FROM team_extra_seats WHERE owner_id = $1")
        .bind(t.owner)
        .fetch_one(&t.pool)
        .await
        .unwrap();
    assert_eq!(seats, 3);
    let plans: Vec<String> = query_scalar("SELECT plan_name FROM subscriptions WHERE user_id = $1")
        .bind(t.owner)
        .fetch_all(&t.pool)
        .await
        .unwrap();
    assert_eq!(plans, vec!["Team"], "the Team plan was not replaced");
    t.cleanup().await;
}

async fn call(
    app: &Router,
    method: &str,
    path: &str,
    headers: &HeaderMap,
    body: Value,
) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(path);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    let req = req
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), 1_000_000).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
#[serial]
async fn the_team_endpoints() {
    let mut t = Team::new("http", "Team").await;
    let app: Router = billing_routes().with_state(t.pool.clone());
    let headers = auth_headers_for(&t.pool, t.owner).await;
    let ali = t.email("http-ali");

    let (status, _) = call(&app, "GET", "/api/v1/team", &HeaderMap::new(), json!({})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/team/invite",
        &headers,
        json!({ "email": ali }),
    )
    .await;
    assert_eq!(
        (status, body["success"].clone()),
        (StatusCode::OK, json!(true))
    );
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/team/invite",
        &headers,
        json!({ "email": "x" }),
    )
    .await;
    assert_eq!(
        (status, body["error"].clone()),
        (StatusCode::BAD_REQUEST, json!("invalid_email"))
    );

    let (status, body) = call(&app, "GET", "/api/v1/team", &headers, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["seats_used"], 2);

    let (member, _) = create_test_user(&t.pool, &ali).await;
    let member_headers = auth_headers_for(&t.pool, member.id).await;
    let (_, body) = call(
        &app,
        "GET",
        "/api/v1/billing/subscription-status",
        &member_headers,
        json!({}),
    )
    .await;
    assert_eq!(body["shared_plan"]["plan"], "Team");
    assert_eq!(body["shared_plan"]["owner_email"], t.emails[0]);
    assert_eq!(body["usage"]["plan"], "Team");
    let (_, body) = call(
        &app,
        "GET",
        "/api/v1/billing/subscription-status",
        &headers,
        json!({}),
    )
    .await;
    assert_eq!(
        body["shared_plan"],
        Value::Null,
        "the owner uses their own plan"
    );

    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/team/leave",
        &member_headers,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/team/seats",
        &headers,
        json!({ "extra_seats": 0 }),
    )
    .await;
    assert_eq!(
        (status, body["result"].clone()),
        (StatusCode::OK, json!("unchanged"))
    );
    t.cleanup().await;
}

#[tokio::test]
#[serial]
async fn nobody_pays_twice_for_one_person() {
    let mut t = Team::new("twice", "Team").await;

    // Already paying for their own plan: cannot be invited.
    let payer = t.email("twice-payer");
    let (paying, _) = create_test_user(&t.pool, &payer).await;
    insert_test_subscription(&t.pool, paying.id, "sub_team_twice_payer", "Team", "active").await;
    assert_eq!(
        invite(&t.pool, t.owner, &payer).await,
        Err(TeamError::HasOwnPlan)
    );

    // A Free account can be invited, and uses the owner's plan at once.
    let ali = t.email("twice-ali");
    let (member, _) = create_test_user(&t.pool, &ali).await;
    assert_eq!(invite(&t.pool, t.owner, &ali).await, Ok(()));
    assert_eq!(plan_holder(&t.pool, member.id).await, t.owner);
    leave_team_if_paying(&t.pool, member.id).await;
    assert_eq!(
        team_view(&t.pool, t.owner).await.unwrap()["seats_used"],
        2,
        "still Free: stays"
    );

    // The member buys their own plan: taken off the team, seat freed.
    insert_test_subscription(&t.pool, member.id, "sub_team_twice_ali", "Team", "active").await;
    leave_team_if_paying(&t.pool, member.id).await;
    let view = team_view(&t.pool, t.owner).await.unwrap();
    assert_eq!(view["seats_used"], 1);
    assert_eq!(plan_holder(&t.pool, member.id).await, member.id);
    t.cleanup().await;
}

#[tokio::test]
#[serial]
async fn a_member_who_buys_a_plan_is_taken_off_by_the_webhook() {
    let mut t = Team::new("buyer", "Team").await;
    let ali = t.email("buyer-ali");
    invite(&t.pool, t.owner, &ali).await.unwrap();
    let (member, _) = create_test_user(&t.pool, &ali).await;

    let secret = var("CREEM_WEBHOOK_SECRET").expect("CREEM_WEBHOOK_SECRET in .env");
    let sub_id = format!("sub_buyer_{}", Uuid::now_v7());
    let raw = json!({
        "id": format!("evt_buyer_{}", Uuid::now_v7()),
        "eventType": "subscription.paid",
        "created_at": 1_700_000_000,
        "object": {
            "id": sub_id,
            "status": "active",
            "current_period_end_date": "2030-01-01T00:00:00Z",
            "product": { "id": var("CREEM_TEAM_PRODUCT_ID").unwrap_or_default(), "name": "Team" },
            "customer": { "id": "cust_buyer", "email": ali },
            "metadata": { "safely_user_id": member.id.to_string() },
        },
    })
    .to_string();
    let mut headers = HeaderMap::new();
    headers.insert(
        "creem-signature",
        HeaderValue::from_str(&compute_creem_signature(&secret, &raw)).unwrap(),
    );
    creem_webhook(State(t.pool.clone()), headers, Bytes::from(raw))
        .await
        .unwrap();
    let still_member: i64 = query_scalar("SELECT COUNT(*) FROM team_members WHERE email = $1")
        .bind(&ali)
        .fetch_one(&t.pool)
        .await
        .unwrap();
    assert_eq!(
        still_member, 0,
        "paying for their own plan took them off the team"
    );
    let _ = query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(&sub_id)
        .execute(&t.pool)
        .await;
    t.cleanup().await;
}
