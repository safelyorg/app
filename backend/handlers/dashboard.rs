use crate::services::{
    auth::{extract_user_id, find_user_by_id},
    history::{get_history_detail, get_user_history, get_user_reports},
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

/// GET /api/v1/history
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

#[derive(Debug, serde::Deserialize)]
pub struct UpdateMeRequest {
    pub name: String,
}

/// PATCH /api/v1/me
/// Currently only lets a person set/change their display name - this is
/// the one field magic-link users have no other way of ever getting
/// populated (there's no name to scrape from just an email address,
/// unlike Google sign-in which provides one automatically).
pub async fn update_me(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(req): Json<UpdateMeRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in required".to_string()))?;

    let trimmed = req.name.trim();
    if trimmed.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Name cannot be empty".to_string()));
    }
    if trimmed.chars().count() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Name must be 100 characters or fewer".to_string(),
        ));
    }

    sqlx::query("UPDATE users SET name = $1 WHERE id = $2")
        .bind(trimmed)
        .bind(user_id)
        .execute(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(
        serde_json::json!({ "success": true, "name": trimmed }),
    ))
}

/// GET /api/v1/me
/// Read-only account info for the Settings page - email, name, how the
/// person signed in, and account dates. Deliberately minimal: no plan/
/// billing section here, since there's no subscription system wired up
/// yet - showing one would be UI pretending a feature exists that doesn't.
pub async fn get_me(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in required".to_string()))?;

    let user = find_user_by_id(&pool, user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "User not found".to_string()))?;

    // Reads last_login_method directly rather than going through the User
    // struct, so this doesn't depend on that struct having been updated to
    // include the new column - a raw query here is a small, self-contained
    // way to add this without needing to touch models/users.rs at all.
    let method_row = sqlx::query("SELECT last_login_method FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let last_login_method: Option<String> = method_row.get("last_login_method");

    // Falls back to the old google_id-based guess only for accounts that
    // haven't logged in since this column was added (still NULL) - every
    // login going forward sets this explicitly and accurately.
    let signed_in_with = last_login_method.unwrap_or_else(|| {
        if user.google_id.is_some() {
            "google".to_string()
        } else {
            "email".to_string()
        }
    });

    Ok(Json(serde_json::json!({
        "email": user.email,
        "name": user.name,
        "signed_in_with": signed_in_with,
        "created_at": user.created_at,
        "last_login_at": user.last_login_at,
    })))
}
