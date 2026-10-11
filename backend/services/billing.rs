use crate::{
    errors::billing::{BillingError, WebhookError},
    models::billing::{
        CreateCheckoutRequest, CreateCheckoutResponse, CreemWebhookEvent, ParsedProduct,
        ParsedSubscription,
    },
    services::email::{send_payment_failed_email, send_subscription_ended_email},
};
use axum::{Json, body::Bytes, http::HeaderMap};
use chrono::{DateTime, NaiveDate, Utc};
use hex::encode;
use hmac::{Hmac, KeyInit, Mac};
use reqwest::Client;
use serde_json::{Value, from_str, from_value, json};
use sha2::Sha256;
use sqlx::{Error, Pool, Postgres, query, query_scalar};
use std::{env::var, str::from_utf8};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug)]
pub enum CreateCheckoutError {
    MissingApiKey,
    RequestFailed(String),
    CreemRejected(String),
}

#[derive(Debug)]
pub enum ScanLimitError {
    /// An active Team plan has used its monthly scans.
    LimitReached { limit: i32 },
    /// The user is on Free and has used all of this month's free
    /// scans. `resets_on` is when they come back: the same day of the
    /// month the user signed up.
    FreeLimitReached { limit: i32, resets_on: NaiveDate },
    /// The database could not be read, so the allowance could not be
    /// checked. The scan is refused rather than given away for free.
    Unavailable,
}

/// Free plan: this many scans a month, for anyone without an active
/// paid subscription. The month runs from the day the user signed up
/// (signed up on the 14th -> resets every 14th); unused scans do not
/// carry over.
pub const FREE_MONTHLY_SCANS: i32 = 5;

/// How often Creem bills a subscription. Scan limits are monthly on
/// both - a yearly plan still gets its scans back every month.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillingInterval {
    Month,
    Year,
}

impl BillingInterval {
    /// The value stored in subscriptions.billing_interval.
    pub fn as_str(self) -> &'static str {
        match self {
            BillingInterval::Month => "month",
            BillingInterval::Year => "year",
        }
    }

    pub fn from_db(value: &str) -> Self {
        if value == "year" {
            BillingInterval::Year
        } else {
            BillingInterval::Month
        }
    }
}

/// The 4 Creem products: (env var holding its product ID, plan, interval).
/// The key the dashboard uses for each is in product_ids_key.
pub const PLAN_PRODUCTS: [(&str, &str, BillingInterval); 4] = [
    ("CREEM_TEAM_PRODUCT_ID", "Team", BillingInterval::Month),
    (
        "CREEM_TEAM_YEARLY_PRODUCT_ID",
        "Team",
        BillingInterval::Year,
    ),
    (
        "CREEM_ENTERPRISE_PRODUCT_ID",
        "Enterprise",
        BillingInterval::Month,
    ),
    (
        "CREEM_ENTERPRISE_YEARLY_PRODUCT_ID",
        "Enterprise",
        BillingInterval::Year,
    ),
];

/// "Team" + Month -> "Team", "Team" + Year -> "TeamYearly" (the keys
/// returned by GET /billing/product-ids).
pub fn product_ids_key(plan: &str, interval: BillingInterval) -> String {
    match interval {
        BillingInterval::Month => plan.to_string(),
        BillingInterval::Year => format!("{}Yearly", plan),
    }
}

/// Which plan and interval one of our Creem product IDs is, using the
/// product IDs in .env. None if the ID is not one of the 4.
pub fn plan_for_product_id(product_id: &str) -> Option<(&'static str, BillingInterval)> {
    PLAN_PRODUCTS
        .iter()
        .find(|(env_name, _, _)| var(env_name).map(|id| id == product_id).unwrap_or(false))
        .map(|(_, plan, interval)| (*plan, *interval))
}

/// "Team", "Team Yearly", "Safely Team (annual)" -> "Team"; same for
/// Enterprise. Anything else is kept as it is (and gets no scans).
pub fn normalize_plan_name(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.contains("enterprise") {
        "Enterprise".to_string()
    } else if lower.contains("team") {
        "Team".to_string()
    } else {
        name.to_string()
    }
}

/// Creem's billing_period ("every-month", "every-year", ...) -> interval.
pub fn interval_from_billing_period(billing_period: Option<&str>) -> BillingInterval {
    match billing_period {
        Some(p) if p.to_lowercase().contains("year") => BillingInterval::Year,
        _ => BillingInterval::Month,
    }
}

/// The plan and interval of the product Creem sent. Our own product IDs
/// decide first; if the ID is not in .env, the product's name and
/// billing period are used instead.
pub fn plan_from_product(product: &ParsedProduct) -> (String, BillingInterval) {
    match plan_for_product_id(&product.id) {
        Some((plan, interval)) => (plan.to_string(), interval),
        None => (
            normalize_plan_name(&product.name),
            interval_from_billing_period(product.billing_period.as_deref()),
        ),
    }
}

fn plan_rank(plan: &str) -> u8 {
    match plan {
        "Team" => 1,
        "Enterprise" => 2,
        _ => 0,
    }
}

