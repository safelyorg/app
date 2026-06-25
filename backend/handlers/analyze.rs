use crate::{
    models::{
        analysis::{AnalyzeRequest, AnalyzeResponse, RiskLevel},
        helpers::format_account_age,
        listings::ListingsRequest,
        sellers::{SellersRequest, SellersResponse},
    },
    services::{
        analysis::create_analysis,
        claude::call_claude,
        fraud_reports::{build_network_summary, count_fraud_reports},
        listings::create_listing,
        scoring::calculate_risk_score,
        sellers::{create_seller, find_seller},
        signals::build_signals,
    },
};
use axum::{Json, extract::State};
use sqlx::{Pool, Postgres};

pub async fn analyze(
    State(pool): State<Pool<Postgres>>,
    Json(request): Json<AnalyzeRequest>,
) -> Result<Json<AnalyzeResponse>, String> {
    let seller_req = SellersRequest {
        platform: request.platform.clone(),
        platform_id: request.seller_platform_id.clone(),
        name: request.seller_name.clone(),
        handle: request.seller_handle.clone(),
        phone: request.seller_phone.clone(),
        profile_url: request.seller_profile_url.clone(),
        join_date: request.seller_join_date.clone(),
        location: request.seller_location,
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

    let platform_id = request.seller_platform_id.as_deref().unwrap_or("");
    let seller = match find_seller(&pool, &request.platform, platform_id)
        .await
        .map_err(|e| e.to_string())?
    {
        Some(existing) => existing,
        None => create_seller(&pool, &seller_req)
            .await
            .map_err(|e| e.to_string())?,
    };

    let fraud_count = count_fraud_reports(&pool, seller.id)
        .await
        .map_err(|e| e.to_string())?;
    let network_summary = build_network_summary(fraud_count);

    let listing = create_listing(&pool, &listing_req, seller.id)
        .await
        .map_err(|e| e.to_string())?;

    let account_age = seller
        .join_date
        .map(|d| format_account_age(d))
        .unwrap_or_else(|| "Unknown".to_string());

    let claude_analysis = call_claude(
        &listing.platform,
        seller.name.as_deref().unwrap_or("Unknown"),
        &account_age,
        seller.total_deals,
        seller.disputes,
        listing.title.as_deref().unwrap_or("Untitled"),
        listing.price.unwrap_or(0),
        listing.description.as_deref().unwrap_or(""),
    )
    .await
    .map_err(|e| e.to_string())?;

    let signals = build_signals(&claude_analysis, &seller);
    let risk_score = calculate_risk_score(&claude_analysis, fraud_count);
    let risk_level = match risk_score {
        0..=33 => RiskLevel::Low,
        34..=66 => RiskLevel::Caution,
        _ => RiskLevel::High,
    };

    let signals_json = serde_json::to_value(&signals).map_err(|e| e.to_string())?;

    let saved_analysis = create_analysis(
        &pool,
        listing.id,
        risk_score,
        risk_level,
        signals_json,
        claude_analysis.overall_risk_notes.clone(),
        String::new(),
    )
    .await
    .map_err(|e| e.to_string())?;

    let mut seller_response = SellersResponse::from(seller);
    seller_response.network_summary = network_summary;

    Ok(Json(AnalyzeResponse {
        risk_score: saved_analysis.risk_score,
        risk_level: saved_analysis.risk_level,
        seller: seller_response,
        signals,
        network_summary: claude_analysis.overall_risk_notes,
    }))
}
