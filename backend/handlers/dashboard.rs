use crate::services::{
    auth::{extract_user_id, find_user_by_id},
    history::{get_history_detail, get_user_history, get_user_reports},
};
use axum::{
    Json,
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
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
    let method_row = sqlx::query(
        "SELECT last_login_method, (avatar_data IS NOT NULL) AS has_avatar
         FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let last_login_method: Option<String> = method_row.get("last_login_method");
    let has_avatar: bool = method_row.get("has_avatar");

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
        "has_avatar": has_avatar,
    })))
}

/// POST /api/v1/me/avatar
/// Accepts a single image file (PNG/JPEG/WEBP, 2MB max) and stores the
/// raw bytes directly in the database - no filesystem involved at all,
/// so the image lives and travels with the rest of the account data
/// (backups, restores, migrations between hosts all just work, since
/// there's no separate file to remember to move alongside the DB).
pub async fn upload_avatar(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in required".to_string()))?;

    let mut file_bytes: Option<Vec<u8>> = None;
    let mut content_type: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        if field.name() == Some("avatar") {
            content_type = field.content_type().map(|s| s.to_string());
            let data = field
                .bytes()
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
            file_bytes = Some(data.to_vec());
        }
    }

    let bytes = file_bytes.ok_or((StatusCode::BAD_REQUEST, "No file provided".to_string()))?;

    if bytes.len() > 2 * 1024 * 1024 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Image must be 2MB or smaller".to_string(),
        ));
    }

    let ct = match content_type.as_deref() {
        Some("image/png") => "image/png",
        Some("image/jpeg") => "image/jpeg",
        Some("image/webp") => "image/webp",
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Only PNG, JPEG, or WEBP images are allowed".to_string(),
            ));
        }
    };

    sqlx::query("UPDATE users SET avatar_data = $1, avatar_content_type = $2 WHERE id = $3")
        .bind(&bytes)
        .bind(ct)
        .bind(user_id)
        .execute(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({ "success": true })))
}

/// GET /api/v1/me/avatar
/// Returns the raw image bytes with the correct Content-Type, so the
/// browser can render them directly - this is authenticated the same
/// way as every other endpoint (Bearer token), which is exactly why the
/// frontend can't just point a plain <img src="..."> at this URL; it
/// has to fetch the bytes itself with the auth header attached, then
/// hand the result to the browser as an object URL.
pub async fn get_avatar(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in required".to_string()))?;

    let row = sqlx::query("SELECT avatar_data, avatar_content_type FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let data: Option<Vec<u8>> = row.get("avatar_data");
    let content_type: Option<String> = row.get("avatar_content_type");

    let bytes = data.ok_or((StatusCode::NOT_FOUND, "No avatar set".to_string()))?;
    let ct = content_type.unwrap_or_else(|| "image/png".to_string());

    Ok(([(header::CONTENT_TYPE, ct)], bytes))
}
