use crate::{
    errors::dashboard::DashboardError,
    services::{
        auth::{extract_user_id, find_user_by_id, set_login_method},
        dashboard::{delete_user_account, unlink_google_account},
        history::{get_history_detail, get_user_history, get_user_reports},
    },
};
use axum::{
    Json,
    extract::{Multipart, Path, State},
    http::{HeaderMap, header::CONTENT_TYPE},
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::{Value, json, to_value};
use sqlx::{Pool, Postgres, Row, query};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct UpdateMeRequest {
    pub name: String,
}

/// GET /api/v1/history
///
/// It hands back the person's real, complete analysis history — every
/// listing they've ever checked through Safely — so the dashboard can
/// show them a genuine record of what they've looked into.
///
/// It confirms who's genuinely signed in, then fetches their real
/// history from the database, reporting back specifically if either
/// step goes wrong, rather than a single, generic failure.
pub async fn get_history(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<Value>, DashboardError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| DashboardError::InternalError("Failed to verify session".to_string()))?
        .ok_or(DashboardError::Unauthorized)?;

    let items = get_user_history(&pool, user_id)
        .await
        .map_err(|e| DashboardError::InternalError(e.to_string()))?;

    Ok(Json(json!({ "history": items })))
}

/// GET /api/v1/reports
///
/// It hands back the real fraud reports this person has personally
/// submitted through Safely, so the dashboard can show them their own
/// contribution history.
///
/// It confirms who's genuinely signed in, then fetches their real
/// reports from the database, reporting back specifically if either
/// step goes wrong, rather than a single, generic failure.
pub async fn get_reports(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<Value>, DashboardError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| DashboardError::InternalError("Failed to verify session".to_string()))?
        .ok_or(DashboardError::Unauthorized)?;

    let items = get_user_reports(&pool, user_id)
        .await
        .map_err(|e| DashboardError::InternalError(e.to_string()))?;

    Ok(Json(json!({ "reports": items })))
}

/// GET /api/v1/history/{id}
///
/// It hands back the full, detailed record of one specific analysis the
/// person ran in the past, so they can revisit exactly what Safely told
/// them about a listing, rather than just seeing it in a summarized
/// list.
///
/// It confirms who's genuinely signed in, looks up that specific
/// analysis by its real ID, and confirms it actually belongs to this
/// person before returning it — a genuinely missing or someone-else's
/// analysis both correctly report back as not found, rather than
/// leaking whether a given ID exists at all.
pub async fn get_history_item(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, DashboardError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| DashboardError::InternalError("Failed to verify session".to_string()))?
        .ok_or(DashboardError::Unauthorized)?;

    let detail = get_history_detail(&pool, id, user_id)
        .await
        .map_err(|e| DashboardError::InternalError(e.to_string()))?;

    match detail {
        Some(d) => {
            let value = to_value(d).map_err(|e| DashboardError::InternalError(e.to_string()))?;
            Ok(Json(value))
        }
        None => Err(DashboardError::NotFound("Not found".to_string())),
    }
}

/// PATCH /api/v1/me
///
/// Currently only lets a person set/change their display name - this is
/// the one field magic-link users have no other way of ever getting
/// populated (there's no name to scrape from just an email address,
/// unlike Google sign-in which provides one automatically).
///
/// It confirms who's genuinely signed in, checks the new name is
/// neither empty nor unreasonably long, then saves it - reporting back
/// specifically which of those checks failed, rather than a single,
/// generic rejection.
pub async fn update_me(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(req): Json<UpdateMeRequest>,
) -> Result<Json<Value>, DashboardError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| DashboardError::InternalError("Failed to verify session".to_string()))?
        .ok_or(DashboardError::Unauthorized)?;

    let trimmed = req.name.trim();
    if trimmed.is_empty() {
        return Err(DashboardError::BadRequest(
            "Name cannot be empty".to_string(),
        ));
    }

    if trimmed.chars().count() > 100 {
        return Err(DashboardError::BadRequest(
            "Name must be 100 characters or fewer".to_string(),
        ));
    }

    query("UPDATE users SET name = $1 WHERE id = $2")
        .bind(trimmed)
        .bind(user_id)
        .execute(&pool)
        .await
        .map_err(|e| DashboardError::InternalError(e.to_string()))?;

    Ok(Json(json!({ "success": true, "name": trimmed })))
}

/// GET /api/v1/me
///
/// Read-only account info for the Settings page - email, name, how the
/// person signed in, and account dates. Deliberately minimal: no plan/
/// billing section here, since there's no subscription system wired up
/// yet - showing one would be UI pretending a feature exists that doesn't.
///
/// It confirms who's genuinely signed in, looks up their real account
/// row, then a second, small query pulls in two extra fields
/// (last_login_method, whether an avatar is set) that aren't part of
/// the main User struct yet - reporting back specifically if any of
/// these steps fail, rather than a single, generic failure.
pub async fn get_me(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<Value>, DashboardError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| DashboardError::InternalError("Failed to verify session".to_string()))?
        .ok_or(DashboardError::Unauthorized)?;

    let user = find_user_by_id(&pool, user_id)
        .await
        .map_err(|e| DashboardError::InternalError(e.to_string()))?
        .ok_or(DashboardError::NotFound("User not found".to_string()))?;

    let method_row = query(
        "SELECT last_login_method, (avatar_data IS NOT NULL) AS has_avatar
         FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| DashboardError::InternalError(e.to_string()))?;

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

    Ok(Json(json!({
        "email": user.email,
        "name": user.name,
        "signed_in_with": signed_in_with,
        "created_at": user.created_at,
        "last_login_at": user.last_login_at,
        "has_avatar": has_avatar,
        "google_linked": user.google_id.is_some(),
    })))
}

