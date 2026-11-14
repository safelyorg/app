//! Watch suppliers: Safely checks a supplier again by itself and emails
//! when something changes.
//!
//! - Team: up to 25 suppliers, checked every week. Enterprise (and the
//!   unlimited list): no limit, checked every day. Free: not available.
//! - A team shares one list (the owner's account, like History).
//! - The marketplace page itself is not scanned again (marketplaces block
//!   automatic visits). What is checked again:
//!     - new scam reports from Safely users;
//!     - the outside records (sanctions lists, official registers...),
//!       with the facts saved from the scan (seller_facts table);
//!     - the supplier's own website: still answering, and not
//!       registered again by someone else;
//!     - the listing page: only "gone" when the site itself says the
//!       page does not exist (404 / 410). A site that blocks the check
//!       gives "Not checked", never "gone".
//! - The first check only notes how things are. Later checks compare
//!   with the last one and email only when something got worse.
//! - The checks run in the background (start_watch_scheduler, main.rs)
//!   and do not use the plan's scans.

use crate::models::analysis::Signal;
use crate::services::features::billing::is_unlimited_user;
use crate::services::features::domain_age::{lookup, normalize_domain};
use crate::services::features::email::send_watch_alert_email;
use crate::services::features::osint::web_scam_pages;
use crate::services::features::team::plan_holder;
use crate::services::features::team_history::scan_owner_for_viewer;
use crate::services::shared::standards::database_card_enabled;
use crate::services::sources::databases::{
    OUTSIDE_RECORDS, SellerFacts, check_outside_records,
    numbers::{REGISTER_CLOSED, REGISTER_LABEL},
    sanctions,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, Pool, Postgres, query, query_as, query_scalar};
use std::collections::BTreeMap;
use std::time::Duration;
use uuid::Uuid;

/// How many suppliers a Team plan can watch.
pub const TEAM_WATCH_LIMIT: i64 = 25;

/// How the supplier looked at the last check.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WatchState {
    /// Outside-records cards: label -> (type, value, text).
    pub cards: BTreeMap<String, (String, String, String)>,
    /// Scam reports from Safely users.
    pub fraud_reports: i64,
    /// The supplier's own website ("supplier.com"), when known.
    pub website: Option<String>,
    /// Some(true) answering, Some(false) down (two checks in a row).
    pub website_up: Option<bool>,
    /// Checks in a row the website did not answer.
    pub website_down_checks: u32,
    /// When the website's domain was registered (from the registries).
    pub domain_registered_on: Option<String>,
    /// The listing page: "online" or "gone" (last known answer).
    pub listing: Option<String>,
    /// False when the listing site blocked this check.
    pub listing_checked: bool,
    /// Pages found online with the company's name next to scam words
    /// (the scan's own web search): (address, title).
    pub web_pages: Vec<(String, String)>,
    /// False until the web search has worked once (no key, or it
    /// failed): then nothing found is never read as "no warnings".
    pub web_checked: bool,
}

/// One thing that got worse since the last check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatchChange {
    /// "scam_report", "sanctions", "company_closed", "record",
    /// "website_down", "website_owner" or "listing_gone".
    pub kind: String,
    pub title: String,
    pub detail: String,
}

fn change(kind: &str, title: &str, detail: String) -> WatchChange {
    WatchChange {
        kind: kind.to_string(),
        title: title.to_string(),
        detail,
    }
}

fn is_flagged(signal_type: &str) -> bool {
    signal_type != "good" && signal_type != "info"
}

