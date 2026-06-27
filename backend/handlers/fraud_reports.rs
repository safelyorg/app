use crate::models::fraud_reports::FraudReportsRequest;
use axum::{Json, extract::State};
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

pub async fn create_fraud_report(
    State(pool): State<Pool<Postgres>>,
    Json(request): Json<FraudReportsRequest>,
) -> Result<Json<serde_json::Value>, String> {
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
    let report_type = request.report_type.to_string();

    let result = sqlx::query(
        "INSERT INTO fraud_reports (
            id, seller_id, platform, platform_id,
            report_type, description, reported_at
        )
        VALUES ($1, $2, $3, $4, $5::report_types, $6, NOW())",
    )
    .bind(id)
    .bind(seller_id)
    .bind(&request.platform)
    .bind(request.platform_id.as_deref().unwrap_or(""))
    .bind(&report_type)
    .bind(&request.description)
    .execute(&pool)
    .await
    .map_err(|e| {
        println!("DEBUG INSERT error: {}", e);
        e.to_string()
    })?;

    Ok(Json(serde_json::json!({ "success": true })))
}
