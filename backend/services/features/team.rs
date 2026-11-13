//! Team plans: how many people can use one paid plan, inviting and
//! removing them, and buying extra seats.
//!
//! - Free: 1 person (only the account itself, no invites).
//! - Team: 3 people (the owner + 2 invited), plus 1 per extra seat. An
//!   extra seat is the "Team extra member" product at Creem ($25 each,
//!   CREEM_EXTRA_SEAT_PRODUCT_ID / CREEM_EXTRA_SEAT_YEARLY_PRODUCT_ID).
//! - Enterprise: unlimited people.
//!
//! An invited person uses the owner's plan as soon as they sign in to
//! Safely with the invited email: their scans count on the owner's plan
//! (plan_holder). One email can be in one team only. When the owner has
//! fewer seats than members (after a downgrade), the members invited
//! first keep access.
//!
//! The extra seats are their own Creem subscription, kept in
//! team_extra_seats - never in the subscriptions table, so they never
//! count as a plan. They are cancelled by themselves when the owner no
//! longer has a Team plan (sync_extra_seats).

use crate::services::features::billing::{BillingInterval, cancel_with_creem};
use crate::services::features::email::send_team_invite_email;
use reqwest::Client;
use serde_json::{Value, json};
use sqlx::{Pool, Postgres, query, query_as, query_scalar};
use std::env::var;
use uuid::Uuid;

/// People included in a Team plan, the owner counted.
pub const TEAM_INCLUDED_SEATS: i64 = 3;
/// Price of one extra seat a month, shown in the dashboard.
pub const EXTRA_SEAT_PRICE_USD: i64 = 25;
/// Most extra seats one Team plan can buy from the dashboard. Bigger
/// teams are Enterprise.
pub const MAX_EXTRA_SEATS: i64 = 50;

/// The Creem products for extra seats: (env var, billing).
pub const SEAT_PRODUCTS: [(&str, BillingInterval); 2] = [
    ("CREEM_EXTRA_SEAT_PRODUCT_ID", BillingInterval::Month),
    ("CREEM_EXTRA_SEAT_YEARLY_PRODUCT_ID", BillingInterval::Year),
];

/// Why a team change was refused. The handler sends the code to the
/// dashboard, which shows a message in the page's language.
#[derive(Debug, PartialEq, Eq)]
pub enum TeamError {
    /// "invalid_email"
    InvalidEmail,
    /// "own_email": the owner tried to invite themselves.
    OwnEmail,
    /// "needs_team_plan": Free has no invites; extra seats need Team.
    NeedsTeamPlan,
    /// "no_seats_left": every seat is used.
    NoSeatsLeft,
    /// "already_in_a_team": the email is in a team already.
    AlreadyInATeam,
    /// "has_own_plan": the person already pays for their own plan, so a
    /// seat for them would be paid for twice.
    HasOwnPlan,
    /// "not_found": no such member, or not in a team.
    NotFound,
    /// "too_many_seats": more than MAX_EXTRA_SEATS.
    TooManySeats,
    /// "remove_people_first": fewer seats than people in the team.
    RemovePeopleFirst,
    /// "seats_unavailable": the extra seat product is not in .env.
    SeatsUnavailable,
    /// "payment_failed": Creem could not be reached or said no.
    PaymentFailed,
    /// "unavailable": the database could not be read.
    Unavailable,
}

impl TeamError {
    pub fn code(&self) -> &'static str {
        match self {
            TeamError::InvalidEmail => "invalid_email",
            TeamError::OwnEmail => "own_email",
            TeamError::NeedsTeamPlan => "needs_team_plan",
            TeamError::NoSeatsLeft => "no_seats_left",
            TeamError::AlreadyInATeam => "already_in_a_team",
            TeamError::HasOwnPlan => "has_own_plan",
            TeamError::NotFound => "not_found",
            TeamError::TooManySeats => "too_many_seats",
            TeamError::RemovePeopleFirst => "remove_people_first",
            TeamError::SeatsUnavailable => "seats_unavailable",
            TeamError::PaymentFailed => "payment_failed",
            TeamError::Unavailable => "unavailable",
        }
    }
}

impl From<sqlx::Error> for TeamError {
    fn from(e: sqlx::Error) -> Self {
        eprintln!("Safely: team database error: {}", e);
        TeamError::Unavailable
    }
}

