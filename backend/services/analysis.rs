use crate::{
    errors::analyze::AnalyzeError,
    models::{
        analysis::{Analysis, AnalyzeRequest, AnalyzeResponse, RiskLevel, Signal},
        helpers::format_account_age,
        listings::{Listings, ListingsRequest},
        sellers::{SellerVerification, Sellers, SellersRequest, SellersResponse},
    },
    services::{
        auth::extract_user_id,
        claude::{ClaudeAnalysis, call_claude},
        fraud_reports::{build_network_summary, count_fraud_reports},
        listings::get_monthly_visit_activity,
        sellers::{create_seller, find_seller},
        signals::{build_domain_signal, build_signals},
    },
};
use axum::{Json, http::HeaderMap};
use serde_json::{Value, to_value};
use sqlx::{Error, Pool, Postgres, query_as};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use uuid::Uuid;

pub struct CreateAnalysisData<'a> {
    pub pool: &'a Pool<Postgres>,
    pub listing_id: Uuid,
    pub risk_score: i16,
    pub risk_level: RiskLevel,
    pub signals: Value,
    pub network_summary: String,
    pub claude_raw: String,
    pub user_id: Uuid,
}

pub struct ResolvedSeller {
    pub seller: Sellers,
    pub fraud_count: i64,
    pub network_summary: String,
}

pub struct BuildResponseData<'a> {
    pub pool: &'a Pool<Postgres>,
    pub listing_id: Uuid,
    pub risk_score: i16,
    pub risk_level: RiskLevel,
    pub signals: Vec<Signal>,
    pub claude_analysis: ClaudeAnalysis,
    pub user_id: Uuid,
    pub seller: Sellers,
    pub fraud_count: i64,
    pub network_summary: String,
}

// This stops any one person from calling the /analyze endpoint more than 10 times within any 5-minute stretch.
// It sets up a way to track how many times each logged-in user has called the expensive /analyze endpoint recently.
pub static RATE_LIMITS: OnceLock<Mutex<HashMap<Uuid, (u32, Instant)>>> = OnceLock::new();
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(300);
const RATE_LIMIT_MAX_REQUESTS: u32 = 10;

/// Every time someone tries to use /analyze, this checks their notebook entry, lets them
/// through and counts it, unless they've already hit 10 within the last 5 minutes, in which
/// case it tells them exactly how many seconds until they can try again.
///
/// Gets the notebook (creating it, if this is the very first time), finds this person's
/// page in the notebook — or creates one, if they've never called before, checks
/// if their 5-minute window has already run out and counts the current request,
/// checks if they're still under the limit. If they've gone over and calculate
/// exactly how long they need to wait.
pub fn check_rate_limit(user_id: Uuid) -> Result<(), AnalyzeError> {
    let map = RATE_LIMITS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = map.lock().expect("expected to lock the map");
    let now = Instant::now();
    let entry = map.entry(user_id).or_insert((0, now));

    if now.duration_since(entry.1) > RATE_LIMIT_WINDOW {
        entry.0 = 0;
        entry.1 = now;
    }

    entry.0 += 1;

    if entry.0 <= RATE_LIMIT_MAX_REQUESTS {
        Ok(())
    } else {
        let elapsed = now.duration_since(entry.1);
        let remaining = RATE_LIMIT_WINDOW.saturating_sub(elapsed);
        Err(AnalyzeError::RateLimited(remaining.as_secs()))
    }
}

/// Confirms the caller is genuinely signed in, then checks they haven't
/// exceeded their request rate limit. Real analysis costs real Claude
/// API money per request, so this endpoint must actually reject an
/// anonymous or over-limit caller, not just proceed anyway.
///
/// It checks if this is a genuinely signed-in person, checks if they've
/// already hit their rate limit and if both checks passed, hand back their real user ID.
pub async fn authorize_request(
    headers: &HeaderMap,
    pool: &Pool<Postgres>,
) -> Result<Uuid, AnalyzeError> {
    let user_id = extract_user_id(headers, pool)
        .await
        .map_err(|_| AnalyzeError::Unauthorized)?
        .ok_or(AnalyzeError::Unauthorized)?;

    check_rate_limit(user_id)?;

    Ok(user_id)
}

