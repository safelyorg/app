use crate::errors::billing::{BillingError, WebhookError};
use crate::services::features::billing::{extract_subscription, mark_event_processed_if_new};
use crate::services::{
    features::auth::extract_user_id,
    features::billing::{
        BillingInterval, CreateCheckoutError, PLAN_PRODUCTS, PlanChange,
        apply_scheduled_downgrade_if_due, apply_upgrade, cancel_at_period_end_with_creem,
        classify_plan_change, create_checkout_for, customer_portal_link, fetch_subscriber_email,
        get_scan_usage, handle_subscription_granted, handle_subscription_lost,
        handle_subscription_past_due, handle_subscription_update, plan_for_product_id,
        product_ids_key, resume_with_creem, schedule_downgrade, verify_and_parse_webhook,
    },
    features::email::{send_subscription_canceled_email, send_team_plan_ending_email},
    features::meta_capi::{AdClick, payment_in, send_purchase},
    features::team::{
        end_seats_with_plan, handle_seat_webhook, keep_seats_with_plan, leave_team_if_paying,
        plan_holder, shared_plan, sync_extra_seats, team_member_emails, team_plan_ends,
    },
};
use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::{Pool, Postgres, query, query_as, query_scalar};
use std::env::var;

#[derive(Debug, Deserialize)]
pub struct CreateCheckoutBody {
    pub product_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangePlanBody {
    pub product_id: String,
}

/// POST /api/v1/billing/checkout
///
/// It's the real endpoint your frontend calls when someone wants to start a
/// subscription. It confirms who's asking, then hands off to Creem to
/// actually build a real, ready-to-pay checkout link for that specific plan.
///
/// It confirms who's genuinely signed in, right now, sends the chosen plan
/// and the real user's ID to Creem, to build the checkout, and if anything
/// went wrong along the way like an unrecognized user, a broken connection to
/// Creem, or Creem itself refusing the request, it reports back specifically
/// which of those actually happened, instead of a single, generic failure.
/// Otherwise, it hands back the real, working checkout link.
pub async fn create_checkout_handler(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(body): Json<CreateCheckoutBody>,
) -> Result<Json<Value>, BillingError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| BillingError::InternalError("Failed to verify session".to_string()))?
        .ok_or(BillingError::Unauthorized)?;

    let (new_plan, new_interval) = plan_for_product_id(&body.product_id)
        .ok_or_else(|| BillingError::InvalidRequest("Unknown plan".to_string()))?;

    // Someone already on a paid plan only gets a new checkout when moving
    // from monthly to yearly (the monthly plan is then ended automatically
    // once the yearly one is paid). Every other change goes through
    // /billing/change-plan, so nobody ends up paying for two plans.
    if let Some((current_plan, current_interval)) = active_plan(&pool, user_id).await? {
        match classify_plan_change(&current_plan, current_interval, new_plan, new_interval) {
            PlanChange::NewYearlyCheckout => {}
            PlanChange::YearlyToMonthly => {
                return Err(BillingError::InvalidRequest(
                    "A yearly plan can't be switched to monthly from the dashboard".to_string(),
                ));
            }
            _ => {
                return Err(BillingError::InvalidRequest(
                    "You already have a plan - use change plan instead".to_string(),
                ));
            }
        }
    }

    // The browser's Meta cookies go with the checkout, so the payment can
    // be matched to the ad it came from (meta_capi.rs).
    let ad = AdClick::from_headers(&headers);
    let email: Option<String> = query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten();
    let checkout = create_checkout_for(&body.product_id, user_id, email.as_deref(), &ad)
        .await
        .map_err(|e| match e {
            CreateCheckoutError::MissingApiKey => {
                eprintln!("CREEM_API_KEY not set");
                BillingError::InternalError("Checkout is temporarily unavailable".to_string())
            }
            CreateCheckoutError::RequestFailed(msg) => {
                eprintln!("Creem checkout request failed: {}", msg);
                BillingError::ServiceUnavailable("Checkout provider is unreachable".to_string())
            }
            CreateCheckoutError::CreemRejected(msg) => {
                eprintln!("Creem rejected checkout: {}", msg);
                BillingError::InvalidRequest(format!("Checkout request rejected: {}", msg))
            }
        })?;

    Ok(Json(json!({ "checkout_url": checkout.checkout_url })))
}