/// What got worse between two checks.
pub fn compare(old: &WatchState, new: &WatchState) -> Vec<WatchChange> {
    let mut changes = Vec::new();

    let new_reports = new.fraud_reports - old.fraud_reports;
    if new_reports > 0 {
        let title = if new_reports == 1 {
            "New scam report".to_string()
        } else {
            format!("{new_reports} new scam reports")
        };
        changes.push(change(
            "scam_report",
            &title,
            "Safely users reported this supplier since the last check.".to_string(),
        ));
    }

    for (label, (kind, value, text)) in &new.cards {
        if !is_flagged(kind) {
            continue;
        }
        let was_same = old
            .cards
            .get(label)
            .is_some_and(|(k, v, _)| is_flagged(k) && v == value);
        if was_same {
            continue;
        }
        let item = if label == sanctions::LABEL {
            change("sanctions", "On a sanctions list", text.clone())
        } else if label == REGISTER_LABEL && value == REGISTER_CLOSED {
            change("company_closed", "Company closed", text.clone())
        } else {
            change("record", &format!("{label}: {value}"), text.clone())
        };
        changes.push(item);
    }

    if let Some(site) = &new.website {
        if old.website_up == Some(true) && new.website_up == Some(false) {
            changes.push(change(
                "website_down",
                "Website down",
                format!("{site} did not answer in the last two checks."),
            ));
        }
        if let (Some(before), Some(now)) = (&old.domain_registered_on, &new.domain_registered_on)
            && before != now
        {
            changes.push(change(
                "website_owner",
                "Website registered again",
                format!(
                    "{site} was registered again on {now} (before: {before}). It may now belong to someone else."
                ),
            ));
        }
    }

    // New scam pages online: only when the last search worked too, so
    // the first working search never sends every old page as "new".
    if old.web_checked && new.web_checked {
        let fresh: Vec<&(String, String)> = new
            .web_pages
            .iter()
            .filter(|(url, _)| !old.web_pages.iter().any(|(u, _)| u == url))
            .collect();
        if let Some((url, title)) = fresh.first() {
            let more = if fresh.len() > 1 {
                format!(" (and {} more)", fresh.len() - 1)
            } else {
                String::new()
            };
            changes.push(change(
                "web_warning",
                "New scam warning online",
                format!("{title} - {url}{more}"),
            ));
        }
    }

    if old.listing.as_deref() == Some("online") && new.listing.as_deref() == Some("gone") {
        changes.push(change(
            "listing_gone",
            "Listing removed",
            "The marketplace now says this listing page does not exist.".to_string(),
        ));
    }
    changes
}

/// Saves what a scan knew about the seller (name, country, website,
/// company numbers), so a watch can run the same outside checks later.
/// Called by the scan (analysis.rs, websites/mod.rs). A failed save only
/// means a later watch has fewer facts.
pub async fn remember_seller_facts(pool: &Pool<Postgres>, seller_id: Uuid, facts: &SellerFacts) {
    let Ok(value) = serde_json::to_value(facts) else {
        return;
    };
    if let Err(e) = query(
        "INSERT INTO seller_facts (seller_id, facts, updated_at) VALUES ($1, $2, NOW())
         ON CONFLICT (seller_id) DO UPDATE SET facts = EXCLUDED.facts, updated_at = NOW()",
    )
    .bind(seller_id)
    .bind(value)
    .execute(pool)
    .await
    {
        eprintln!("Safely: could not save the seller's facts: {e}");
    }
}

/// The facts saved from the scan, or the name and place from the seller
/// row when none were saved.
async fn facts_for(pool: &Pool<Postgres>, seller_id: Uuid) -> SellerFacts {
    let saved: Option<Value> = query_scalar("SELECT facts FROM seller_facts WHERE seller_id = $1")
        .bind(seller_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    if let Some(facts) = saved.and_then(|v| serde_json::from_value(v).ok()) {
        return facts;
    }
    let row: Option<(Option<String>, Option<String>)> =
        query_as("SELECT name, location FROM sellers WHERE id = $1")
            .bind(seller_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    let (company_name, country) = row.unwrap_or_default();
    SellerFacts {
        company_name,
        country,
        ..SellerFacts::default()
    }
}

fn http_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36",
        )
        .build()
        .ok()
}

/// The listing page: Some("online"), Some("gone") (the site says it does
/// not exist), or None (the site blocked the check or did not answer).
pub fn listing_answer(status: Option<u16>) -> Option<&'static str> {
    match status {
        Some(404) | Some(410) => Some("gone"),
        Some(s) if (200..300).contains(&s) => Some("online"),
        _ => None,
    }
}

async fn check_listing(client: &reqwest::Client, url: &str) -> Option<&'static str> {
    let status = client
        .get(url)
        .send()
        .await
        .ok()
        .map(|r| r.status().as_u16());
    listing_answer(status)
}

/// True when the website answers (any answer below 500 counts).
async fn website_answers(client: &reqwest::Client, domain: &str) -> bool {
    match client.get(format!("https://{domain}")).send().await {
        Ok(r) => r.status().as_u16() < 500,
        Err(_) => client
            .get(format!("http://{domain}"))
            .send()
            .await
            .is_ok_and(|r| r.status().as_u16() < 500),
    }
}