/// It takes the one big request that arrives from your extension, and splits it into two separate,
/// smaller pieces. One containing just the seller's information, and one containing just
/// the listing's information.
///
/// It builds the seller-specific piece, builds the listing-specific piece
/// and returns both pieces together, as a pair.
pub fn build_requests(r: &AnalyzeRequest) -> (SellersRequest, ListingsRequest) {
    let seller_request = SellersRequest {
        platform: r.platform.clone(),
        platform_id: r.platform_id.clone(),
        name: r.seller_name.clone(),
        handle: r.seller_handle.clone(),
        phone: r.seller_phone.clone(),
        profile_url: r.seller_profile_url.clone(),
        join_date: r.seller_join_date.clone(),
        location: r.seller_location.clone(),
        last_active: r.seller_last_active.clone(),
    };

    let listing_request = ListingsRequest {
        seller_id: r.seller_id,
        platform: r.platform.clone(),
        listing_url: r.listing_url.clone(),
        listing_id: r.listing_id.clone(),
        title: r.title.clone(),
        price: r.price,
        description: r.description.clone(),
        category: r.category.clone(),
        image_urls: r.image_urls.clone(),
        posted_date: r.posted_date.clone(),
    };

    (seller_request, listing_request)
}

/// Before writing anything to the database, first check if this seller already has fraud
/// reports against them so their very first database record is already correct, never briefly,
/// incorrectly saying 'Unknown' about someone who's already known to be a problem.
///
/// It checks if this seller already exists in the database. If they already exist,
/// checks how many fraud reports they have — before doing anything else, decides their
/// verification status, based on that count, creates (or updates) the seller row,
/// using that correctly-determined verification, counts their fraud reports again,
/// this time for the real, final result.
pub async fn resolve_seller(
    pool: &Pool<Postgres>,
    seller_req: &SellersRequest,
    platform: &str,
    platform_id: &str,
) -> Result<ResolvedSeller, AnalyzeError> {
    let existing_seller = find_seller(pool, platform, platform_id)
        .await
        .map_err(|e| AnalyzeError::Database(e.to_string()))?;

    let preliminary_fraud_count = if let Some(ref s) = existing_seller {
        count_fraud_reports(pool, s.id)
            .await
            .map_err(|e| AnalyzeError::Database(e.to_string()))?
    } else {
        0
    };

    let verification = if preliminary_fraud_count > 0 {
        SellerVerification::Reported
    } else {
        SellerVerification::Unknown
    };

    let seller = create_seller(pool, seller_req, verification)
        .await
        .map_err(|e| AnalyzeError::Database(e.to_string()))?;

    let fraud_count = count_fraud_reports(pool, seller.id)
        .await
        .map_err(|e| AnalyzeError::Database(e.to_string()))?;

    let network_summary = build_network_summary(fraud_count);

    Ok(ResolvedSeller {
        seller,
        fraud_count,
        network_summary,
    })
}

/// It sends the listing's real details to Claude, asking it to analyze
/// whether this looks like a genuine or fraudulent listing by filling in
/// sensible defaults for anything that's missing, so Claude always gets
/// something usable, even if the original listing had gaps.
///
/// It calculates the seller's account age, from their real join date, gets
/// the listing's images, or an empty list if there are none, actually calls
/// Claude, with everything it needs and it waits for Claude's response,
/// and handles failure clearly
pub async fn run_claude_analysis(
    listing: &Listings,
    seller: &Sellers,
) -> Result<ClaudeAnalysis, AnalyzeError> {
    let account_age = seller
        .join_date
        .map(format_account_age)
        .unwrap_or_else(|| "Unknown".to_string());

    let image_urls = listing.image_urls.as_deref().unwrap_or(&[]);

    call_claude(
        &listing.platform,
        seller.name.as_deref().unwrap_or("Unknown"),
        &account_age,
        listing.title.as_deref().unwrap_or("Untitled"),
        listing.price.unwrap_or(0),
        listing.description.as_deref().unwrap_or("No Description"),
        image_urls,
    )
    .await
    .map_err(|e| AnalyzeError::ClaudeAnalysisFailed(e.to_string()))
}