/// DELETE /api/v1/me
///
/// Permanently deletes the account. See delete_user_account in
/// services/auth.rs for exactly what happens to each piece of data.
///
/// It confirms who's genuinely signed in, then hands off to the real
/// deletion logic - reporting back if either step fails, rather than a
/// single, generic failure.
pub async fn delete_account(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<Value>, DashboardError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| DashboardError::InternalError("Failed to verify session".to_string()))?
        .ok_or(DashboardError::Unauthorized)?;

    delete_user_account(&pool, user_id)
        .await
        .map_err(|e| DashboardError::InternalError(e.to_string()))?;

    Ok(Json(json!({ "success": true })))
}

/// POST /api/v1/me/google/disconnect
///
/// Always safe to allow: email is the one identity that's never optional
/// on this account (even a Google-only signup has an email captured from
/// the Google profile), so disconnecting Google never locks anyone out -
/// they can always fall back to a magic link on that same email.
///
/// It confirms who's genuinely signed in, clears the Google connection
/// from their account, then immediately updates their "signed in with"
/// label to email - since Google no longer works as a way back in for
/// this account, the label should reflect that right away, not just
/// after their next actual login happens to update it.
pub async fn disconnect_google(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<Value>, DashboardError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| DashboardError::InternalError("Failed to verify session".to_string()))?
        .ok_or(DashboardError::Unauthorized)?;

    unlink_google_account(&pool, user_id)
        .await
        .map_err(|e| DashboardError::InternalError(e.to_string()))?;

    let _ = set_login_method(&pool, user_id, "email").await;

    Ok(Json(json!({ "success": true, "signed_in_with": "email" })))
}

/// POST /api/v1/me/avatar
///
/// Accepts a single image file (PNG/JPEG/WEBP, 2MB max) and stores the
/// raw bytes directly in the database - no filesystem involved at all,
/// so the image lives and travels with the rest of the account data
/// (backups, restores, migrations between hosts all just work, since
/// there's no separate file to remember to move alongside the DB).
///
/// It confirms who's genuinely signed in, reads the uploaded file from
/// the real multipart request, checks it's genuinely present, under
/// the size limit, and a real, supported image type - reporting back
/// specifically which of those checks failed, rather than a single,
/// generic rejection.
pub async fn upload_avatar(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<Value>, DashboardError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| DashboardError::InternalError("Failed to verify session".to_string()))?
        .ok_or(DashboardError::Unauthorized)?;

    let mut file_bytes: Option<Vec<u8>> = None;
    let mut content_type: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| DashboardError::BadRequest(e.to_string()))?
    {
        if field.name() == Some("avatar") {
            content_type = field.content_type().map(|s| s.to_string());
            let data = field
                .bytes()
                .await
                .map_err(|e| DashboardError::BadRequest(e.to_string()))?;
            file_bytes = Some(data.to_vec());
        }
    }

    let bytes = file_bytes.ok_or(DashboardError::BadRequest("No file provided".to_string()))?;

    if bytes.len() > 2 * 1024 * 1024 {
        return Err(DashboardError::BadRequest(
            "Image must be 2MB or smaller".to_string(),
        ));
    }

    let ct = match content_type.as_deref() {
        Some("image/png") => "image/png",
        Some("image/jpeg") => "image/jpeg",
        Some("image/webp") => "image/webp",
        _ => {
            return Err(DashboardError::BadRequest(
                "Only PNG, JPEG, or WEBP images are allowed".to_string(),
            ));
        }
    };

    query("UPDATE users SET avatar_data = $1, avatar_content_type = $2 WHERE id = $3")
        .bind(&bytes)
        .bind(ct)
        .bind(user_id)
        .execute(&pool)
        .await
        .map_err(|e| DashboardError::InternalError(e.to_string()))?;

    Ok(Json(json!({ "success": true })))
}

/// GET /api/v1/me/avatar
///
/// Returns the raw image bytes with the correct Content-Type, so the
/// browser can render them directly - this is authenticated the same
/// way as every other endpoint (Bearer token), which is exactly why the
/// frontend can't just point a plain <img src="..."> at this URL; it
/// has to fetch the bytes itself with the auth header attached, then
/// hand the result to the browser as an object URL.
///
/// It confirms who's genuinely signed in, looks up their real avatar
/// data, and reports back specifically if either step fails, or if
/// this account genuinely has no avatar set at all.
pub async fn get_avatar(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, DashboardError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| DashboardError::InternalError("Failed to verify session".to_string()))?
        .ok_or(DashboardError::Unauthorized)?;

    let row = query("SELECT avatar_data, avatar_content_type FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .map_err(|e| DashboardError::InternalError(e.to_string()))?;

    let data: Option<Vec<u8>> = row.get("avatar_data");
    let content_type: Option<String> = row.get("avatar_content_type");

    let bytes = data.ok_or(DashboardError::NotFound("No avatar set".to_string()))?;
    let ct = content_type.unwrap_or_else(|| "image/png".to_string());

    Ok(([(CONTENT_TYPE, ct)], bytes))
}
