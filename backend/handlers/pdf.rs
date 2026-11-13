use crate::{
    errors::dashboard::DashboardError,
    services::{
        features::auth::extract_user_id, features::history::get_history_detail,
        features::pdf_report::generate_evidence_pdf,
        features::results::sees_full_results,
    },
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::json;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

/// GET /api/v1/history/{id}/pdf
///
/// Generates and returns a real, downloadable PDF evidence report for
/// one specific, past analysis - reusing the exact same, real data
/// already shown on the dashboard's history detail view.
pub async fn download_evidence_pdf(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Response, DashboardError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| DashboardError::InternalError("Failed to verify session".to_string()))?
        .ok_or(DashboardError::Unauthorized)?;

    let detail = get_history_detail(&pool, id, user_id)
        .await
        .map_err(|e| DashboardError::InternalError(e.to_string()))?
        .ok_or_else(|| DashboardError::NotFound("Not found".to_string()))?;

    // The PDF is the full report: paid plans only (results.rs).
    if !sees_full_results(&pool, user_id).await {
        return Ok((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "needs_paid_plan" })),
        )
            .into_response());
    }

    let pdf_bytes = generate_evidence_pdf(&detail).map_err(DashboardError::InternalError)?;

    let filename = format!("safely-evidence-{}.pdf", id);

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", filename),
            ),
        ],
        pdf_bytes,
    )
        .into_response())
}