/// The outside-records cards that are switched on, by label.
fn cards_by_label(cards: Vec<Signal>) -> BTreeMap<String, (String, String, String)> {
    cards
        .into_iter()
        .filter(|c| c.category == OUTSIDE_RECORDS && database_card_enabled(&c.label))
        .map(|c| (c.label, (c.signal_type, c.value, c.sub)))
        .collect()
}

/// Checks the supplier now. `old` keeps what this check could not see
/// (a source that did not answer keeps its last answer).
pub async fn current_state(
    pool: &Pool<Postgres>,
    seller_id: Uuid,
    listing_url: Option<&str>,
    old: Option<&WatchState>,
) -> WatchState {
    let facts = facts_for(pool, seller_id).await;
    let website = facts.website.as_deref().and_then(normalize_domain);
    let mut state = WatchState {
        website: website.clone(),
        ..WatchState::default()
    };

    // Outside records: a source that gave no card this time keeps its
    // last card (it could not finish; that is not a change).
    let mut cards = old.map(|o| o.cards.clone()).unwrap_or_default();
    cards.extend(cards_by_label(check_outside_records(&facts).await));
    state.cards = cards;

    state.fraud_reports = query_scalar("SELECT COUNT(*) FROM fraud_reports WHERE seller_id = $1")
        .bind(seller_id)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|_| old.map_or(0, |o| o.fraud_reports));

    let client = http_client();
    if let (Some(client), Some(site)) = (client.as_ref(), website.as_deref()) {
        if website_answers(client, site).await {
            state.website_up = Some(true);
            state.website_down_checks = 0;
        } else {
            state.website_down_checks = old.map_or(0, |o| o.website_down_checks) + 1;
            state.website_up = if state.website_down_checks >= 2 {
                Some(false)
            } else {
                old.and_then(|o| o.website_up)
            };
        }
        state.domain_registered_on = match lookup(site).await {
            Ok(age) => age
                .registered_on
                .or_else(|| old.and_then(|o| o.domain_registered_on.clone())),
            Err(_) => old.and_then(|o| o.domain_registered_on.clone()),
        };
    }

    let answer = match (client.as_ref(), listing_url) {
        (Some(client), Some(url)) if url.starts_with("http") => check_listing(client, url).await,
        _ => None,
    };
    state.listing_checked = answer.is_some();
    state.listing = answer
        .map(str::to_string)
        .or_else(|| old.and_then(|o| o.listing.clone()));

    // The scan's web search for scam warnings, again (osint.rs): the
    // company's name next to scam words, and on complaint sites.
    state.web_pages = old.map(|o| o.web_pages.clone()).unwrap_or_default();
    state.web_checked = old.is_some_and(|o| o.web_checked);
    if let Some(name) = facts
        .company_name
        .as_deref()
        .filter(|n| !n.trim().is_empty())
    {
        let platform: String = query_scalar("SELECT platform FROM sellers WHERE id = $1")
            .bind(seller_id)
            .fetch_one(pool)
            .await
            .unwrap_or_default();
        if let Some(pages) = web_scam_pages(name, &platform, facts.country.as_deref()).await {
            state.web_pages = pages.into_iter().map(|p| (p.url, p.title)).collect();
            state.web_checked = true;
        }
    }
    state
}