/// What a plan change from the dashboard does.
#[derive(Debug, PartialEq, Eq)]
pub enum PlanChange {
    /// Same plan and billing - nothing to do.
    AlreadyOnPlan,
    /// Team -> Enterprise (same billing): switch and charge now.
    UpgradeNow,
    /// Enterprise -> Team (same billing): switch at the next renewal.
    DowngradeAtRenewal,
    /// Monthly -> yearly: a new yearly checkout; the monthly plan is
    /// ended automatically once the yearly one is paid.
    NewYearlyCheckout,
    /// Yearly -> monthly: not done from the dashboard.
    YearlyToMonthly,
    /// Unknown plan.
    Invalid,
}

pub fn classify_plan_change(
    current_plan: &str,
    current_interval: BillingInterval,
    new_plan: &str,
    new_interval: BillingInterval,
) -> PlanChange {
    let (cur, new) = (plan_rank(current_plan), plan_rank(new_plan));
    if cur == 0 || new == 0 {
        return PlanChange::Invalid;
    }
    match (current_interval, new_interval) {
        (BillingInterval::Month, BillingInterval::Year) => PlanChange::NewYearlyCheckout,
        (BillingInterval::Year, BillingInterval::Month) => PlanChange::YearlyToMonthly,
        _ if new > cur => PlanChange::UpgradeNow,
        _ if new < cur => PlanChange::DowngradeAtRenewal,
        _ => PlanChange::AlreadyOnPlan,
    }
}

/// It's the one real step that actually talks to Creem — building a real,
/// working checkout session for a specific plan, tied to a specific
/// person, so Creem knows exactly who to bill and where to send them once
/// they've paid.
///
/// It builds the real request, attaching the person's actual Safely user
/// ID via metadata — this is what lets every future webhook event tied to
/// this subscription (checkout.completed, subscription.paid,
/// subscription.canceled, ...) reliably identify which account to update,
/// rather than guessing by email. It then sends this to Creem's real
/// checkout endpoint, and checks that Creem genuinely accepted it, rather
/// than assuming the request going out means it worked.
pub async fn create_checkout(
    product_id: &str,
    user_id: Uuid,
) -> Result<CreateCheckoutResponse, CreateCheckoutError> {
    let api_key = var("CREEM_API_KEY").map_err(|_| CreateCheckoutError::MissingApiKey)?;
    let base_url = var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());

    let request_body = CreateCheckoutRequest {
        product_id: product_id.to_string(),
        success_url: format!("{}/dashboard/?checkout=success", base_url),
        metadata: serde_json::json!({ "safely_user_id": user_id.to_string() }),
    };

    let client = Client::new();
    let creem_base_url =
        var("CREEM_API_BASE_URL").unwrap_or_else(|_| "https://test-api.creem.io".to_string());
    let response = client
        .post(format!("{}/v1/checkouts", creem_base_url))
        .header("x-api-key", api_key)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| CreateCheckoutError::RequestFailed(e.to_string()))?;

    if !response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(CreateCheckoutError::CreemRejected(text));
    }

    response
        .json::<CreateCheckoutResponse>()
        .await
        .map_err(|e| CreateCheckoutError::RequestFailed(e.to_string()))
}