/// How many people a plan has, the owner counted. None is unlimited.
/// No plan (or an unknown one) is Free: 1.
pub fn seat_limit(plan: Option<&str>, extra_seats: i64) -> Option<i64> {
    match plan {
        Some("Enterprise") => None,
        Some("Team") => Some(TEAM_INCLUDED_SEATS + extra_seats.max(0)),
        _ => Some(1),
    }
}

/// True when a member at this place in the owner's list (0 = invited
/// first) has a seat. The owner takes one seat.
pub fn has_seat(place: i64, limit: Option<i64>) -> bool {
    match limit {
        None => true,
        Some(limit) => place < limit - 1,
    }
}

/// A cleaned email to invite, or None when it does not look like one.
pub fn clean_invite_email(text: &str) -> Option<String> {
    let email = text.trim().to_lowercase();
    let (local, domain) = email.split_once('@')?;
    let ok = !local.is_empty()
        && email.len() <= 254
        && !domain.contains('@')
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email
            .chars()
            .any(|c| c.is_whitespace() || c == ',' || c == ';');
    ok.then_some(email)
}

/// True when this Creem product is an extra seat product (from .env).
pub fn is_seat_product(product_id: &str) -> bool {
    SEAT_PRODUCTS
        .iter()
        .any(|(env_name, _)| var(env_name).is_ok_and(|id| !id.is_empty() && id == product_id))
}

/// The extra seat product for this billing: the yearly one for a yearly
/// plan when it is set, else the monthly one.
pub fn seat_product_for(interval: BillingInterval) -> Option<String> {
    let monthly = var(SEAT_PRODUCTS[0].0).ok().filter(|s| !s.is_empty());
    let yearly = var(SEAT_PRODUCTS[1].0).ok().filter(|s| !s.is_empty());
    match interval {
        BillingInterval::Year => yearly.or(monthly),
        BillingInterval::Month => monthly,
    }
}

/// The account's own active paid plan: (plan, billing).
async fn own_plan(
    pool: &Pool<Postgres>,
    user_id: Uuid,
) -> Result<Option<(String, BillingInterval)>, sqlx::Error> {
    let row: Option<(String, String)> = query_as(
        "SELECT plan_name, billing_interval FROM subscriptions
         WHERE user_id = $1 AND status = 'active'
         ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(plan, interval)| (plan, BillingInterval::from_db(&interval))))
}

/// The owner's extra seats that are paid for (0 when none).
async fn extra_seats(pool: &Pool<Postgres>, owner_id: Uuid) -> Result<i64, sqlx::Error> {
    let seats: Option<i32> = query_scalar(
        "SELECT seats FROM team_extra_seats WHERE owner_id = $1 AND status = 'active'",
    )
    .bind(owner_id)
    .fetch_optional(pool)
    .await?;
    Ok(seats.unwrap_or(0) as i64)
}

/// The owner's plan and how many people it has.
async fn owner_seats(
    pool: &Pool<Postgres>,
    owner_id: Uuid,
) -> Result<(Option<String>, Option<i64>, i64), sqlx::Error> {
    let plan = own_plan(pool, owner_id).await?.map(|(p, _)| p);
    let extra = extra_seats(pool, owner_id).await?;
    let limit = seat_limit(plan.as_deref(), extra);
    Ok((plan, limit, extra))
}

