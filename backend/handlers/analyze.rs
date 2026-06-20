use crate::{
    models::{
        analysis::AnalyzeRequest,
        helpers::format_account_age,
        listings::ListingsRequest,
        sellers::{SellersRequest, SellersResponse},
    },
    services::{
        claude::call_claude,
        listings::{create_listing, find_listing},
        sellers::{create_seller, find_seller},
    },
};
use axum::{Json, extract::State};
use sqlx::{Pool, Postgres};

pub async fn analyze(
    State(pool): State<Pool<Postgres>>,
    Json(request): Json<AnalyzeRequest>,
) -> Result<Json<SellersResponse>, String> {
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

    let listing = match find_listing(&pool, &request.listing_url)
        .await
        .map_err(|e| e.to_string())?
    {
        Some(existing) => existing,
        None => create_listing(&pool, &listing_req, seller.id)
            .await
            .map_err(|e| e.to_string())?,
    };

    let account_age = seller
        .join_date
        .map(|d| format_account_age(d))
        .unwrap_or_else(|| "Unknown".to_string());

    let _ = call_claude(
        &listing.platform,
        seller.name.as_deref().unwrap_or("Unknown"),
        &account_age,
        seller.total_deals,
        seller.disputes,
        listing.title.as_deref().unwrap_or("Untitled"),
        listing.price.unwrap_or(0),
        listing.description.as_deref().unwrap_or(""),
    )
    .await;

    Ok(Json(SellersResponse::from(seller)))
}