/// Confirms a webhook request is genuinely, verifiably from Creem, and
/// hands back the real, parsed event if so.
///
/// It checks the secret is configured, the signature header exists, the
/// body is readable, the signature genuinely matches, and finally that
/// the body parses into a real event - failing at whichever specific
/// step actually went wrong.
pub async fn verify_and_parse_webhook(
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<CreemWebhookEvent, WebhookError> {
    let secret = var("CREEM_WEBHOOK_SECRET")
        .map_err(|_| WebhookError::Misconfigured("CREEM_WEBHOOK_SECRET not set".to_string()))?;

    let signature = headers
        .get("creem-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(WebhookError::MissingSignature)?;

    let raw_body = from_utf8(body).map_err(|_| WebhookError::InvalidBody)?;

    if !verify_creem_signature(raw_body, signature, &secret) {
        eprintln!("Creem webhook: signature verification FAILED");
        return Err(WebhookError::InvalidSignature);
    }

    from_str(raw_body)
        .map_err(|e| WebhookError::InvalidPayload(format!("Invalid webhook payload: {}", e)))
}

/// Verifies that a webhook request genuinely came from Creem, and
/// wasn't forged by someone who simply knows your webhook URL.
///
/// Creem signs every webhook using HMAC-SHA256, with your webhook
/// secret as the key and the raw request body as the message - this
/// recomputes that same signature independently and compares it
/// against what Creem actually sent in the `creem-signature` header.
/// If they don't match exactly, the request is rejected outright,
/// before any real event-handling logic ever runs.
pub fn verify_creem_signature(raw_body: &str, received_signature: &str, secret: &str) -> bool {
    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };

    mac.update(raw_body.as_bytes());
    let computed = encode(mac.finalize().into_bytes());

    computed == received_signature
}

/// Pulls the subscription data out of a webhook event - the data can
/// arrive in one of two places, depending on the event type.
///
/// A "subscription.*" event has it directly at the top level. A
/// "checkout.completed" event has it nested one level deeper, inside a
/// "subscription" field. This checks which one applies and pulls it
/// out from the right spot. If the event type is something else, or
/// the data doesn't actually look like a real subscription, it just
/// returns None.
pub fn extract_subscription(event_type: &str, object: &Value) -> Option<ParsedSubscription> {
    let subscription_value = if event_type.starts_with("subscription.") {
        object.clone()
    } else if event_type == "checkout.completed" {
        object.get("subscription")?.clone()
    } else {
        return None;
    };

    from_value(subscription_value).ok()
}

/// Handles a subscription becoming active - a
/// payment going through, or a subscription otherwise turning paid -
/// by saving the current state to our database.
///
/// It looks for the real Safely user ID inside the event's metadata.
/// If it's there, it saves the subscription using Creem's own status
/// directly, rather than guessing based on which event fired. If the
/// user ID is missing, it just logs that and stops, since there's no
/// account to attach this event to.
pub async fn handle_subscription_granted(pool: &Pool<Postgres>, parsed: &ParsedSubscription) {
    match extract_metadata_user_id(parsed) {
        Some(user_id) => {
            if let Err(e) = upsert_subscription(pool, user_id, parsed, &parsed.status).await {
                eprintln!("Failed to upsert subscription: {}", e);
                return;
            }
            if parsed.status == "active" {
                end_replaced_subscriptions(pool, user_id, &parsed.id).await;
            }
        }
        None => eprintln!(
            "Subscription event is missing safely_user_id in metadata. It can't link to an account"
        ),
    }
}

/// When someone moves from a monthly plan to a yearly one, the yearly
/// plan is a new Creem subscription. Once it is active, this cancels
/// their other (old monthly) subscription at Creem, so they are never
/// billed twice, and marks it canceled here. The old one is marked
/// canceled first, so its "subscription.canceled" webhook sends no
/// "your subscription ended" email.
pub async fn end_replaced_subscriptions(
    pool: &Pool<Postgres>,
    user_id: Uuid,
    new_subscription_id: &str,
) {
    let old_ids: Vec<String> = query_scalar(
        "SELECT creem_subscription_id FROM subscriptions
         WHERE user_id = $1 AND creem_subscription_id <> $2
           AND status IN ('active', 'past_due')",
    )
    .bind(user_id)
    .bind(new_subscription_id)
    .fetch_all(pool)
    .await
    .unwrap_or_else(|e| {
        eprintln!("Failed to look up replaced subscriptions: {}", e);
        Vec::new()
    });

    for old_id in old_ids {
        let _ = query(
            "UPDATE subscriptions SET status = 'canceled'::subscription_status,
             canceled_at = NOW(), updated_at = NOW()
             WHERE creem_subscription_id = $1",
        )
        .bind(&old_id)
        .execute(pool)
        .await;

        if let Err(e) = cancel_with_creem(&old_id).await {
            eprintln!(
                "Failed to cancel replaced subscription {} at Creem - cancel it in the Creem dashboard: {:?}",
                old_id, e
            );
        }
    }
}

/// Pulls the real Safely user ID out of a subscription's metadata, if
/// it's genuinely present and valid.
///
/// It checks the metadata itself exists, then that a user ID string was
/// actually included inside it, then that string genuinely parses as a
/// real UUID - failing quietly back to None at whichever step doesn't
/// hold, rather than treating a missing or malformed piece as an error.
pub fn extract_metadata_user_id(parsed: &ParsedSubscription) -> Option<Uuid> {
    parsed
        .metadata
        .as_ref()
        .and_then(|m| m.safely_user_id.as_ref())
        .and_then(|s| Uuid::parse_str(s).ok())
}

/// Whether Creem's webhook is reporting a genuinely NEW billing
/// period rolling over (vs. just re-sending an update for the period
/// already on file) - this is what decides whether
/// scans_used_this_period gets reset to 0. A subscription with no
/// existing row yet always counts as a new period (there's nothing
/// to compare against); otherwise it's only new when the incoming
/// period end is genuinely later than the one already stored.
pub fn is_new_billing_period(
    existing_period_end: Option<Option<DateTime<Utc>>>,
    current_period_end: Option<DateTime<Utc>>,
) -> bool {
    match existing_period_end {
        None => true,
        Some(old_end) => match (old_end, current_period_end) {
            (Some(old), Some(new)) => new > old,
            _ => false,
        },
    }
}

/// Creates or updates a subscription row for a user, matched by
/// Creem's own subscription ID. This one function handles every real
/// state a subscription can be in - active, past_due, canceled, and so
/// on - so it's called from every event type that touches a
/// subscription, each time with whatever status that specific event
/// represents.
///
/// The plan is always stored as "Team" or "Enterprise" (see
/// plan_from_product), with billing_interval 'month' or 'year'.
///
/// - New billing period (renewal, or the first event): scans go back
///   to 0, the monthly scan count starts now (scan_anchor - a yearly
///   plan's scan months run from this moment), and a scheduled
///   downgrade (if any) becomes the plan.
/// - Otherwise the plan follows Creem's product - except while a
///   downgrade is scheduled to that product: the user keeps the plan
///   they paid for until the renewal.
pub async fn upsert_subscription(
    pool: &Pool<Postgres>,
    user_id: Uuid,
    parsed: &ParsedSubscription,
    status: &str,
) -> Result<(), Error> {
    let current_period_end = parsed
        .current_period_end_date
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let canceled_at = parsed
        .canceled_at
        .as_ref()
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let existing_period_end: Option<Option<DateTime<Utc>>> = query_scalar(
        "SELECT current_period_end FROM subscriptions WHERE creem_subscription_id = $1",
    )
    .bind(&parsed.id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let is_new_period = is_new_billing_period(existing_period_end, current_period_end);
    let (plan_name, interval) = plan_from_product(&parsed.product);

    query(
        "INSERT INTO subscriptions (
            id, user_id, creem_subscription_id, creem_customer_id,
            creem_product_id, plan_name, status, current_period_end,
            canceled_at, scans_used_this_period, billing_interval,
            scan_period_start, scan_anchor, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7::subscription_status, $8, $9, 0, $11, NOW(), NOW(), NOW(), NOW())
        ON CONFLICT (creem_subscription_id) DO UPDATE SET
            status = EXCLUDED.status,
            current_period_end = EXCLUDED.current_period_end,
            canceled_at = EXCLUDED.canceled_at,
            creem_product_id = CASE
                WHEN NOT $10 AND subscriptions.scheduled_product_id = EXCLUDED.creem_product_id
                    THEN subscriptions.creem_product_id
                ELSE EXCLUDED.creem_product_id END,
            plan_name = CASE
                WHEN NOT $10 AND subscriptions.scheduled_product_id = EXCLUDED.creem_product_id
                    THEN subscriptions.plan_name
                ELSE EXCLUDED.plan_name END,
            billing_interval = CASE
                WHEN NOT $10 AND subscriptions.scheduled_product_id = EXCLUDED.creem_product_id
                    THEN subscriptions.billing_interval
                ELSE EXCLUDED.billing_interval END,
            scheduled_product_id = CASE
                WHEN $10 OR subscriptions.scheduled_product_id IS DISTINCT FROM EXCLUDED.creem_product_id
                    AND subscriptions.creem_product_id IS DISTINCT FROM EXCLUDED.creem_product_id
                    THEN NULL
                ELSE subscriptions.scheduled_product_id END,
            scheduled_plan_name = CASE
                WHEN $10 OR subscriptions.scheduled_product_id IS DISTINCT FROM EXCLUDED.creem_product_id
                    AND subscriptions.creem_product_id IS DISTINCT FROM EXCLUDED.creem_product_id
                    THEN NULL
                ELSE subscriptions.scheduled_plan_name END,
            scans_used_this_period = CASE WHEN $10 THEN 0 ELSE subscriptions.scans_used_this_period END,
            scan_period_start = CASE WHEN $10 THEN NOW() ELSE subscriptions.scan_period_start END,
            scan_anchor = CASE WHEN $10 THEN NOW() ELSE subscriptions.scan_anchor END,
            updated_at = NOW()",
    )
    .bind(Uuid::now_v7())
    .bind(user_id)
    .bind(&parsed.id)
    .bind(&parsed.customer.id)
    .bind(&parsed.product.id)
    .bind(&plan_name)
    .bind(status)
    .bind(current_period_end)
    .bind(canceled_at)
    .bind(is_new_period)
    .bind(interval.as_str())
    .execute(pool)
    .await?;

    Ok(())
}

/// Handles the moment Creem tells us a payment genuinely failed - the
/// subscription isn't canceled yet, but billing is now at risk, so the
/// person needs a real heads-up and a way to fix it.
///
/// It builds a real link straight to the billing-management section of
/// the dashboard, sends a payment-failed email to the customer's real
/// address, and separately updates our own database to reflect the
/// "past_due" status - these two steps are genuinely independent, so a
/// failure in one (like the email not sending) doesn't stop the other
/// from completing.
pub async fn handle_subscription_past_due(pool: &Pool<Postgres>, parsed: &ParsedSubscription) {
    let previous_status: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(&parsed.id)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);

    if previous_status.as_deref() != Some("past_due") {
        let portal_url = format!(
            "{}/dashboard/?manage_billing=1",
            var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
        );
        if let Err(e) = send_payment_failed_email(&parsed.customer.email, &portal_url).await {
            eprintln!("Failed to send payment-failed email: {:?}", e);
        }
    }

    if let Some(user_id) = extract_metadata_user_id(parsed) {
        if let Err(e) = upsert_subscription(pool, user_id, parsed, "past_due").await {
            eprintln!("Failed to upsert subscription: {}", e);
        }
    }
}