/// The team this user was invited to: (owner, place in the owner's
/// list, 0 = invited first).
async fn membership(
    pool: &Pool<Postgres>,
    user_id: Uuid,
) -> Result<Option<(Uuid, i64)>, sqlx::Error> {
    query_as(
        "SELECT m.owner_id,
                (SELECT COUNT(*) FROM team_members o
                 WHERE o.owner_id = m.owner_id
                   AND (o.created_at, o.id) < (m.created_at, m.id))
         FROM team_members m
         JOIN users u ON lower(u.email) = m.email
         WHERE u.id = $1 AND m.owner_id <> u.id",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// The account whose plan this user uses: their own when they pay for
/// one; else the owner of their team, when that owner has a paid plan
/// with a seat for them; else their own (Free). Never fails: a database
/// problem means their own account.
pub async fn plan_holder(pool: &Pool<Postgres>, user_id: Uuid) -> Uuid {
    match plan_holder_checked(pool, user_id).await {
        Ok(holder) => holder,
        Err(e) => {
            eprintln!("Safely: could not check the user's team: {}", e);
            user_id
        }
    }
}

async fn plan_holder_checked(pool: &Pool<Postgres>, user_id: Uuid) -> Result<Uuid, sqlx::Error> {
    if own_plan(pool, user_id).await?.is_some() {
        return Ok(user_id);
    }
    let Some((owner_id, place)) = membership(pool, user_id).await? else {
        return Ok(user_id);
    };
    let (plan, limit, _) = owner_seats(pool, owner_id).await?;
    if plan.is_some() && limit != Some(1) && has_seat(place, limit) {
        Ok(owner_id)
    } else {
        Ok(user_id)
    }
}

/// What the dashboard's "Team" card shows for this user.
///
/// - role "owner": the plan, seats used / limit, the people invited
///   (joined = has signed in to Safely; has_seat = inside the limit) and
///   the extra seats.
/// - role "member": whose team they are in, and whether they have a seat.
pub async fn team_view(pool: &Pool<Postgres>, user_id: Uuid) -> Result<Value, TeamError> {
    let own = own_plan(pool, user_id).await?;
    if own.is_none()
        && let Some((owner_id, place)) = membership(pool, user_id).await?
    {
        let (plan, limit, _) = owner_seats(pool, owner_id).await?;
        let owner_email: Option<String> = query_scalar("SELECT email FROM users WHERE id = $1")
            .bind(owner_id)
            .fetch_optional(pool)
            .await?;
        let active = plan.is_some() && limit != Some(1) && has_seat(place, limit);
        return Ok(json!({
            "role": "member",
            "owner_email": owner_email,
            "plan": plan.unwrap_or_else(|| "Free".to_string()),
            "has_seat": active,
        }));
    }

    let (plan, limit, extra) = owner_seats(pool, user_id).await?;
    let rows: Vec<(String, bool)> = query_as(
        "SELECT m.email, EXISTS (SELECT 1 FROM users u WHERE lower(u.email) = m.email)
         FROM team_members m WHERE m.owner_id = $1
         ORDER BY m.created_at, m.id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let members: Vec<Value> = rows
        .iter()
        .enumerate()
        .map(|(place, (email, joined))| {
            json!({
                "email": email,
                "joined": joined,
                "has_seat": plan.is_some() && has_seat(place as i64, limit),
            })
        })
        .collect();
    let interval = own
        .as_ref()
        .map(|(_, i)| *i)
        .unwrap_or(BillingInterval::Month);
    Ok(json!({
        "role": "owner",
        "plan": plan.clone().unwrap_or_else(|| "Free".to_string()),
        "seat_limit": limit,
        "seats_used": 1 + members.len() as i64,
        "included_seats": if plan.as_deref() == Some("Team") { Some(TEAM_INCLUDED_SEATS) } else { None },
        "extra_seats": extra,
        "can_buy_seats": plan.as_deref() == Some("Team") && seat_product_for(interval).is_some(),
        "seat_price_usd": EXTRA_SEAT_PRICE_USD,
        "max_extra_seats": MAX_EXTRA_SEATS,
        "members": members,
    }))
}

/// Invites one person to the owner's team, and emails them. The email
/// is sent in the background; a failed email is only logged (the
/// person can still sign in and use the plan).
pub async fn invite(pool: &Pool<Postgres>, owner_id: Uuid, text: &str) -> Result<(), TeamError> {
    let email = clean_invite_email(text).ok_or(TeamError::InvalidEmail)?;
    let owner_email: String = query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(owner_id)
        .fetch_one(pool)
        .await?;
    if owner_email.trim().to_lowercase() == email {
        return Err(TeamError::OwnEmail);
    }
    let (plan, limit, _) = owner_seats(pool, owner_id).await?;
    if plan.is_none() || limit == Some(1) {
        return Err(TeamError::NeedsTeamPlan);
    }
    let members: i64 = query_scalar("SELECT COUNT(*) FROM team_members WHERE owner_id = $1")
        .bind(owner_id)
        .fetch_one(pool)
        .await?;
    if !has_seat(members, limit) {
        return Err(TeamError::NoSeatsLeft);
    }
    // Someone who already pays for their own plan would use that plan
    // anyway, and the owner would pay for a seat nobody uses.
    let pays_own: bool = query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM users u JOIN subscriptions s ON s.user_id = u.id
             WHERE lower(u.email) = $1 AND s.status = 'active')",
    )
    .bind(&email)
    .fetch_one(pool)
    .await?;
    if pays_own {
        return Err(TeamError::HasOwnPlan);
    }
    let added = query(
        "INSERT INTO team_members (id, owner_id, email) VALUES ($1, $2, $3)
         ON CONFLICT (email) DO NOTHING",
    )
    .bind(Uuid::now_v7())
    .bind(owner_id)
    .bind(&email)
    .execute(pool)
    .await?;
    if added.rows_affected() == 0 {
        return Err(TeamError::AlreadyInATeam);
    }
    let plan = plan.unwrap_or_default();
    tokio::spawn(async move {
        if let Err(e) = send_team_invite_email(&email, &owner_email, &plan).await {
            eprintln!("Safely: could not send the team invite email: {:?}", e);
        }
    });
    Ok(())
}

/// Removes one person from the owner's team.
pub async fn remove(pool: &Pool<Postgres>, owner_id: Uuid, text: &str) -> Result<(), TeamError> {
    let email = text.trim().to_lowercase();
    let removed = query("DELETE FROM team_members WHERE owner_id = $1 AND email = $2")
        .bind(owner_id)
        .bind(&email)
        .execute(pool)
        .await?;
    if removed.rows_affected() == 0 {
        return Err(TeamError::NotFound);
    }
    Ok(())
}

/// The signed-in member leaves the team they were invited to.
pub async fn leave(pool: &Pool<Postgres>, user_id: Uuid) -> Result<(), TeamError> {
    let left = query(
        "DELETE FROM team_members m USING users u
         WHERE u.id = $1 AND m.email = lower(u.email)",
    )
    .bind(user_id)
    .execute(pool)
    .await?;
    if left.rows_affected() == 0 {
        return Err(TeamError::NotFound);
    }
    Ok(())
}

/// Takes a member off the team they were invited to once they pay for
/// their own plan, so the owner's seat is free for someone else and
/// nobody pays twice. Called after every plan event from Creem. Only
/// logged when it fails.
pub async fn leave_team_if_paying(pool: &Pool<Postgres>, user_id: Uuid) {
    match own_plan(pool, user_id).await {
        Ok(Some(_)) => {
            if let Err(e) = query(
                "DELETE FROM team_members m USING users u
                 WHERE u.id = $1 AND m.email = lower(u.email) AND m.owner_id <> u.id",
            )
            .bind(user_id)
            .execute(pool)
            .await
            {
                eprintln!(
                    "Safely: could not take a paying member off their team: {}",
                    e
                );
            }
        }
        Ok(None) => {}
        Err(e) => eprintln!("Safely: could not check a member's own plan: {}", e),
    }
}

/// What changing the number of extra seats did.
#[derive(Debug, PartialEq, Eq)]
pub enum SeatChange {
    /// Nothing to change.
    Unchanged,
    /// No seats yet: pay for them on this Creem checkout page.
    Checkout(String),
    /// The seat count at Creem was changed (charged now, for the rest
    /// of the billing period).
    Updated,
    /// All extra seats were removed: the seat subscription is cancelled.
    Cancelled,
}

/// Sets the owner's extra seats to `wanted` (0 removes them all).
pub async fn set_extra_seats(
    pool: &Pool<Postgres>,
    owner_id: Uuid,
    wanted: i64,
) -> Result<SeatChange, TeamError> {
    if !(0..=MAX_EXTRA_SEATS).contains(&wanted) {
        return Err(TeamError::TooManySeats);
    }
    let Some((plan, interval)) = own_plan(pool, owner_id).await? else {
        return Err(TeamError::NeedsTeamPlan);
    };
    if plan != "Team" {
        return Err(TeamError::NeedsTeamPlan);
    }
    let members: i64 = query_scalar("SELECT COUNT(*) FROM team_members WHERE owner_id = $1")
        .bind(owner_id)
        .fetch_one(pool)
        .await?;
    if 1 + members > TEAM_INCLUDED_SEATS + wanted {
        return Err(TeamError::RemovePeopleFirst);
    }

    let current: Option<(String, i32)> = query_as(
        "SELECT creem_subscription_id, seats FROM team_extra_seats
         WHERE owner_id = $1 AND status = 'active'",
    )
    .bind(owner_id)
    .fetch_optional(pool)
    .await?;

    match current {
        None if wanted == 0 => Ok(SeatChange::Unchanged),
        None => {
            let product = seat_product_for(interval).ok_or(TeamError::SeatsUnavailable)?;
            let url = seat_checkout(&product, owner_id, wanted).await?;
            Ok(SeatChange::Checkout(url))
        }
        Some((_, seats)) if seats as i64 == wanted => Ok(SeatChange::Unchanged),
        Some((sub_id, _)) if wanted == 0 => {
            cancel_with_creem(&sub_id).await.map_err(|e| {
                eprintln!("Safely: could not cancel the extra seats at Creem: {:?}", e);
                TeamError::PaymentFailed
            })?;
            query(
                "UPDATE team_extra_seats SET status = 'canceled', updated_at = NOW()
                 WHERE creem_subscription_id = $1",
            )
            .bind(&sub_id)
            .execute(pool)
            .await?;
            Ok(SeatChange::Cancelled)
        }
        Some((sub_id, _)) => {
            change_seat_units(&sub_id, wanted).await?;
            query(
                "UPDATE team_extra_seats SET seats = $2, updated_at = NOW()
                 WHERE creem_subscription_id = $1",
            )
            .bind(&sub_id)
            .bind(wanted as i32)
            .execute(pool)
            .await?;
            Ok(SeatChange::Updated)
        }
    }
}

fn creem_base_url() -> String {
    var("CREEM_API_BASE_URL").unwrap_or_else(|_| "https://test-api.creem.io".to_string())
}

fn creem_api_key() -> Result<String, TeamError> {
    var("CREEM_API_KEY").map_err(|_| {
        eprintln!("Safely: CREEM_API_KEY not set");
        TeamError::PaymentFailed
    })
}

/// The body of a Creem checkout for `units` extra seats.
pub fn seat_checkout_body(product_id: &str, owner_id: Uuid, units: i64, base_url: &str) -> Value {
    json!({
        "product_id": product_id,
        "units": units,
        "success_url": format!("{}/settings?seats=success", base_url),
        "metadata": {
            "safely_user_id": owner_id.to_string(),
            "safely_purpose": "extra_seats",
        },
    })
}

/// A Creem checkout page for `units` extra seats. Returns its address.
async fn seat_checkout(product_id: &str, owner_id: Uuid, units: i64) -> Result<String, TeamError> {
    let base_url = var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let response = Client::new()
        .post(format!("{}/v1/checkouts", creem_base_url()))
        .header("x-api-key", creem_api_key()?)
        .json(&seat_checkout_body(product_id, owner_id, units, &base_url))
        .send()
        .await
        .map_err(|e| {
            eprintln!("Safely: Creem seat checkout failed: {}", e);
            TeamError::PaymentFailed
        })?;
    if !response.status().is_success() {
        eprintln!(
            "Safely: Creem refused the seat checkout: {}",
            response.text().await.unwrap_or_default()
        );
        return Err(TeamError::PaymentFailed);
    }
    let body: Value = response
        .json()
        .await
        .map_err(|_| TeamError::PaymentFailed)?;
    body["checkout_url"]
        .as_str()
        .map(str::to_string)
        .ok_or(TeamError::PaymentFailed)
}

/// Text safe to put in an address: letters, digits, "-" and "_" stay,
/// anything else is %-encoded.
fn url_part(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' => (b as char).to_string(),
            _ => format!("%{:02X}", b),
        })
        .collect()
}

