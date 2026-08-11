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
async fn authorize_request(
    headers: &HeaderMap,
    pool: &Pool<Postgres>,
) -> Result<Uuid, AnalyzeError> {
    let user_id = extract_user_id(headers, pool)
        .await
        .ok_or(AnalyzeError::Unauthorized)?;

    check_rate_limit(user_id)?;

    Ok(user_id)
}

/// Converts the incoming request into the two separate shapes the
/// seller and listing creation functions each expect.
fn build_requests(request: &AnalyzeRequest) -> (SellersRequest, ListingsRequest) {
    let seller_req = SellersRequest {
        platform: request.platform.clone(),
        platform_id: request.platform_id.clone(),
        name: request.seller_name.clone(),
        handle: request.seller_handle.clone(),
        phone: request.seller_phone.clone(),
        profile_url: request.seller_profile_url.clone(),
        join_date: request.seller_join_date.clone(),
        location: request.seller_location.clone(),
        last_active: request.seller_last_active.clone(),
    };

    let listing_req = ListingsRequest {
        seller_id: request.seller_id,
        platform: request.platform.clone(),
        listing_url: request.listing_url.clone(),
        listing_id: request.listing_id.clone(),
        title: request.title.clone(),
        price: request.price,
        description: request.description.clone(),
        category: request.category.clone(),
        image_urls: request.image_urls.clone(),
        posted_date: request.posted_date.clone(),
    };

    (seller_req, listing_req)
}

/// Finds or creates this seller, checking their fraud history BEFORE
/// creation so the initial verification status reflects whether
/// they've already been reported - then returns the real seller row
/// alongside the final fraud count and a summary built from it.
async fn resolve_seller(
    pool: &Pool<Postgres>,
    seller_req: &SellersRequest,
    platform: &str,
    platform_id: &str,
) -> Result<(Sellers, i64, String), AnalyzeError> {
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

    Ok((seller, fraud_count, network_summary))
}

/// Runs the actual Claude analysis on this listing - computes the
/// seller's account age first, since that's one of the inputs Claude
/// needs alongside the listing's own details.
async fn run_claude_analysis(
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

/// Builds the full signal list Claude's analysis produces, then adds
/// the domain-mismatch check as one more signal, placed first since
/// it's the most fundamental thing to know before trusting anything
/// else shown.
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

/// Saves the completed analysis, fetches the seller's visit history for
/// the chart, and assembles the final response the caller actually sees.
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
pub async fn analyze(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(request): Json<AnalyzeRequest>,
) -> Result<Json<AnalyzeResponse>, AnalyzeError> {
    let user_id = authorize_request(&headers, &pool).await?;

    let (seller_req, listing_req) = build_requests(&request);

    let platform_id = request.platform_id.as_deref().unwrap_or("");
    let (seller, fraud_count, network_summary) =
        resolve_seller(&pool, &seller_req, &request.platform, platform_id).await?;

    let listing = create_listing(&pool, &listing_req, seller.id)
        .await
        .map_err(|e| AnalyzeError::Database(e.to_string()))?;

    let claude_analysis = run_claude_analysis(&listing, &seller).await?;

    let signals = build_all_signals(&claude_analysis, &seller, &request);

    let risk_score = calculate_risk_score(&claude_analysis, fraud_count);
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
        seller,
        fraud_count,
        network_summary,
    )
    .await
}