/// Handles the three real ways a subscription stops being active -
/// paused, expired, or genuinely canceled and, for cancellation
/// specifically, decides whether this is news worth emailing about.
///
/// It maps the event type to the correct status, checks what status
/// this subscription already had before this event arrived, updates our
/// own database to the new status, and only for a genuinely new
/// cancellation, one we didn't already know about. It sends the final
/// "subscription ended" email. If they already canceled through our own
/// site, this is just Creem confirming what we already know, so no
/// second email goes out.
pub async fn handle_subscription_lost(
    pool: &Pool<Postgres>,
    parsed: &ParsedSubscription,
    event_type: &str,
) {
    let status = match event_type {
        "subscription.paused" => "paused",
        "subscription.expired" => "expired",
        _ => "canceled",
    };

    let previous_status: Option<String> =
        query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
            .bind(&parsed.id)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);

    if let Some(user_id) = extract_metadata_user_id(parsed) {
        if let Err(e) = upsert_subscription(pool, user_id, parsed, status).await {
            eprintln!("Failed to upsert subscription: {}", e);
        }
    }

    if event_type == "subscription.canceled" && previous_status.as_deref() != Some("canceled") {
        if let Err(e) = send_subscription_ended_email(&parsed.customer.email).await {
            eprintln!("Failed to send subscription-ended email: {:?}", e);
        }
    }
}