/// It builds the complete list of warning signals shown on the dashboard
/// starting with everything Claude's analysis found, then adding a domain-mismatch
/// check at the very top, if one was detected.
///
/// It builds the main signal list from Claude's analysis, checks if a domain
/// mismatch was detected and returns the complete list.
pub fn build_all_signals(
    claude_analysis: &ClaudeAnalysis,
    seller: &Sellers,
    request: &AnalyzeRequest,
) -> Vec<Signal> {
    let mut signals = build_signals(claude_analysis, seller);

    if let Some(domain_signal) = build_domain_signal(
        request.domain_check_status.as_deref(),
        request.domain_check_real_name.as_deref(),
        request.domain_check_real_domain.as_deref(),
        request.domain_check_current_domain.as_deref(),
        request.domain_check_current_html.as_deref(),
        request.domain_check_real_html.as_deref(),
    ) {
        signals.insert(0, domain_signal);
    }

    signals
}

/// It saves the complete analysis result to the database, fetches the seller's
/// real visit-history chart, and packages everything together into the final
/// response the extension actually receives.
///
/// It converts the signals list into a format the database can store, actually
/// saves the analysis to the database, fetches the seller's real visit history,
/// for the chart, builds the seller portion of the response, assembles and returns
/// the complete, final response.
pub async fn save_and_build_response(
    data: BuildResponseData<'_>,
) -> Result<Json<AnalyzeResponse>, AnalyzeError> {
    let signals_json =
        to_value(&data.signals).map_err(|e| AnalyzeError::SerializationFailed(e.to_string()))?;

    let saved_analysis = create_analysis(CreateAnalysisData {
        pool: data.pool,
        listing_id: data.listing_id,
        risk_score: data.risk_score,
        risk_level: data.risk_level,
        signals: signals_json,
        network_summary: data.claude_analysis.overall_risk_notes.clone(),
        claude_raw: String::new(),
        user_id: data.user_id,
    })
    .await
    .map_err(|e| AnalyzeError::Database(e.to_string()))?;

    let monthly_activity = get_monthly_visit_activity(data.pool, data.seller.id)
        .await
        .unwrap_or_else(|_| vec![0i32; 12]);

    let mut seller_response = SellersResponse::from(data.seller);
    seller_response.network_summary = data.network_summary;
    seller_response.monthly_activity = monthly_activity;

    Ok(Json(AnalyzeResponse {
        risk_score: saved_analysis.risk_score,
        risk_level: saved_analysis.risk_level,
        seller: seller_response,
        signals: data.signals,
        network_summary: data.claude_analysis.overall_risk_notes,
        fraud_report_count: data.fraud_count,
    }))
}

// This only ever fails for one genuine reason - a real database
// problem (e.g. a bad foreign key on listing_id/user_id) - so plain
// sqlx::Error is honest here; a custom error type isn't needed.
pub async fn create_analysis(data: CreateAnalysisData<'_>) -> Result<Analysis, Error> {
    let id = Uuid::now_v7();
    let analysis = query_as::<_, Analysis>(
        "
        INSERT INTO analysis (
            id,
            listing_id,
            risk_score,
            risk_level,
            signals,
            network_summary,
            claude_raw,
            user_id,
            created_at
        )
        VALUES (
            $1,  $2,  $3,  $4,   $5,
            $6,  $7,  $8,  NOW()
        )
        RETURNING *
        ",
    )
    .bind(id)
    .bind(&data.listing_id)
    .bind(&data.risk_score)
    .bind(&data.risk_level)
    .bind(&data.signals)
    .bind(&data.network_summary)
    .bind(&data.claude_raw)
    .bind(&data.user_id)
    .fetch_one(data.pool)
    .await?;
    Ok(analysis)
}