/// How often this team's suppliers are checked, or None when the plan is
/// not paid (watching is paused).
async fn check_every(pool: &Pool<Postgres>, owner_id: Uuid) -> Option<chrono::Duration> {
    if is_unlimited_user(pool, owner_id).await {
        return Some(chrono::Duration::days(1));
    }
    let plan: Option<String> = query_scalar(
        "SELECT plan_name FROM subscriptions WHERE user_id = $1 AND status = 'active'
         ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(owner_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let plan = plan?;
    if plan.to_lowercase().contains("enterprise") {
        Some(chrono::Duration::days(1))
    } else {
        Some(chrono::Duration::days(7))
    }
}

/// How many suppliers this team may watch (None = no limit). Err when
/// the plan is not paid.
async fn watch_limit(pool: &Pool<Postgres>, owner_id: Uuid) -> Result<Option<i64>, WatchError> {
    let every = check_every(pool, owner_id)
        .await
        .ok_or(WatchError::NeedsPaidPlan)?;
    Ok((every > chrono::Duration::days(1)).then_some(TEAM_WATCH_LIMIT))
}

#[derive(Debug)]
pub enum WatchError {
    NeedsPaidPlan,
    LimitReached(i64),
    NotFound,
    Database(String),
}

impl From<sqlx::Error> for WatchError {
    fn from(e: sqlx::Error) -> Self {
        WatchError::Database(e.to_string())
    }
}

impl WatchError {
    /// The short code the dashboard reads.
    pub fn code(&self) -> &'static str {
        match self {
            WatchError::NeedsPaidPlan => "needs_paid_plan",
            WatchError::LimitReached(_) => "watch_limit_reached",
            WatchError::NotFound => "not_found",
            WatchError::Database(_) => "internal_error",
        }
    }
}

/// Starts watching the supplier of a scan the person can open (their own
/// or a teammate's). Watching it again changes nothing.
pub async fn add_watch(
    pool: &Pool<Postgres>,
    viewer: Uuid,
    analysis_id: Uuid,
) -> Result<(), WatchError> {
    scan_owner_for_viewer(pool, analysis_id, viewer)
        .await?
        .ok_or(WatchError::NotFound)?;
    let owner_id = plan_holder(pool, viewer).await;
    let limit = watch_limit(pool, owner_id).await?;

    let (seller_id, listing_url): (Uuid, String) = query_as(
        "SELECT l.seller_id, l.listing_url FROM analysis a
         JOIN listings l ON l.id = a.listing_id
         WHERE a.id = $1 AND l.seller_id IS NOT NULL",
    )
    .bind(analysis_id)
    .fetch_optional(pool)
    .await?
    .ok_or(WatchError::NotFound)?;

    let already: bool = query_scalar(
        "SELECT EXISTS (SELECT 1 FROM watched_suppliers WHERE team_owner_id = $1 AND seller_id = $2)",
    )
    .bind(owner_id)
    .bind(seller_id)
    .fetch_one(pool)
    .await?;
    if already {
        return Ok(());
    }
    if let Some(limit) = limit {
        let used: i64 =
            query_scalar("SELECT COUNT(*) FROM watched_suppliers WHERE team_owner_id = $1")
                .bind(owner_id)
                .fetch_one(pool)
                .await?;
        if used >= limit {
            return Err(WatchError::LimitReached(limit));
        }
    }
    query(
        "INSERT INTO watched_suppliers (team_owner_id, seller_id, analysis_id, listing_url, added_by)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (team_owner_id, seller_id) DO NOTHING",
    )
    .bind(owner_id)
    .bind(seller_id)
    .bind(analysis_id)
    .bind(listing_url)
    .bind(viewer)
    .execute(pool)
    .await?;
    Ok(())
}

/// Stops watching a supplier (anyone in the team can).
pub async fn stop_watch(
    pool: &Pool<Postgres>,
    viewer: Uuid,
    seller_id: Uuid,
) -> Result<(), WatchError> {
    let owner_id = plan_holder(pool, viewer).await;
    let done = query("DELETE FROM watched_suppliers WHERE team_owner_id = $1 AND seller_id = $2")
        .bind(owner_id)
        .bind(seller_id)
        .execute(pool)
        .await?;
    if done.rows_affected() == 0 {
        return Err(WatchError::NotFound);
    }
    Ok(())
}

/// True when the supplier of this scan is on the person's watch list.
pub async fn is_watched(pool: &Pool<Postgres>, viewer: Uuid, analysis_id: Uuid) -> bool {
    let owner_id = plan_holder(pool, viewer).await;
    query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM watched_suppliers w
            JOIN listings l ON l.seller_id = w.seller_id
            JOIN analysis a ON a.listing_id = l.id
            WHERE w.team_owner_id = $1 AND a.id = $2)",
    )
    .bind(owner_id)
    .bind(analysis_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

#[derive(Debug, FromRow)]
struct WatchRow {
    seller_id: Uuid,
    analysis_id: Option<Uuid>,
    name: Option<String>,
    platform: String,
    status: String,
    changes: Value,
    state: Option<Value>,
    created_at: DateTime<Utc>,
    last_checked_at: Option<DateTime<Utc>>,
    next_check_at: DateTime<Utc>,
    added_by: Option<Uuid>,
    added_by_email: Option<String>,
    added_by_name: Option<String>,
}

/// The "Watched suppliers" page: the team's list, how many are used out
/// of the limit, and when the next check is.
pub async fn watch_list(pool: &Pool<Postgres>, viewer: Uuid) -> Result<Value, WatchError> {
    let owner_id = plan_holder(pool, viewer).await;
    let every = check_every(pool, owner_id).await;
    let rows: Vec<WatchRow> = query_as(
        // Open = the newest scan of this supplier in the team (the same
        // one History shows), or the scan it was watched from.
        "SELECT w.seller_id,
                COALESCE((
                    SELECT a.id FROM analysis a
                    JOIN listings l ON l.id = a.listing_id
                    WHERE l.seller_id = w.seller_id
                      AND (a.team_owner_id = $1
                           OR (a.team_owner_id IS NULL AND a.user_id = $1)
                           OR a.user_id = $2)
                    ORDER BY a.created_at DESC LIMIT 1
                ), w.analysis_id) AS analysis_id,
                s.name, s.platform, w.status, w.changes, w.state,
                w.created_at, w.last_checked_at, w.next_check_at,
                w.added_by, lower(u.email) AS added_by_email, u.name AS added_by_name
         FROM watched_suppliers w
         JOIN sellers s ON s.id = w.seller_id
         LEFT JOIN users u ON u.id = w.added_by
         WHERE w.team_owner_id = $1
         ORDER BY (w.status = 'changed') DESC, w.created_at DESC",
    )
    .bind(owner_id)
    .bind(viewer)
    .fetch_all(pool)
    .await?;
    let next_check = rows.iter().map(|r| r.next_check_at).min();
    let items: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let state: Option<WatchState> = r.state.and_then(|s| serde_json::from_value(s).ok());
            json!({
                "seller_id": r.seller_id,
                "analysis_id": r.analysis_id,
                "name": r.name,
                "platform_name": crate::services::features::signals::platform_display_name(&r.platform),
                "platform": r.platform,
                "status": r.status,
                "changes": r.changes,
                "listing_checked": state.as_ref().is_none_or(|s| s.listing_checked),
                "website": state.as_ref().and_then(|s| s.website.clone()),
                "watched_since": r.created_at.format("%Y-%m-%d").to_string(),
                // Who added it ("Added by ali@company.com" / "Added by you").
                "added_by": r.added_by_email,
                "added_by_name": r.added_by_name.filter(|n| !n.trim().is_empty()),
                "added_by_you": r.added_by == Some(viewer),
                "last_checked": r.last_checked_at.map(|d| d.format("%Y-%m-%d").to_string()),
            })
        })
        .collect();
    let limit = match every {
        None => Some(0),
        Some(e) if e > chrono::Duration::days(1) => Some(TEAM_WATCH_LIMIT),
        Some(_) => None,
    };
    Ok(json!({
        "paid": every.is_some(),
        "limit": limit,
        "used": items.len(),
        "every_days": every.map(|e| e.num_days()),
        "next_check": next_check.map(|d| d.format("%Y-%m-%d").to_string()),
        "items": items,
    }))
}