/// Keeps our own database in sync whenever a subscription changes on
/// Creem's side through some path other than our own endpoints - most
/// often, someone editing it directly from Creem's own dashboard.
///
/// It pulls the real user ID out of the event's metadata, and if
/// present, saves whatever status Creem genuinely reports right now.
/// No separate logic beyond that.
pub async fn handle_subscription_update(pool: &Pool<Postgres>, parsed: &ParsedSubscription) {
    if let Some(user_id) = extract_metadata_user_id(parsed) {
        if let Err(e) = upsert_subscription(pool, user_id, parsed, &parsed.status).await {
            eprintln!("Failed to sync subscription.update: {}", e);
        }
    }
}

/// It's the one real step that actually talks to Creem — telling them,
/// for real, to stop billing this specific subscription.
///
/// It builds the real request with the correct API key, sends it to Creem's
/// real cancellation endpoint, and checks that Creem genuinely accepted it,
/// rather than just assuming the request going out means it worked.
pub async fn cancel_with_creem(sub_id: &str) -> Result<(), BillingError> {
    let api_key = var("CREEM_API_KEY")
        .map_err(|_| BillingError::InternalError("CREEM_API_KEY not set".to_string()))?;
    let creem_base_url =
        var("CREEM_API_BASE_URL").unwrap_or_else(|_| "https://test-api.creem.io".to_string());

    let response = Client::new()
        .post(format!(
            "{}/v1/subscriptions/{}/cancel",
            creem_base_url, sub_id
        ))
        .header("x-api-key", api_key)
        .send()
        .await
        .map_err(|_| BillingError::ServiceUnavailable("Could not reach Creem".to_string()))?;

    if !response.status().is_success() {
        return Err(BillingError::ServiceUnavailable(
            "Creem rejected the cancellation".to_string(),
        ));
    }

    Ok(())
}

/// Finds the real email address of whoever owns this subscription, so
/// a cancellation confirmation actually reaches the right person.
///
/// It joins the subscription back to its real user and returns their
/// email if found - returning nothing either way if it wasn't found,
/// or if a real database error happened, since a missing email should
/// only ever mean the confirmation gets skipped, never that anything
/// about the cancellation itself failed. A genuine database error still
/// gets logged, even though it's not treated as a hard failure here.
pub async fn fetch_subscriber_email(pool: &Pool<Postgres>, sub_id: &str) -> Option<String> {
    sqlx::query_scalar(
        "SELECT u.email FROM users u
         JOIN subscriptions s ON s.user_id = u.id
         WHERE s.creem_subscription_id = $1",
    )
    .bind(sub_id)
    .fetch_optional(pool)
    .await
    .unwrap_or_else(|e| {
        eprintln!("Failed to fetch subscriber email: {}", e);
        None
    })
}

/// Backup for a scheduled downgrade: normally the renewal webhook
/// applies it (see upsert_subscription). If the paid period has ended
/// and that webhook has not arrived yet, this switches our own row to
/// the scheduled plan and hands back its name. Creem was already told
/// about the change when the downgrade was scheduled, so nothing is
/// sent to Creem here.
pub async fn apply_scheduled_downgrade_if_due(
    pool: &Pool<Postgres>,
    sub_id: &str,
    scheduled_product_id: Option<&str>,
    scheduled_plan_name: Option<&str>,
    current_period_end: Option<DateTime<Utc>>,
) -> Option<String> {
    let (sched_product_id, sched_plan_name, period_end) = (
        scheduled_product_id?,
        scheduled_plan_name?,
        current_period_end?,
    );

    if Utc::now() < period_end {
        return None;
    }

    let interval = plan_for_product_id(sched_product_id).map(|(_, i)| i.as_str());

    let _ = query(
        "UPDATE subscriptions SET plan_name = $1, creem_product_id = $2,
         billing_interval = COALESCE($4, billing_interval),
         scheduled_product_id = NULL, scheduled_plan_name = NULL, updated_at = NOW()
         WHERE creem_subscription_id = $3",
    )
    .bind(sched_plan_name)
    .bind(sched_product_id)
    .bind(sub_id)
    .bind(interval)
    .execute(pool)
    .await;

    Some(sched_plan_name.to_string())
}

