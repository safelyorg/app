use crate::services::{
    auth::extract_user_id,
    history::{get_history_detail, get_user_history, get_user_reports},
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

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

/// GET /api/v1/history/:id
/// Full detail for one listing analysis - the rich Risk/Intelligence view.
pub async fn get_history_item(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in required".to_string()))?;

    let detail = get_history_detail(&pool, id, user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match detail {
        Some(d) => {
            Ok(Json(serde_json::to_value(d).map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            })?))
        }
        None => Err((StatusCode::NOT_FOUND, "Not found".to_string())),
    }
}