#[derive(Debug, FromRow)]
struct DueRow {
    id: Uuid,
    team_owner_id: Uuid,
    seller_id: Uuid,
    analysis_id: Option<Uuid>,
    listing_url: Option<String>,
    added_by: Option<Uuid>,
    state: Option<Value>,
}

/// Checks one watched supplier and saves the result. Emails when
/// something got worse. Returns the changes.
async fn check_due(pool: &Pool<Postgres>, row: DueRow) -> Vec<WatchChange> {
    let Some(every) = check_every(pool, row.team_owner_id).await else {
        // Not on a paid plan any more: paused, looked at again tomorrow.
        let _ = query(
            "UPDATE watched_suppliers SET status = 'paused', next_check_at = NOW() + INTERVAL '1 day' WHERE id = $1",
        )
        .bind(row.id)
        .execute(pool)
        .await;
        return Vec::new();
    };
    let old: Option<WatchState> = row
        .state
        .clone()
        .and_then(|s| serde_json::from_value(s).ok());
    let new = current_state(
        pool,
        row.seller_id,
        row.listing_url.as_deref(),
        old.as_ref(),
    )
    .await;
    // The first check only notes how things are.
    let changes = old.as_ref().map(|o| compare(o, &new)).unwrap_or_default();
    let status = if !changes.is_empty() {
        "changed"
    } else if !new.listing_checked && row.listing_url.is_some() {
        "listing_not_checked"
    } else {
        "no_change"
    };
    let next = Utc::now() + every;
    let saved = if changes.is_empty() {
        query(
            "UPDATE watched_suppliers SET state = $2, status = $3, last_checked_at = NOW(), next_check_at = $4
             WHERE id = $1",
        )
        .bind(row.id)
        .bind(serde_json::to_value(&new).unwrap_or(Value::Null))
        .bind(status)
        .bind(next)
        .execute(pool)
        .await
    } else {
        query(
            "UPDATE watched_suppliers SET state = $2, status = $3, last_checked_at = NOW(), next_check_at = $4,
                    changes = $5, changed_at = NOW()
             WHERE id = $1",
        )
        .bind(row.id)
        .bind(serde_json::to_value(&new).unwrap_or(Value::Null))
        .bind(status)
        .bind(next)
        .bind(serde_json::to_value(&changes).unwrap_or(Value::Null))
        .execute(pool)
        .await
    };
    if let Err(e) = saved {
        eprintln!("Safely: could not save a supplier watch check: {e}");
    }
    if !changes.is_empty() {
        email_changes(pool, &row, &changes).await;
    }
    changes
}