/// Tells Creem to switch a subscription to a different product - used
/// for upgrades ("proration-charge-immediately": charged now) and for
/// scheduling downgrades ("proration-none": the new price from the next
/// renewal). This just makes the API call and reports back whether
/// Creem accepted it.
pub async fn change_creem_subscription_product(
    sub_id: &str,
    new_product_id: &str,
    update_behavior: &str,
) -> Result<(), String> {
    let api_key = var("CREEM_API_KEY").map_err(|_| "CREEM_API_KEY not set".to_string())?;
    let creem_base_url =
        var("CREEM_API_BASE_URL").unwrap_or_else(|_| "https://test-api.creem.io".to_string());
    let client = Client::new();

    let response = client
        .post(format!(
            "{}/v1/subscriptions/{}/upgrade",
            creem_base_url, sub_id
        ))
        .header("x-api-key", api_key)
        .json(&json!({
            "product_id": new_product_id,
            "update_behavior": update_behavior,
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!("Creem rejected plan change: {}", text));
    }

    Ok(())
}

/// Applies an upgrade (Team -> Enterprise) immediately, both at Creem
/// and in our own database - unlike a downgrade, an upgrade never waits
/// for the current period to end.
///
/// It tells Creem to make the change now, with proration so the person
/// is charged correctly for the switch, then updates our own row to
/// match, clearing any previously scheduled downgrade, since a fresh
/// upgrade supersedes it.
pub async fn apply_upgrade(
    pool: &Pool<Postgres>,
    sub_id: &str,
    product_id: &str,
    plan_name: &str,
    interval: BillingInterval,
) -> Result<Json<Value>, BillingError> {
    change_creem_subscription_product(sub_id, product_id, "proration-charge-immediately")
        .await
        .map_err(|e| {
            eprintln!("Failed to upgrade subscription: {}", e);
            BillingError::ServiceUnavailable("Creem rejected the upgrade".to_string())
        })?;

    query(
        "UPDATE subscriptions SET plan_name = $1, creem_product_id = $2, billing_interval = $4,
         scheduled_product_id = NULL, scheduled_plan_name = NULL, updated_at = NOW()
         WHERE creem_subscription_id = $3",
    )
    .bind(plan_name)
    .bind(product_id)
    .bind(sub_id)
    .bind(interval.as_str())
    .execute(pool)
    .await
    .map_err(|_| BillingError::InternalError("Failed to update subscription".to_string()))?;

    Ok(Json(json!({ "applied": "immediately" })))
}

/// Schedules a downgrade (Enterprise -> Team). Creem is told now, with
/// no proration, so the next renewal is billed at the cheaper price;
/// the user keeps Enterprise until then (upsert_subscription switches
/// the plan when the renewal arrives).
pub async fn schedule_downgrade(
    pool: &Pool<Postgres>,
    sub_id: &str,
    product_id: &str,
    plan_name: &str,
) -> Result<Json<Value>, BillingError> {
    // Saved before telling Creem, so the "subscription.update" webhook
    // Creem sends right after already sees the downgrade as scheduled.
    query(
        "UPDATE subscriptions SET scheduled_product_id = $1, scheduled_plan_name = $2, updated_at = NOW()
         WHERE creem_subscription_id = $3",
    )
    .bind(product_id)
    .bind(plan_name)
    .bind(sub_id)
    .execute(pool)
    .await
    .map_err(|_| BillingError::InternalError("Failed to schedule downgrade".to_string()))?;

    if let Err(e) = change_creem_subscription_product(sub_id, product_id, "proration-none").await {
        eprintln!("Failed to schedule downgrade: {}", e);
        let _ = query(
            "UPDATE subscriptions SET scheduled_product_id = NULL, scheduled_plan_name = NULL, updated_at = NOW()
             WHERE creem_subscription_id = $1",
        )
        .bind(sub_id)
        .execute(pool)
        .await;
        return Err(BillingError::ServiceUnavailable(
            "Creem rejected the plan change".to_string(),
        ));
    }

    Ok(Json(json!({ "applied": "scheduled" })))
}

/// Atomically records that a specific webhook event has been
/// processed, and reports whether this is genuinely the FIRST time
/// we've seen it. Uses the database's own uniqueness guarantee
/// (ON CONFLICT DO NOTHING) rather than a separate "check, then
/// write" pair of steps - this is what actually closes the rare
/// race condition where two, truly simultaneous deliveries of the
/// same event could otherwise both slip through.
pub async fn mark_event_processed_if_new(pool: &Pool<Postgres>, event_id: &str) -> bool {
    let result = query(
        "INSERT INTO webhook_events_processed (event_id) VALUES ($1)
         ON CONFLICT (event_id) DO NOTHING",
    )
    .bind(event_id)
    .execute(pool)
    .await;

    match result {
        Ok(res) => res.rows_affected() > 0,
        Err(e) => {
            eprintln!("Failed to record webhook event {}: {}", event_id, e);
            true
        }
    }
}

/// Monthly scan limit of an active paid plan. None means unlimited.
/// The same on monthly and yearly billing. (Anyone without an active
/// paid plan is on Free - see FREE_MONTHLY_SCANS.)
pub fn scan_limit_for_plan(plan_name: &str) -> Option<i32> {
    match plan_name {
        "Team" => Some(750),
        "Enterprise" => None,
        _ => Some(0),
    }
}

/// SQL: whole months from `$anchor` to now. 14 Oct -> 13 Nov is 0,
/// 14 Oct -> 14 Nov is 1.
macro_rules! months_since {
    ($anchor:literal) => {
        concat!(
            "(EXTRACT(YEAR FROM age(NOW(), ",
            $anchor,
            ")) * 12 + EXTRACT(MONTH FROM age(NOW(), ",
            $anchor,
            ")))::int"
        )
    };
}

/// SQL: when the current scan month started - `$anchor` plus whole
/// months, so it always falls on the anchor's day of the month (or the
/// month's last day when that day doesn't exist, e.g. 31 -> 30 Nov).
macro_rules! scan_month_start {
    ($anchor:literal) => {
        concat!(
            "(",
            $anchor,
            " + make_interval(months => ",
            months_since!($anchor),
            "))"
        )
    };
}

/// SQL: when the next scan month starts - when the scans come back.
macro_rules! next_scan_month_start {
    ($anchor:literal) => {
        concat!(
            "(",
            $anchor,
            " + make_interval(months => ",
            months_since!($anchor),
            " + 1))"
        )
    };
}

/// SQL: true when a yearly plan has reached a new scan month, so its
/// count starts again. A yearly plan's months run from scan_anchor (when
/// it was bought or last renewed). Monthly plans reset on renewal
/// instead, which is already their subscribe date.
macro_rules! yearly_month_rolled {
    () => {
        concat!(
            "(billing_interval = 'year' AND scan_period_start < ",
            scan_month_start!("scan_anchor"),
            ")"
        )
    };
}

/// Checks whether this user can run one more scan right now, and if
/// so, counts it in the same query, so two scans at the same moment
/// can never both slip past the limit.
///
/// - An ACTIVE paid subscription (Team / Enterprise, monthly or
///   yearly) uses its own monthly limit.
/// - Everyone else (no subscription, past_due, canceled...) is on
///   Free: FREE_MONTHLY_SCANS a month, counted from their sign-up day.
pub async fn check_and_increment_scan_usage(
    pool: &Pool<Postgres>,
    user_id: Uuid,
) -> Result<(), ScanLimitError> {
    let paid: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, plan_name FROM subscriptions
         WHERE user_id = $1 AND status = 'active'
         ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        eprintln!("Failed to look up subscription for scan check: {}", e);
        ScanLimitError::Unavailable
    })?;

    match paid {
        Some((subscription_id, plan_name)) => {
            use_paid_scan(pool, subscription_id, &plan_name).await
        }
        None => use_free_scan(pool, user_id).await,
    }
}

