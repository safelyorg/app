use crate::models::fraud_reports::FraudReportsRequest;
use crate::services::auth::extract_user_id;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

pub async fn create_fraud_report(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(request): Json<FraudReportsRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Reporting doesn't cost real money the way /analyze does, but it's
    // still worth requiring a real account - it stops a stranger's
    // website from silently submitting fake reports through an
    // innocent visitor's browser, and ties every report to a genuine
    // person rather than an anonymous, unaccountable submission.
    let user_id = extract_user_id(&headers, &pool)
        .await
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in required".to_string()))?;

    let seller = sqlx::query(
        "SELECT id FROM sellers
         WHERE platform = $1 AND platform_id = $2
         LIMIT 1",
    )
    .bind(&request.platform)
    .bind(request.platform_id.as_deref().unwrap_or(""))
    .fetch_optional(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let seller_id: Uuid = match seller {
        Some(row) => row.get("id"),
        None => return Err((StatusCode::NOT_FOUND, "Seller not found".to_string())),
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
    .bind(user_id)
    .execute(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // The frontend already shows "Reported" immediately for this same
    // session as soon as submission succeeds - but that's only ever a
    // temporary, in-memory display update. This is what makes it real
    // and permanent: without this, the dashboard (reading the seller's
    // actual stored status later) would keep showing "Unknown" forever,
    // since nothing had ever actually updated the real record.
    sqlx::query("UPDATE sellers SET verification = 'reported' WHERE id = $1")
        .bind(seller_id)
        .execute(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({ "success": true })))
}
