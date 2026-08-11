use crate::{
    models::{
        analysis::{AnalyzeRequest, AnalyzeResponse, RiskLevel},
        helpers::format_account_age,
        listings::ListingsRequest,
        sellers::{SellerVerification, SellersRequest, SellersResponse},
    },
    services::{
        analysis::create_analysis,
        auth::extract_user_id,
        claude::call_claude,
        fraud_reports::{build_network_summary, count_fraud_reports},
        listings::{create_listing, get_monthly_visit_activity},
        scoring::calculate_risk_score,
        sellers::{create_seller, find_seller},
        signals::{build_domain_signal, build_signals},
    },
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

// This stops any one person from calling the /analyze endpoint more than 10 times
// within any 5-minute stretch.
//
// It sets up a way to track how many times each logged-in user has called the expensive
// /analyze endpoint recently.
static RATE_LIMITS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<Uuid, (u32, std::time::Instant)>>,
> = std::sync::OnceLock::new();

const RATE_LIMIT_WINDOW: std::time::Duration = std::time::Duration::from_secs(300);
const RATE_LIMIT_MAX_REQUESTS: u32 = 10;

/// Every time someone tries to use /analyze, this checks their notebook entry
/// lets them through and counts it, unless they've already hit 10 within the last 5 minutes,
/// in which case it tells them exactly how many seconds left until they can try again.
///
/// Gets the notebook (creating it, if this is the very first time), finds this person's
/// page in the notebook — or creates one, if they've never called before, checks
/// if their 5-minute window has already run out and counts the current request,
/// checks if they're still under the limit. If they've gone over and calculate
/// exactly how long they need to wait.
pub fn check_rate_limit(user_id: Uuid) -> Result<(), u64> {
    let map = RATE_LIMITS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    let mut map = map.lock().unwrap();
    let now = std::time::Instant::now();

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
        Err(remaining.as_secs())
    }
}

pub async fn analyze(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(request): Json<AnalyzeRequest>,
) -> Result<Json<AnalyzeResponse>, (StatusCode, String)> {
    // Real analysis costs real Claude API money per request, so this is
    // the one endpoint that must actually reject an anonymous caller
    // rather than just proceed without a user_id - the extension itself
    // already gates this on its own side (showing a sign-in prompt
    // instead of ever calling this endpoint), but that check lives in a
    // browser and can be bypassed by anyone willing to call this URL
    // directly. This is what makes that bypass actually pointless.
    let user_id = extract_user_id(&headers, &pool)
        .await
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in required".to_string()))?;

    // Checked right after login, before any real work (or Claude cost)
    // happens - a signed-in account is still just one person, and one
    // person calling this dozens of times a minute is either a stuck
    // script or someone testing the limits of the system, not genuine
    // browsing.
    // Checked right after login, before any real work (or Claude cost)
    // happens - a signed-in account is still just one person, and one
    // person calling this dozens of times a minute is either a stuck
    // script or someone testing the limits of the system, not genuine
    // browsing. The exact remaining seconds is calculated here (not
    // guessed by the browser) and passed along so the person can be
    // told a real, accurate wait time rather than a vague message.
    if let Err(retry_after_secs) = check_rate_limit(user_id) {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            format!("RATE_LIMITED:{}", retry_after_secs),
        ));
    }

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
        category: request.category,
        image_urls: request.image_urls.clone(),
        posted_date: request.posted_date.clone(),
    };

    let platform_id = request.platform_id.as_deref().unwrap_or("");
    let existing_seller = find_seller(&pool, &request.platform, platform_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let preliminary_fraud_count = if let Some(ref s) = existing_seller {
        count_fraud_reports(&pool, s.id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        0
    };

    let verification = if preliminary_fraud_count > 0 {
        SellerVerification::Reported
    } else {
        SellerVerification::Unknown
    };

    let seller = create_seller(&pool, &seller_req, verification)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let fraud_count = count_fraud_reports(&pool, seller.id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let network_summary = build_network_summary(fraud_count);

    let listing = create_listing(&pool, &listing_req, seller.id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let account_age = seller
        .join_date
        .map(|d| format_account_age(d))
        .unwrap_or_else(|| "Unknown".to_string());

    let image_urls = listing_req.image_urls.as_deref().unwrap_or(&[]);

    let claude_analysis = call_claude(
        &listing.platform,
        seller.name.as_deref().unwrap_or("Unknown"),
        &account_age,
        listing.title.as_deref().unwrap_or("Untitled"),
        listing.price.unwrap_or(0),
        listing.description.as_deref().unwrap_or(""),
        image_urls,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut signals = build_signals(&claude_analysis, &seller);

    // Domain check appears here as one more listing signal, alongside
    // the AI-driven ones - matches the exact same Intelligence tab
    // display everything else already uses. Placed first, as the most
    // fundamental thing to know before trusting anything else shown.
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

    let risk_score = calculate_risk_score(&claude_analysis, fraud_count);
    let risk_level = match risk_score {
        0..=33 => RiskLevel::Low,
        34..=66 => RiskLevel::Caution,
        _ => RiskLevel::High,
    };

    let signals_json = serde_json::to_value(&signals)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let saved_analysis = create_analysis(
        &pool,
        listing.id,
        risk_score,
        risk_level,
        signals_json,
        claude_analysis.overall_risk_notes.clone(),
        String::new(),
        user_id,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let monthly_activity = get_monthly_visit_activity(&pool, seller.id)
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