/// Emails the person who added the supplier and the team owner.
async fn email_changes(pool: &Pool<Postgres>, row: &DueRow, changes: &[WatchChange]) {
    let supplier: Option<(Option<String>, String)> =
        query_as("SELECT name, platform FROM sellers WHERE id = $1")
            .bind(row.seller_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    let (name, platform) = supplier.unwrap_or((None, String::new()));
    let name = name.unwrap_or_else(|| "A supplier you watch".to_string());
    let mut people = vec![row.team_owner_id];
    if let Some(added_by) = row.added_by
        && added_by != row.team_owner_id
    {
        people.push(added_by);
    }
    let emails: Vec<String> = query_scalar("SELECT email FROM users WHERE id = ANY($1)")
        .bind(&people)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    // The email's button opens the newest scan of this supplier in the team.
    let newest: Option<Uuid> = query_scalar(
        "SELECT a.id FROM analysis a
         JOIN listings l ON l.id = a.listing_id
         WHERE l.seller_id = $1
           AND (a.team_owner_id = $2 OR (a.team_owner_id IS NULL AND a.user_id = $2))
         ORDER BY a.created_at DESC LIMIT 1",
    )
    .bind(row.seller_id)
    .bind(row.team_owner_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let open = newest.or(row.analysis_id);
    for email in emails {
        if let Err(e) = send_watch_alert_email(&email, &name, &platform, open, changes).await {
            eprintln!("Safely: could not send a supplier watch email: {e:?}");
        }
    }
}

/// Checks the suppliers whose check is due (at most `max`). Each one is
/// claimed first (its next check moved 1 hour on), so two servers never
/// check the same supplier at once.
pub async fn run_due_checks(pool: &Pool<Postgres>, max: i64) -> usize {
    let rows: Vec<DueRow> = match query_as(
        "UPDATE watched_suppliers SET next_check_at = NOW() + INTERVAL '1 hour'
         WHERE id IN (
            SELECT id FROM watched_suppliers WHERE next_check_at <= NOW()
            ORDER BY next_check_at LIMIT $1 FOR UPDATE SKIP LOCKED)
         RETURNING id, team_owner_id, seller_id, analysis_id, listing_url, added_by, state",
    )
    .bind(max)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("Safely: could not load the supplier watch checks: {e}");
            return 0;
        }
    };
    let count = rows.len();
    for row in rows {
        check_due(pool, row).await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    count
}

/// Runs the due checks every 10 minutes, in the background. Off when
/// SAFELY_WATCH_DISABLED is "true" (for example on a second server).
pub fn start_watch_scheduler(pool: Pool<Postgres>) {
    if std::env::var("SAFELY_WATCH_DISABLED").is_ok_and(|v| v.trim() == "true") {
        println!("Safely: supplier watch checks are off (SAFELY_WATCH_DISABLED)");
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        loop {
            run_due_checks(&pool, 20).await;
            tokio::time::sleep(Duration::from_secs(600)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(t: &str, v: &str) -> (String, String, String) {
        (t.to_string(), v.to_string(), format!("{v} text"))
    }

    #[test]
    fn the_alert_email_lists_what_changed() {
        let mut context = tera::Context::new();
        context.insert("supplier", "Bernal <Inc>");
        context.insert("platform", "ThomasNet");
        context.insert(
            "changes",
            &vec![change(
                "scam_report",
                "New scam report",
                "Reported.".to_string(),
            )],
        );
        context.insert("open_url", "https://safely.sh/dashboard/thomasnet/1");
        context.insert("watch_url", "https://safely.sh/watch");
        context.insert("logo_url", "https://safely.sh/images/white_logo.png");
        let html = crate::services::features::templates::get_tera()
            .render("watch_alert_email.html", &context)
            .unwrap();
        assert!(html.contains("New scam report"));
        assert!(html.contains("Bernal &lt;Inc&gt;"));
        assert!(html.contains("https://safely.sh/watch"));
    }

    #[test]
    fn nothing_new_is_no_change() {
        let mut s = WatchState::default();
        s.cards
            .insert(sanctions::LABEL.to_string(), card("info", "Not found"));
        s.fraud_reports = 1;
        assert!(compare(&s, &s).is_empty());
    }

    #[test]
    fn a_new_report_a_sanctions_hit_and_a_closed_company() {
        let mut old = WatchState::default();
        old.cards
            .insert(sanctions::LABEL.to_string(), card("info", "Not found"));
        old.cards
            .insert(REGISTER_LABEL.to_string(), card("good", "Active"));
        let mut new = old.clone();
        new.fraud_reports = 2;
        new.cards
            .insert(sanctions::LABEL.to_string(), card("bad", "Match found"));
        new.cards
            .insert(REGISTER_LABEL.to_string(), card("caution", REGISTER_CLOSED));
        let kinds: Vec<String> = compare(&old, &new).into_iter().map(|c| c.kind).collect();
        assert_eq!(kinds, vec!["scam_report", "company_closed", "sanctions"]);
        assert_eq!(compare(&old, &new)[0].title, "2 new scam reports");
    }

    #[test]
    fn a_warning_that_was_already_there_is_not_sent_again() {
        let mut old = WatchState::default();
        old.cards
            .insert(REGISTER_LABEL.to_string(), card("caution", REGISTER_CLOSED));
        let new = old.clone();
        assert!(compare(&old, &new).is_empty());
    }

    #[test]
    fn website_down_and_registered_again() {
        let old = WatchState {
            website: Some("supplier.com".into()),
            website_up: Some(true),
            domain_registered_on: Some("2015-01-01".into()),
            ..WatchState::default()
        };
        let new = WatchState {
            website_up: Some(false),
            website_down_checks: 2,
            domain_registered_on: Some("2026-11-01".into()),
            ..old.clone()
        };
        let kinds: Vec<String> = compare(&old, &new).into_iter().map(|c| c.kind).collect();
        assert_eq!(kinds, vec!["website_down", "website_owner"]);
    }

    #[test]
    fn a_new_scam_page_online() {
        let old = WatchState {
            web_checked: true,
            web_pages: vec![("https://a.com/1".into(), "Old page".into())],
            ..WatchState::default()
        };
        let mut new = old.clone();
        new.web_pages
            .push(("https://b.com/2".into(), "Bernal took my money".into()));
        let found = compare(&old, &new);
        assert_eq!(found[0].kind, "web_warning");
        assert!(found[0].detail.contains("https://b.com/2"));
        // The same pages again: nothing new.
        assert!(compare(&new, &new).is_empty());
        // The search had never worked before: the first pages are not "new".
        let never = WatchState::default();
        assert!(compare(&never, &new).is_empty());
    }

    #[test]
    fn listing_gone_only_when_the_site_says_so() {
        assert_eq!(listing_answer(Some(404)), Some("gone"));
        assert_eq!(listing_answer(Some(410)), Some("gone"));
        assert_eq!(listing_answer(Some(200)), Some("online"));
        // Blocked or no answer: not checked, never "gone".
        assert_eq!(listing_answer(Some(403)), None);
        assert_eq!(listing_answer(Some(429)), None);
        assert_eq!(listing_answer(None), None);
        let old = WatchState {
            listing: Some("online".into()),
            ..WatchState::default()
        };
        let new = WatchState {
            listing: Some("gone".into()),
            listing_checked: true,
            ..WatchState::default()
        };
        assert_eq!(compare(&old, &new)[0].kind, "listing_gone");
    }
}
