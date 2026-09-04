use crate::{
    errors::analyze::AnalyzeError,
    models::analysis::{AnalyzeRequest, AnalyzeResponse, RiskLevel},
    services::{
        analysis::{
            BuildResponseData, authorize_request, build_all_signals, build_b2b_analysis_path,
            build_requests, resolve_seller, run_claude_analysis, save_and_build_response,
        },
        b2b_scrapers::get_scraper_for_platform,
        b2c_scrapers::{requires_client_side_scraping, check_listing_page},
        listings::create_listing,
        scoring::calculate_risk_score,
        sellers::update_seller_from_b2b,
        signals::sort_signals_by_table,
    },
};
use axum::{Json, extract::State, http::HeaderMap};
use sqlx::{Pool, Postgres};

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
    let mut request = request;
    if !requires_client_side_scraping(&request.platform) {
        if let Some(scraped) = check_listing_page(&request.platform, &request.listing_url).await {
            if request.title.is_none() {
                request.title = scraped.title;
            }
            if request.price.is_none() {
                request.price = scraped.price;
            }
            if request.description.is_none() {
                request.description = scraped.description;
            }
            if request.seller_name.is_none() {
                request.seller_name = scraped.seller_name;
            }
            if request.seller_location.is_none() {
                request.seller_location = scraped.location;
            }
            if request.platform_id.is_none() {
                request.platform_id = scraped.platform_id;
            }
            if request.seller_profile_url.is_none() {
                request.seller_profile_url = scraped.seller_profile_url;
            }
            if request.seller_last_active.is_none() {
                request.seller_last_active = scraped.last_active;
            }
            request.seller_verified = Some(scraped.seller_verified);
            if request.seller_rating.is_none() {
                request.seller_rating = scraped.seller_rating;
            }
            if request.seller_total_products.is_none() {
                request.seller_total_products = scraped.seller_total_products;
            }
            if request.seller_join_date.is_none() {
                request.seller_join_date = scraped.seller_join_date;
            }
            if request.image_urls.is_none() && !scraped.image_urls.is_empty() {
                request.image_urls = Some(scraped.image_urls);
            }
            if request.seller_website.is_none() {
                request.seller_website = scraped.seller_website;
            }
        }
    }

    let (seller_req, listing_req) = build_requests(&request);
    let platform_id = request.platform_id.as_deref().unwrap_or("");
    let resolved = resolve_seller(&pool, &seller_req, &request.platform, platform_id).await?;

    let listing = create_listing(&pool, &listing_req, resolved.seller.id)
        .await
        .map_err(|e| AnalyzeError::Database(e.to_string()))?;

    let is_b2b = get_scraper_for_platform(&request.platform).is_some();

    let mut resolved = resolved;

    let (signals, risk_score, overall_risk_notes) = if is_b2b {
        let (signals, risk_score, notes, supplier) =
            build_b2b_analysis_path(&pool, &request, resolved.fraud_count, resolved.seller.id)
                .await?;

        let _ = update_seller_from_b2b(
            &pool,
            resolved.seller.id,
            supplier.company_name.as_deref(),
            supplier.country.as_deref(),
        )
        .await;
        if supplier.company_name.is_some() {
            resolved.seller.name = supplier.company_name.clone();
        }
        if supplier.country.is_some() {
            resolved.seller.location = supplier.country.clone();
        }

        (signals, risk_score, notes)
    } else {
        let claude_analysis = run_claude_analysis(&listing, &resolved.seller).await?;
        let signals = build_all_signals(&pool, &claude_analysis, &resolved.seller, &request).await;
        let risk_score = calculate_risk_score(&claude_analysis, resolved.fraud_count);
        let notes = claude_analysis.overall_risk_notes.clone();
        (signals, risk_score, notes)
    };

    let mut signals = signals;
    sort_signals_by_table(&mut signals);

    let risk_level = match risk_score {
        0..=33 => RiskLevel::Low,
        34..=66 => RiskLevel::Caution,
        _ => RiskLevel::High,
    };

    let data = BuildResponseData {
        pool: &pool,
        listing_id: listing.id,
        risk_score,
        risk_level,
        signals,
        overall_risk_notes,
        user_id,
        seller: resolved.seller,
        fraud_count: resolved.fraud_count,
        network_summary: resolved.network_summary,
        is_b2b,
    };

    save_and_build_response(data).await
}
