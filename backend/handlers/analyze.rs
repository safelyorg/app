use crate::{
    errors::analyze::AnalyzeError,
    models::{
        analysis::{AnalyzeRequest, AnalyzeResponse, RiskLevel, Signal},
        helpers::format_account_age,
        listings::{Listings, ListingsRequest},
        sellers::{SellerVerification, Sellers, SellersRequest, SellersResponse},
    },
    services::{
        analysis::create_analysis,
        auth::extract_user_id,
        claude::{ClaudeAnalysis, call_claude},
        fraud_reports::{build_network_summary, count_fraud_reports},
        listings::{create_listing, get_monthly_visit_activity},
        scoring::calculate_risk_score,
        sellers::{create_seller, find_seller},
        signals::{build_domain_signal, build_signals},
    },
};
use axum::{Json, extract::State, http::HeaderMap};
use sqlx::{Pool, Postgres};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use uuid::Uuid;

pub struct ResolvedSeller {
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
        listing.description.as_deref().unwrap_or(""),
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
fn build_all_signals(
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
async fn save_and_build_response(
    pool: &Pool<Postgres>,
    listing_id: Uuid,
    risk_score: i16,
    risk_level: RiskLevel,
    signals: Vec<Signal>,
    claude_analysis: ClaudeAnalysis,
    user_id: Uuid,
    seller: Sellers,
    fraud_count: i64,
    network_summary: String,
) -> Result<Json<AnalyzeResponse>, AnalyzeError> {
    let signals_json = serde_json::to_value(&signals)
        .map_err(|e| AnalyzeError::SerializationFailed(e.to_string()))?;

    let saved_analysis = create_analysis(
        pool,
        listing_id,
        risk_score,
        risk_level,
        signals_json,
        claude_analysis.overall_risk_notes.clone(),
        String::new(),
        user_id,
    )
    .await
    .map_err(|e| AnalyzeError::Database(e.to_string()))?;

    let monthly_activity = get_monthly_visit_activity(pool, seller.id)
        .await
        .unwrap_or_else(|_| vec![0i32; 12]);

    let mut seller_response = SellersResponse::from(seller);
    seller_response.network_summary = network_summary;
    seller_response.monthly_activity = monthly_activity;

    Ok(Json(AnalyzeResponse {
        risk_score: saved_analysis.risk_score,
        risk_level: saved_analysis.risk_level,
        seller: seller_response,
        signals,
        network_summary: claude_analysis.overall_risk_notes,
        fraud_report_count: fraud_count,
    }))
}

/// POST /api/v1/analyze
///
/// It's the actual, real endpoint your extension calls, it runs the entire
/// fraud-analysis process, step by step, from checking who's asking, all the way
/// to sending back a complete, saved risk report.
///
/// It confirms who's calling, and that they're allowed to right now, splits the
/// incoming request into its two separate pieces, finds or creates the seller,
/// with correctly-set verification, creates the listing itself, runs the actual
/// Claude analysis, builds the complete signal list, domain check included,
/// calculates the actual risk score, and converts it into a risk level and
/// saves everything, and builds the final response.
pub async fn analyze(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(request): Json<AnalyzeRequest>,
) -> Result<Json<AnalyzeResponse>, AnalyzeError> {
    let user_id = authorize_request(&headers, &pool).await?;

    let (seller_req, listing_req) = build_requests(&request);

    let platform_id = request.platform_id.as_deref().unwrap_or("");
    let resolved = resolve_seller(&pool, &seller_req, &request.platform, platform_id).await?;

    let listing = create_listing(&pool, &listing_req, resolved.seller.id)
        .await
        .map_err(|e| AnalyzeError::Database(e.to_string()))?;

    let claude_analysis = run_claude_analysis(&listing, &resolved.seller).await?;

    let signals = build_all_signals(&claude_analysis, &resolved.seller, &request);

    let risk_score = calculate_risk_score(&claude_analysis, resolved.fraud_count);
    let risk_level = match risk_score {
        0..=33 => RiskLevel::Low,
        34..=66 => RiskLevel::Caution,
        _ => RiskLevel::High,
    };

    save_and_build_response(
        &pool,
        listing.id,
        risk_score,
        risk_level,
        signals,
        claude_analysis,
        user_id,
        resolved.seller,
        resolved.fraud_count,
        resolved.network_summary,
    )
    .await
}