/// The item of a Creem subscription to change: (item id, price id). The
/// seat product's item when it can be told apart, else the first one.
pub fn seat_item(subscription: &Value) -> Option<(String, Option<String>)> {
    let items = subscription["items"].as_array()?;
    let item = items
        .iter()
        .find(|i| i["product_id"].as_str().is_some_and(is_seat_product))
        .or_else(|| items.first())?;
    Some((
        item["id"].as_str()?.to_string(),
        item["price_id"].as_str().map(str::to_string),
    ))
}

/// Changes the number of seats on the seat subscription at Creem. The
/// difference is charged now, for the rest of the billing period.
async fn change_seat_units(sub_id: &str, units: i64) -> Result<(), TeamError> {
    let key = creem_api_key()?;
    let client = Client::new();
    let failed = |what: &str| {
        let what = what.to_string();
        move |e: reqwest::Error| {
            eprintln!("Safely: Creem {} failed: {}", what, e);
            TeamError::PaymentFailed
        }
    };
    let subscription: Value = client
        .get(format!(
            "{}/v1/subscriptions?subscription_id={}",
            creem_base_url(),
            url_part(sub_id)
        ))
        .header("x-api-key", &key)
        .send()
        .await
        .map_err(failed("subscription lookup"))?
        .json()
        .await
        .map_err(failed("subscription lookup"))?;
    let (item_id, price_id) = seat_item(&subscription).ok_or_else(|| {
        eprintln!(
            "Safely: the seat subscription {} has no item to change",
            sub_id
        );
        TeamError::PaymentFailed
    })?;
    let mut item = json!({ "id": item_id, "units": units });
    if let Some(price_id) = price_id {
        item["price_id"] = json!(price_id);
    }
    let response = client
        .post(format!("{}/v1/subscriptions/{}", creem_base_url(), sub_id))
        .header("x-api-key", &key)
        .json(&json!({
            "items": [item],
            "update_behavior": "proration-charge-immediately",
        }))
        .send()
        .await
        .map_err(failed("seat change"))?;
    if !response.status().is_success() {
        eprintln!(
            "Safely: Creem refused the seat change: {}",
            response.text().await.unwrap_or_default()
        );
        return Err(TeamError::PaymentFailed);
    }
    Ok(())
}

