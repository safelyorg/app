use crate::models::fraud_reports::FraudReportsRequest;
use crate::services::auth::extract_user_id;
use axum::{Json, extract::State, http::HeaderMap};
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

pub async fn create_fraud_report(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(request): Json<FraudReportsRequest>,
) -> Result<Json<serde_json::Value>, String> {
    let user_id = extract_user_id(&headers, &pool).await;

    let seller = sqlx::query(
        "SELECT id FROM sellers
         WHERE platform = $1 AND platform_id = $2
         LIMIT 1",
    )
    .bind(&request.platform)
    .bind(request.platform_id.as_deref().unwrap_or(""))
    .fetch_optional(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let seller_id: Uuid = match seller {
        Some(row) => row.get("id"),
        None => return Err("Seller not found".to_string()),
    };

    let id = Uuid::now_v7();

    sqlx::query(
        "INSERT INTO fraud_reports (
            id, seller_id, platform, platform_id,
            report_type, description, listing_url, user_id, reported_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())",
    )
    .bind(id)
    .bind(seller_id)
    .bind(&request.platform)
    .bind(request.platform_id.as_deref().unwrap_or(""))
    .bind(&request.report_type)
    .bind(&request.description)
    .bind(&request.listing_url)
    .bind(&user_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        println!("DEBUG INSERT error: {}", e);
        e.to_string()
    })?;

    Ok(Json(serde_json::json!({ "success": true })))
}