/// One scan on an active Team / Enterprise plan. A single UPDATE adds
/// 1 - or, on a yearly plan that has reached a new scan month, starts
/// the count again at 1 - but only while under the plan's limit. If the
/// WHERE stops the update, the scan is refused.
async fn use_paid_scan(
    pool: &Pool<Postgres>,
    subscription_id: Uuid,
    plan_name: &str,
) -> Result<(), ScanLimitError> {
    let limit = scan_limit_for_plan(plan_name);

    let result = query(concat!(
        "UPDATE subscriptions SET
             scans_used_this_period = CASE WHEN ",
        yearly_month_rolled!(),
        " THEN 1 ELSE scans_used_this_period + 1 END,
             scan_period_start = CASE WHEN ",
        yearly_month_rolled!(),
        " THEN ",
        scan_month_start!("scan_anchor"),
        " ELSE scan_period_start END
         WHERE id = $1 AND status = 'active'
           AND ($2::int IS NULL OR ",
        yearly_month_rolled!(),
        " OR scans_used_this_period < $2)"
    ))
    .bind(subscription_id)
    .bind(limit)
    .execute(pool)
    .await;

    match (result, limit) {
        (Ok(res), Some(limit)) if res.rows_affected() == 0 => {
            Err(ScanLimitError::LimitReached { limit })
        }
        (Err(e), Some(_)) => {
            eprintln!("Failed to record paid scan: {}", e);
            Err(ScanLimitError::Unavailable)
        }
        // Unlimited: a failed count is logged, never a reason to refuse.
        (Err(e), None) => {
            eprintln!("Failed to record paid scan: {}", e);
            Ok(())
        }
        _ => Ok(()),
    }
}

/// One scan on the Free plan. The Free month runs from the user's
/// sign-up date (users.created_at), so it resets on that day every
/// month. A single query creates the user's row the first time, starts
/// a fresh count when a new Free month has begun, or adds 1 - but only
/// while they are under FREE_MONTHLY_SCANS. If the WHERE stops the
/// update, no row changes and the scan is refused.
async fn use_free_scan(pool: &Pool<Postgres>, user_id: Uuid) -> Result<(), ScanLimitError> {
    let result = query(concat!(
        "INSERT INTO free_scan_usage (user_id, period_start, scans_used, updated_at)
         SELECT u.id, ",
        scan_month_start!("u.created_at"),
        ", 1, NOW() FROM users u WHERE u.id = $1
         ON CONFLICT (user_id) DO UPDATE SET
             scans_used = CASE
                 WHEN free_scan_usage.period_start < EXCLUDED.period_start THEN 1
                 ELSE free_scan_usage.scans_used + 1
             END,
             period_start = EXCLUDED.period_start,
             updated_at = NOW()
         WHERE free_scan_usage.period_start < EXCLUDED.period_start
            OR free_scan_usage.scans_used < $2"
    ))
    .bind(user_id)
    .bind(FREE_MONTHLY_SCANS)
    .execute(pool)
    .await
    .map_err(|e| {
        eprintln!("Failed to record free scan: {}", e);
        ScanLimitError::Unavailable
    })?;

    if result.rows_affected() > 0 {
        return Ok(());
    }

    let resets_on = free_resets_on(pool, user_id)
        .await
        .unwrap_or_else(|| Utc::now().date_naive());
    Err(ScanLimitError::FreeLimitReached {
        limit: FREE_MONTHLY_SCANS,
        resets_on,
    })
}

/// The day the user's Free scans come back: the next monthly
/// anniversary of their sign-up date. None if it can't be read.
async fn free_resets_on(pool: &Pool<Postgres>, user_id: Uuid) -> Option<NaiveDate> {
    let next: Option<DateTime<Utc>> = query_scalar(concat!(
        "SELECT ",
        next_scan_month_start!("created_at"),
        " FROM users WHERE id = $1"
    ))
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .unwrap_or_else(|e| {
        eprintln!("Failed to read the Free reset date: {}", e);
        None
    });
    next.map(|d| d.date_naive())
}

/// What the dashboard and extension show: which plan's allowance is in
/// use, monthly or yearly billing, how many scans are used this month,
/// the limit (null = unlimited) and when the count resets. Read-only -
/// never counts a scan.
///
/// resets_on: Free - the next monthly anniversary of the sign-up date;
/// monthly plan - its renewal date; yearly plan - the next monthly
/// anniversary of when it was bought (never later than its renewal).
pub async fn get_scan_usage(pool: &Pool<Postgres>, user_id: Uuid) -> Value {
    let paid: Option<(String, String, i32, Option<DateTime<Utc>>)> = sqlx::query_as(concat!(
        "SELECT plan_name, billing_interval,
             CASE WHEN ",
        yearly_month_rolled!(),
        " THEN 0 ELSE scans_used_this_period END,
             CASE WHEN billing_interval = 'year'
                 THEN LEAST(",
        next_scan_month_start!("scan_anchor"),
        ", current_period_end)
                 ELSE current_period_end END
         FROM subscriptions
         WHERE user_id = $1 AND status = 'active'
         ORDER BY updated_at DESC LIMIT 1"
    ))
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .unwrap_or_else(|e| {
        eprintln!("Failed to read paid scan usage: {}", e);
        None
    });

    if let Some((plan_name, interval, used, resets_on)) = paid {
        return json!({
            "plan": plan_name,
            "interval": interval,
            "used": used,
            "limit": scan_limit_for_plan(&plan_name),
            "resets_on": resets_on,
        });
    }

    let free: Option<(i32, DateTime<Utc>)> = sqlx::query_as(concat!(
        "SELECT
             CASE WHEN f.period_start >= ",
        scan_month_start!("u.created_at"),
        " THEN f.scans_used ELSE 0 END,
             ",
        next_scan_month_start!("u.created_at"),
        "
         FROM users u
         LEFT JOIN free_scan_usage f ON f.user_id = u.id
         WHERE u.id = $1"
    ))
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .unwrap_or_else(|e| {
        eprintln!("Failed to read free scan usage: {}", e);
        None
    });

    let (used, resets_on) = match free {
        Some((used, resets_on)) => (used, Some(resets_on.date_naive())),
        None => (0, None),
    };

    json!({
        "plan": "Free",
        "interval": null,
        "used": used,
        "limit": FREE_MONTHLY_SCANS,
        "resets_on": resets_on,
    })
}

#[cfg(test)]
mod plan_tests {
    use super::{
        BillingInterval::{Month, Year},
        PlanChange, classify_plan_change, interval_from_billing_period, normalize_plan_name,
        product_ids_key,
    };

    #[test]
    fn product_names_map_to_plans() {
        assert_eq!(normalize_plan_name("Team"), "Team");
        assert_eq!(normalize_plan_name("Team Yearly"), "Team");
        assert_eq!(
            normalize_plan_name("Safely Enterprise (annual)"),
            "Enterprise"
        );
        assert_eq!(normalize_plan_name("Something else"), "Something else");
    }

    #[test]
    fn billing_period_maps_to_interval() {
        assert_eq!(interval_from_billing_period(Some("every-year")), Year);
        assert_eq!(interval_from_billing_period(Some("every-month")), Month);
        assert_eq!(interval_from_billing_period(None), Month);
    }

    #[test]
    fn product_id_keys() {
        assert_eq!(product_ids_key("Team", Month), "Team");
        assert_eq!(product_ids_key("Enterprise", Year), "EnterpriseYearly");
    }

    #[test]
    fn plan_changes() {
        use PlanChange::*;
        assert_eq!(
            classify_plan_change("Team", Month, "Enterprise", Month),
            UpgradeNow
        );
        assert_eq!(
            classify_plan_change("Team", Year, "Enterprise", Year),
            UpgradeNow
        );
        assert_eq!(
            classify_plan_change("Enterprise", Month, "Team", Month),
            DowngradeAtRenewal
        );
        assert_eq!(
            classify_plan_change("Enterprise", Year, "Team", Year),
            DowngradeAtRenewal
        );
        assert_eq!(
            classify_plan_change("Team", Month, "Team", Year),
            NewYearlyCheckout
        );
        assert_eq!(
            classify_plan_change("Enterprise", Month, "Team", Year),
            NewYearlyCheckout
        );
        assert_eq!(
            classify_plan_change("Team", Year, "Team", Month),
            YearlyToMonthly
        );
        assert_eq!(
            classify_plan_change("Team", Year, "Team", Year),
            AlreadyOnPlan
        );
        assert_eq!(classify_plan_change("Gold", Month, "Team", Month), Invalid);
    }
}
