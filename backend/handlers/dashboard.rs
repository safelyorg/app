use crate::services::{
    auth::extract_user_id,
    history::{get_user_history, get_user_reports},
};
use axum::{Json, extract::State, http::HeaderMap, http::StatusCode};
use sqlx::{Pool, Postgres};

/// GET /api/v1/history
/// Unlike analyze/report, this endpoint REQUIRES a valid session - there's
/// no "anonymous history" to show, so a missing/invalid token is a real
/// 401 here rather than something to quietly proceed past.
pub async fn get_history(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in required".to_string()))?;

    let items = get_user_history(&pool, user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({ "history": items })))
}

/// GET /api/v1/reports
pub async fn get_reports(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in required".to_string()))?;

    let items = get_user_reports(&pool, user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({ "reports": items })))
}