/// POST /api/v1/webhooks/creem
///
/// It's the real, external endpoint Creem calls whenever something happens
/// to a subscription - payments succeeding or failing,
/// cancellations - and it's what keeps your own database's picture of
/// who's paying or lost access genuinely in sync with what's
/// real on Creem's side.
///
/// It first confirms the webhook is genuinely, verifiably from Creem
/// itself, then routes whichever event actually occurred to its own
/// specific handler. Once verification passes, nothing below it can fail
/// the whole request - a real problem handling one specific event gets
/// logged, not thrown back at Creem, since a failed response would just
/// cause endless retries of the same event.
pub async fn creem_webhook(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, WebhookError> {
    let event = verify_and_parse_webhook(&headers, &body).await?;

    if !mark_event_processed_if_new(&pool, &event.id).await {
        eprintln!(
            "Webhook event {} already processed - skipping duplicate",
            event.id
        );
        return Ok(StatusCode::OK);
    }

    report_payment_to_meta(&pool, &event.event_type, &event.object).await;

    // Extra team seats are their own Creem subscription, never a plan
    // (team.rs): handled there, and nothing below runs for them.
    if handle_seat_webhook(&pool, &event.event_type, &event.object).await {
        return Ok(StatusCode::OK);
    }

    match event.event_type.as_str() {
        "checkout.completed" => {
            eprintln!(
                "checkout.completed received for checkout {} - no action taken (subscription.paid handles granting access)",
                event.id
            );
        }
        "subscription.active" | "subscription.paid" => {
            if let Some(parsed) = extract_subscription(&event.event_type, &event.object) {
                handle_subscription_granted(&pool, &parsed).await;
                sync_seats_for(&pool, &parsed).await;
            }
        }
        "subscription.past_due" => {
            if let Some(parsed) = extract_subscription(&event.event_type, &event.object) {
                handle_subscription_past_due(&pool, &parsed).await;
                sync_seats_for(&pool, &parsed).await;
            }
        }
        "subscription.paused" | "subscription.expired" | "subscription.canceled" => {
            if let Some(parsed) = extract_subscription(&event.event_type, &event.object) {
                handle_subscription_lost(&pool, &parsed, &event.event_type).await;
                sync_seats_for(&pool, &parsed).await;
            }
        }
        "subscription.update" => {
            if let Some(parsed) = extract_subscription(&event.event_type, &event.object) {
                handle_subscription_update(&pool, &parsed).await;
                sync_seats_for(&pool, &parsed).await;
            }
        }
        // Canceled for the end of the period (also from Creem's own
        // customer portal): the plan stays active until then.
        "subscription.scheduled_cancel" => {
            if let Some(parsed) = extract_subscription(&event.event_type, &event.object) {
                let already: bool = query_scalar(
                    "SELECT cancel_at IS NOT NULL FROM subscriptions WHERE creem_subscription_id = $1",
                )
                .bind(&parsed.id)
                .fetch_optional(&pool)
                .await
                .ok()
                .flatten()
                .unwrap_or(false);
                handle_subscription_update(&pool, &parsed).await;
                if let Some(owner_id) =
                    crate::services::features::billing::extract_metadata_user_id(&parsed)
                {
                    end_seats_with_plan(&pool, owner_id).await;
                    // Canceled outside Safely: the emails were not sent yet.
                    if !already {
                        tell_plan_ending(&pool, owner_id, &parsed.id).await;
                    }
                }
            }
        }
        "refund.created" => {
            eprintln!("refund.created received for event {}", event.id);
        }
        other => {
            eprintln!("Unhandled Creem event type: {}", other);
        }
    }

    Ok(StatusCode::OK)
}

/// After a plan event: ends the owner's extra seats once they no longer
/// have a Team plan (team.rs sync_extra_seats), and takes the person off
/// a team they were invited to once they pay for their own plan
/// (team.rs leave_team_if_paying).
async fn sync_seats_for(
    pool: &Pool<Postgres>,
    parsed: &crate::models::billing::ParsedSubscription,
) {
    if let Some(owner_id) = crate::services::features::billing::extract_metadata_user_id(parsed) {
        sync_extra_seats(pool, owner_id).await;
        leave_team_if_paying(pool, owner_id).await;
    }
}

/// A paid order (checkout.completed, subscription.paid) goes to Meta as a
/// "Purchase" (meta_capi.rs). Sent in the background, so Creem gets its
/// answer at once; nothing is sent without META_CAPI_TOKEN.
async fn report_payment_to_meta(pool: &Pool<Postgres>, event_type: &str, object: &Value) {
    let Some(payment) = payment_in(event_type, object) else {
        return;
    };
    let metadata = object.get("metadata").cloned().unwrap_or(Value::Null);
    let user_id = metadata
        .get("safely_user_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    // The account's email; Creem's customer email if the account isn't found.
    let mut email = None;
    if let Some(id) = user_id
        .as_deref()
        .and_then(|id| id.parse::<uuid::Uuid>().ok())
    {
        email = query_scalar::<_, String>("SELECT email FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    }
    if email.is_none() {
        email = object
            .pointer("/customer/email")
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    let ad = AdClick::from_metadata(&metadata);
    tokio::spawn(send_purchase(payment, email, user_id, ad));
}

/// POST /api/v1/billing/cancel-subscription
///
/// It cancels the person's active subscription by telling Creem to actually
/// stop billing them, then immediately updating our own database to match,
/// rather than waiting for Creem's webhook to confirm something we already
/// know for certain, since we're the ones who just caused it.
///
/// It confirms who's genuinely signed in, finds their real, currently
/// active subscription, tells Creem to cancel it, updates our own database
/// right away to reflect that, and tries to send a real confirmation email
/// although a failed email doesn't undo the cancellation itself, since
/// that part has already genuinely succeeded by then.
pub async fn cancel_subscription_handler(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<Value>, BillingError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| BillingError::InternalError("Failed to verify session".to_string()))?
        .ok_or(BillingError::Unauthorized)?;

    let row: Option<(String, Option<DateTime<Utc>>)> = query_as(
        "SELECT creem_subscription_id, cancel_at FROM subscriptions
         WHERE user_id = $1 AND status IN ('active', 'past_due')
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .map_err(|_| BillingError::InternalError("Failed to look up subscription".to_string()))?;

    let (sub_id, already_ending) =
        row.ok_or_else(|| BillingError::NotFound("No active subscription found".to_string()))?;
    // Already canceled for the end of the period: nothing more to do.
    if let Some(ends) = already_ending {
        return Ok(Json(json!({ "success": true, "cancel_at": ends })));
    }

    // The plan keeps working until the end of the period the person paid
    // for (like Netflix or Spotify); Creem ends it then.
    cancel_at_period_end_with_creem(&sub_id).await?;

    let ends: Option<DateTime<Utc>> = query_scalar(
        "UPDATE subscriptions SET cancel_at = COALESCE(current_period_end, NOW()), canceled_at = NOW(), updated_at = NOW()
         WHERE creem_subscription_id = $1
         RETURNING cancel_at",
    )
    .bind(&sub_id)
    .fetch_one(&pool)
    .await
    .map_err(|_| BillingError::InternalError("Failed to update subscription".to_string()))?;

    // The team's extra seats end on the same day.
    end_seats_with_plan(&pool, user_id).await;
    tell_plan_ending(&pool, user_id, &sub_id).await;

    Ok(Json(json!({ "success": true, "cancel_at": ends })))
}

/// "Dec 13, 2026".
fn day_in_words(day: DateTime<Utc>) -> String {
    day.format("%b %-d, %Y").to_string()
}

/// Emails about a plan canceled for the end of the period: the owner
/// (confirmation, with the end date) and every team member who uses it.
async fn tell_plan_ending(pool: &Pool<Postgres>, owner_id: uuid::Uuid, sub_id: &str) {
    let row: Option<(String, Option<DateTime<Utc>>)> =
        query_as("SELECT plan_name, cancel_at FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(sub_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    let (plan, ends) = row.unwrap_or_else(|| ("Team".to_string(), None));
    let ends_on = ends.map(day_in_words);
    let members = team_member_emails(pool, owner_id).await;
    if let Some(email) = fetch_subscriber_email(pool, sub_id).await {
        if let Err(e) =
            send_subscription_canceled_email(&email, ends_on.as_deref(), members.len()).await
        {
            eprintln!("Failed to send cancellation confirmation email: {:?}", e);
        }
        if let Some(ends_on) = ends_on.as_deref() {
            for member in &members {
                if let Err(e) = send_team_plan_ending_email(member, &email, &plan, ends_on).await {
                    eprintln!("Failed to send the team plan ending email: {:?}", e);
                }
            }
        }
    }
}

/// POST /api/v1/billing/resume-subscription
///
/// "Keep my plan": undoes a cancellation that has not happened yet. The
/// plan (and the team's extra seats) simply keep renewing.
pub async fn resume_subscription_handler(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<Value>, BillingError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| BillingError::InternalError("Failed to verify session".to_string()))?
        .ok_or(BillingError::Unauthorized)?;

    let sub_id: Option<String> = query_scalar(
        "SELECT creem_subscription_id FROM subscriptions
         WHERE user_id = $1 AND status IN ('active', 'past_due') AND cancel_at IS NOT NULL
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .map_err(|_| BillingError::InternalError("Failed to look up subscription".to_string()))?;
    let sub_id =
        sub_id.ok_or_else(|| BillingError::NotFound("No plan is waiting to end".to_string()))?;

    resume_with_creem(&sub_id).await?;

    query(
        "UPDATE subscriptions SET cancel_at = NULL, canceled_at = NULL, updated_at = NOW()
         WHERE creem_subscription_id = $1",
    )
    .bind(&sub_id)
    .execute(&pool)
    .await
    .map_err(|_| BillingError::InternalError("Failed to update subscription".to_string()))?;

    keep_seats_with_plan(&pool, user_id).await;

    Ok(Json(json!({ "success": true })))
}

/// POST /api/v1/billing/manage-card
///
/// "Manage card": a link to the person's Creem customer portal, where
/// they change the card for their plan and their extra seats (both are
/// paid with the same email, so they belong to one Creem customer). Only
/// for someone who pays for a plan themselves.
pub async fn manage_card_handler(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<Value>, BillingError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| BillingError::InternalError("Failed to verify session".to_string()))?
        .ok_or(BillingError::Unauthorized)?;
    let customer_id: Option<String> = query_scalar(
        "SELECT creem_customer_id FROM subscriptions
         WHERE user_id = $1 AND creem_customer_id <> ''
         ORDER BY (status IN ('active', 'past_due')) DESC, updated_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .map_err(|_| BillingError::InternalError("Failed to look up subscription".to_string()))?;
    let customer_id =
        customer_id.ok_or_else(|| BillingError::NotFound("No card on file".to_string()))?;
    let url = customer_portal_link(&customer_id).await?;
    Ok(Json(json!({ "url": url })))
}

/// One subscription row for the status endpoint: id, plan, billing
/// interval, status, period end, scheduled downgrade (product, plan) and
/// when a canceled plan ends.
type StatusRow = (
    String,
    String,
    String,
    String,
    Option<DateTime<Utc>>,
    Option<String>,
    Option<String>,
    Option<DateTime<Utc>>,
);

/// GET /api/v1/billing/subscription-status
///
/// It reports back the person's real, current subscription state — which
/// plan they're on, what status it's in, and when it renews so the
/// frontend can show accurate billing info without guessing.
///
/// It also returns "usage" - plan ("Free", "Team" or "Enterprise"),
/// scans used, limit (null = unlimited) and reset date - so the
/// extension and dashboard can show e.g. "Free · 37/100".
///
/// It confirms who's genuinely signed in, looks up their most recent real
/// subscription, and if a scheduled downgrade's deferred period has
/// genuinely ended by now, actually applies it at this exact moment,
/// before reporting back the final, up-to-date picture.
pub async fn get_subscription_status(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<Value>, BillingError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| BillingError::InternalError("Failed to verify session".to_string()))?
        .ok_or(BillingError::Unauthorized)?;

    let row: Option<StatusRow> = query_as(
        "SELECT creem_subscription_id, plan_name, billing_interval, status::text, current_period_end,
                scheduled_product_id, scheduled_plan_name, cancel_at
         FROM subscriptions WHERE user_id = $1
         ORDER BY (status = 'active') DESC, created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .map_err(|_| BillingError::InternalError("Failed to look up subscription".to_string()))?;

    let usage = get_scan_usage(&pool, user_id).await;
    // A team member uses the owner's plan: {"plan": "Team", "owner_email": "...",
    // "cancel_at": when the owner's canceled plan ends, or null}.
    let shared = match shared_plan(&pool, user_id).await {
        Some((plan, owner_email)) => {
            let ends = team_plan_ends(&pool, plan_holder(&pool, user_id).await).await;
            Some(json!({ "plan": plan, "owner_email": owner_email, "cancel_at": ends }))
        }
        None => None,
    };

    let Some((
        sub_id,
        mut plan_name,
        billing_interval,
        status,
        current_period_end,
        scheduled_product_id,
        scheduled_plan_name,
        cancel_at,
    )) = row
    else {
        return Ok(Json(json!({
            "plan_name": null, "billing_interval": null, "status": null, "current_period_end": null,
            "scheduled_plan_name": null,
            "cancel_at": null,
            "usage": usage,
            "shared_plan": shared,
        })));
    };

    let mut scheduled_plan_name_response = scheduled_plan_name.clone();
    if let Some(new_plan_name) = apply_scheduled_downgrade_if_due(
        &pool,
        &sub_id,
        scheduled_product_id.as_deref(),
        scheduled_plan_name.as_deref(),
        current_period_end,
    )
    .await
    {
        plan_name = new_plan_name;
        scheduled_plan_name_response = None;
    }

    Ok(Json(json!({
        "plan_name": plan_name,
        "billing_interval": billing_interval,
        "status": status,
        "current_period_end": current_period_end,
        "scheduled_plan_name": scheduled_plan_name_response,
        // Canceled for the end of the period: when it ends (else null).
        "cancel_at": cancel_at,
        "usage": usage,
        "shared_plan": shared,
    })))
}

/// POST /api/v1/billing/change-plan
///
/// Switches an existing subscription to a different plan with the same
/// billing (monthly -> monthly, yearly -> yearly). Upgrades apply and
/// charge immediately; downgrades happen at the next renewal, matching
/// standard Netflix/Spotify-style downgrade behavior. Monthly -> yearly
/// goes through /billing/checkout instead.
pub async fn change_plan_handler(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(body): Json<ChangePlanBody>,
) -> Result<Json<Value>, BillingError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| BillingError::InternalError("Failed to verify session".to_string()))?
        .ok_or(BillingError::Unauthorized)?;

    let (new_plan, new_interval) = plan_for_product_id(&body.product_id)
        .ok_or_else(|| BillingError::InvalidRequest("Unknown plan".to_string()))?;

    let ending: bool = query_scalar(
        "SELECT EXISTS (SELECT 1 FROM subscriptions
         WHERE user_id = $1 AND status = 'active' AND cancel_at IS NOT NULL)",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap_or(false);
    // A plan waiting to end: "Keep my plan" first, then change it.
    if ending {
        return Err(BillingError::Conflict("plan_ending".to_string()));
    }

    let current: Option<(String, String, String)> = query_as(
        "SELECT creem_subscription_id, plan_name, billing_interval FROM subscriptions
         WHERE user_id = $1 AND status = 'active'
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .map_err(|_| BillingError::InternalError("Failed to look up subscription".to_string()))?;

    let (sub_id, current_plan, current_interval) = current
        .ok_or_else(|| BillingError::NotFound("No active subscription found".to_string()))?;
    let current_interval = BillingInterval::from_db(&current_interval);

    match classify_plan_change(&current_plan, current_interval, new_plan, new_interval) {
        PlanChange::UpgradeNow => {
            let result =
                apply_upgrade(&pool, &sub_id, &body.product_id, new_plan, new_interval).await;
            // Enterprise has no seat limit: the extra seats are ended.
            if result.is_ok() {
                sync_extra_seats(&pool, user_id).await;
            }
            result
        }
        PlanChange::DowngradeAtRenewal => {
            schedule_downgrade(&pool, &sub_id, &body.product_id, new_plan).await
        }
        PlanChange::NewYearlyCheckout => Err(BillingError::InvalidRequest(
            "Switching to yearly is done through checkout".to_string(),
        )),
        PlanChange::YearlyToMonthly => Err(BillingError::InvalidRequest(
            "A yearly plan can't be switched to monthly from the dashboard".to_string(),
        )),
        PlanChange::AlreadyOnPlan | PlanChange::Invalid => Err(BillingError::InvalidRequest(
            "Not a valid plan change".to_string(),
        )),
    }
}

/// The user's active paid plan and its billing, if they have one.
async fn active_plan(
    pool: &Pool<Postgres>,
    user_id: uuid::Uuid,
) -> Result<Option<(String, BillingInterval)>, BillingError> {
    let row: Option<(String, String)> = query_as(
        "SELECT plan_name, billing_interval FROM subscriptions
         WHERE user_id = $1 AND status = 'active'
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| BillingError::InternalError("Failed to look up subscription".to_string()))?;

    Ok(row.map(|(plan, interval)| (plan, BillingInterval::from_db(&interval))))
}

/// GET /api/v1/billing/product-ids
///
/// It hands the frontend the real Creem product IDs for each plan, so
/// checkout requests always use the actual, correct ID - rather than the
/// frontend needing to hardcode them itself.
pub async fn get_product_ids() -> Result<Json<Value>, BillingError> {
    // Monthly IDs must be set; yearly ones are null until they're added
    // to .env (the dashboard then says that option isn't available yet).
    for env_name in ["CREEM_TEAM_PRODUCT_ID", "CREEM_ENTERPRISE_PRODUCT_ID"] {
        if var(env_name).is_err() {
            return Err(BillingError::InternalError(format!("{} not set", env_name)));
        }
    }

    let mut ids = Map::new();
    for (env_name, plan, interval) in PLAN_PRODUCTS {
        ids.insert(
            product_ids_key(plan, interval),
            var(env_name).map(Value::String).unwrap_or(Value::Null),
        );
    }

    Ok(Json(Value::Object(ids)))
}
