use crate::{
    errors::outcomes::OutcomeError,
    models::outcomes::OutcomeRequest,
    services::{auth::extract_user_id, outcomes::record_outcome},
};
use axum::{Json, extract::State, http::HeaderMap};
use serde_json::{Value, json};
use sqlx::{Pool, Postgres};

/// POST /api/v1/outcomes
///
/// Records what the person genuinely decided to do after seeing an
/// analysis - proceeded or backed out - as real, permanent Layer 12
/// feedback data.
pub async fn create_outcome(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(request): Json<OutcomeRequest>,
) -> Result<Json<Value>, OutcomeError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| OutcomeError::InternalError("Failed to verify session".to_string()))?
        .ok_or(OutcomeError::Unauthorized)?;

    record_outcome(&pool, request.analysis_id, user_id, request.action)
        .await
        .map_err(|e| OutcomeError::InternalError(e.to_string()))?;

    Ok(Json(json!({ "status": "recorded" })))
}