/// The number of seats in a Creem subscription: the seat item's units,
/// or 1 when Creem does not say.
pub fn units_in(subscription: &Value) -> i64 {
    let items = subscription["items"].as_array();
    let item = items.and_then(|items| {
        items
            .iter()
            .find(|i| i["product_id"].as_str().is_some_and(is_seat_product))
            .or_else(|| items.first())
    });
    item.and_then(|i| i["units"].as_f64())
        .map(|u| u.max(0.0) as i64)
        .unwrap_or(1)
}

/// The product id of a Creem subscription object ("product" is an
/// object or just its id).
fn product_id_of(subscription: &Value) -> Option<&str> {
    subscription["product"]["id"]
        .as_str()
        .or_else(|| subscription["product"].as_str())
}

/// Handles a Creem webhook when it is about extra seats, and returns
/// true; returns false for every other event (plans, ...), which the
/// normal billing code then handles. Seat events never touch the
/// subscriptions table.
pub async fn handle_seat_webhook(pool: &Pool<Postgres>, event_type: &str, object: &Value) -> bool {
    let subscription = if event_type.starts_with("subscription.") {
        object
    } else if event_type == "checkout.completed" {
        let sub = &object["subscription"];
        let product = product_id_of(sub).or_else(|| product_id_of(object));
        return product.is_some_and(is_seat_product);
    } else {
        return false;
    };
    let Some(product_id) = product_id_of(subscription) else {
        return false;
    };
    if !is_seat_product(product_id) {
        return false;
    }

    let owner_id = subscription["metadata"]["safely_user_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok());
    let Some(sub_id) = subscription["id"].as_str() else {
        return true;
    };
    let Some(owner_id) = owner_id else {
        eprintln!("Safely: seat event for {} has no safely_user_id", sub_id);
        return true;
    };
    let status = match event_type {
        "subscription.paused" => "paused",
        "subscription.expired" => "expired",
        "subscription.canceled" => "canceled",
        "subscription.past_due" => "past_due",
        _ => subscription["status"].as_str().unwrap_or("active"),
    };
    let seats = units_in(subscription);
    // A late event about an old seat subscription never replaces a newer
    // active one.
    let saved = query(
        "INSERT INTO team_extra_seats (owner_id, creem_subscription_id, seats, status, updated_at)
         VALUES ($1, $2, $3, $4, NOW())
         ON CONFLICT (owner_id) DO UPDATE SET
             creem_subscription_id = EXCLUDED.creem_subscription_id,
             seats = EXCLUDED.seats,
             status = EXCLUDED.status,
             updated_at = NOW()
         WHERE team_extra_seats.creem_subscription_id = EXCLUDED.creem_subscription_id
            OR EXCLUDED.status = 'active'",
    )
    .bind(owner_id)
    .bind(sub_id)
    .bind(seats as i32)
    .bind(status)
    .execute(pool)
    .await;
    if let Err(e) = saved {
        eprintln!("Safely: could not save the extra seats: {}", e);
    }
    // Seats bought while the owner is not on Team are cancelled at once.
    sync_extra_seats(pool, owner_id).await;
    true
}

/// Cancels the owner's extra seats when they no longer have a Team plan
/// (cancelled, ended, or moved to Enterprise, which has no seat limit),
/// so they are never billed for seats they cannot use.
pub async fn sync_extra_seats(pool: &Pool<Postgres>, owner_id: Uuid) {
    let plan = match own_plan(pool, owner_id).await {
        Ok(plan) => plan.map(|(p, _)| p),
        Err(e) => {
            eprintln!("Safely: could not check the plan for extra seats: {}", e);
            return;
        }
    };
    if plan.as_deref() == Some("Team") {
        return;
    }
    let sub_id: Option<String> = query_scalar(
        "SELECT creem_subscription_id FROM team_extra_seats
         WHERE owner_id = $1 AND status IN ('active', 'past_due')",
    )
    .bind(owner_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    let Some(sub_id) = sub_id else {
        return;
    };
    if let Err(e) = cancel_with_creem(&sub_id).await {
        eprintln!(
            "Safely: could not cancel extra seats {} at Creem - cancel them in the Creem dashboard: {:?}",
            sub_id, e
        );
    }
    let _ = query(
        "UPDATE team_extra_seats SET status = 'canceled', updated_at = NOW()
         WHERE creem_subscription_id = $1",
    )
    .bind(&sub_id)
    .execute(pool)
    .await;
}

/// The plan this member uses from their team, for the dashboard's
/// "Plan & Billing" card: (plan, owner's email). None when they use
/// their own account.
pub async fn shared_plan(pool: &Pool<Postgres>, user_id: Uuid) -> Option<(String, String)> {
    let holder = plan_holder(pool, user_id).await;
    if holder == user_id {
        return None;
    }
    let plan = own_plan(pool, holder).await.ok().flatten()?.0;
    let email: String = query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(holder)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()?;
    Some((plan, email))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seats_per_plan() {
        assert_eq!(seat_limit(None, 0), Some(1));
        assert_eq!(seat_limit(Some("Gold"), 5), Some(1));
        assert_eq!(seat_limit(Some("Team"), 0), Some(3));
        assert_eq!(seat_limit(Some("Team"), 2), Some(5));
        assert_eq!(seat_limit(Some("Team"), -4), Some(3));
        assert_eq!(seat_limit(Some("Enterprise"), 0), None);
    }

    #[test]
    fn the_owner_takes_one_seat() {
        // Team (3 seats): the first 2 people invited have a seat.
        assert!(has_seat(0, Some(3)));
        assert!(has_seat(1, Some(3)));
        assert!(!has_seat(2, Some(3)));
        assert!(!has_seat(0, Some(1)), "Free has no seat for others");
        assert!(has_seat(500, None));
    }

    #[test]
    fn invite_emails_are_cleaned() {
        assert_eq!(
            clean_invite_email("  Ali@Example.COM ").as_deref(),
            Some("ali@example.com")
        );
        for bad in [
            "",
            "ali",
            "@example.com",
            "ali@",
            "ali@example",
            "a b@x.com",
            "a@b@c.com",
            "a@x.com,b@y.com",
            "a@.com",
        ] {
            assert_eq!(clean_invite_email(bad), None, "{bad}");
        }
    }

    #[test]
    fn checkout_asks_for_the_seats() {
        let owner = Uuid::nil();
        let body = seat_checkout_body("prod_seat", owner, 2, "https://safely.sh");
        assert_eq!(body["units"], 2);
        assert_eq!(body["product_id"], "prod_seat");
        assert_eq!(body["metadata"]["safely_purpose"], "extra_seats");
        assert_eq!(body["metadata"]["safely_user_id"], owner.to_string());
        assert_eq!(
            body["success_url"],
            "https://safely.sh/settings?seats=success"
        );
    }

    #[test]
    fn units_and_items_from_creem() {
        let sub = json!({"items": [{"id": "sitem_1", "price_id": "pprice_1", "units": 3}]});
        assert_eq!(units_in(&sub), 3);
        assert_eq!(
            seat_item(&sub),
            Some(("sitem_1".to_string(), Some("pprice_1".to_string())))
        );
        assert_eq!(units_in(&json!({"items": []})), 1);
        assert_eq!(units_in(&json!({})), 1);
        assert_eq!(seat_item(&json!({})), None);
    }

    #[test]
    fn error_codes() {
        assert_eq!(TeamError::NoSeatsLeft.code(), "no_seats_left");
        assert_eq!(TeamError::AlreadyInATeam.code(), "already_in_a_team");
    }
}
