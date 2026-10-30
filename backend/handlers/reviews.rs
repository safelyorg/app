use crate::{
    errors::reviews::ReviewError,
    models::reviews::{MyReview, PublicReview, ReviewPhoto, ReviewRequest},
    services::{
        features::auth::extract_user_id,
        features::reviews::{
            MAX_PHOTO_BYTES, detect_image_type, find_approved_photo, find_my_photo, find_my_review,
            list_approved_reviews, save_review, save_review_photo, validate_review,
        },
    },
};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

/// The signed-in user, or 401.
async fn signed_in_user(headers: &HeaderMap, pool: &Pool<Postgres>) -> Result<Uuid, ReviewError> {
    extract_user_id(headers, pool)
        .await
        .map_err(|e| ReviewError::InternalError(format!("session check failed: {}", e)))?
        .ok_or(ReviewError::Unauthorized)
}

/// POST /api/v1/reviews
///
/// Saves the signed-in user's review (one per user). It always goes to
/// "pending" and only shows on the website after it is approved.
pub async fn submit_review(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(req): Json<ReviewRequest>,
) -> Result<Json<MyReview>, ReviewError> {
    let user_id = signed_in_user(&headers, &pool).await?;
    let review = validate_review(&req).map_err(ReviewError::BadRequest)?;
    let saved = save_review(&pool, user_id, &review)
        .await
        .map_err(|e| ReviewError::InternalError(format!("save review: {}", e)))?;
    Ok(Json(saved))
}

/// POST /api/v1/reviews/mine/photo
///
/// Saves the photo for the signed-in user's review. The body is the
/// image itself (JPEG, PNG or WebP, max 1 MB). The type is read from the
/// file's bytes, not trusted from the browser.
pub async fn upload_review_photo(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ReviewError> {
    let user_id = signed_in_user(&headers, &pool).await?;

    if body.is_empty() {
        return Err(ReviewError::BadRequest(
            "Please choose a photo.".to_string(),
        ));
    }
    if body.len() > MAX_PHOTO_BYTES {
        return Err(ReviewError::BadRequest(
            "The photo is too large. Please use a smaller one.".to_string(),
        ));
    }
    let content_type = detect_image_type(&body).ok_or_else(|| {
        ReviewError::BadRequest("Please use a JPG, PNG or WebP photo.".to_string())
    })?;

    let saved = save_review_photo(&pool, user_id, &body, content_type)
        .await
        .map_err(|e| ReviewError::InternalError(format!("save review photo: {}", e)))?;
    if !saved {
        return Err(ReviewError::BadRequest(
            "Please save your review first.".to_string(),
        ));
    }
    Ok(Json(json!({ "saved": true })))
}

/// GET /api/v1/reviews/mine
///
/// The signed-in user's own review, or `{"review": null}`.
pub async fn get_my_review(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ReviewError> {
    let user_id = signed_in_user(&headers, &pool).await?;
    let review = find_my_review(&pool, user_id)
        .await
        .map_err(|e| ReviewError::InternalError(format!("find my review: {}", e)))?;
    Ok(Json(json!({ "review": review })))
}

/// GET /api/v1/reviews
///
/// Approved reviews for the website. Public, no sign-in. Never includes
/// emails or user ids.
pub async fn list_public_reviews(
    State(pool): State<Pool<Postgres>>,
) -> Result<Json<Vec<PublicReview>>, ReviewError> {
    let rows = list_approved_reviews(&pool)
        .await
        .map_err(|e| ReviewError::InternalError(format!("list reviews: {}", e)))?;
    let reviews = rows
        .into_iter()
        .map(|r| PublicReview {
            photo_url: format!("/api/v1/reviews/{}/photo", r.id),
            display_name: r.display_name,
            role: r.role,
            rating: r.rating,
            body: r.body,
        })
        .collect();
    Ok(Json(reviews))
}

/// GET /api/v1/reviews/{id}/photo
///
/// The photo of an approved review. Pending or rejected photos return 404.
pub async fn get_review_photo(
    State(pool): State<Pool<Postgres>>,
    Path(review_id): Path<i64>,
) -> Result<Response, ReviewError> {
    let photo = find_approved_photo(&pool, review_id)
        .await
        .map_err(|e| ReviewError::InternalError(format!("find review photo: {}", e)))?
        .ok_or(ReviewError::NotFound)?;

    photo_response(photo)
}

/// GET /api/v1/reviews/mine/photo
///
/// The signed-in user's own photo, so the dashboard can show it while
/// the review is still waiting for approval.
pub async fn get_my_review_photo(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Response, ReviewError> {
    let user_id = signed_in_user(&headers, &pool).await?;
    let photo = find_my_photo(&pool, user_id)
        .await
        .map_err(|e| ReviewError::InternalError(format!("find my review photo: {}", e)))?
        .ok_or(ReviewError::NotFound)?;
    photo_response(photo)
}

/// Sends a saved photo with its image type. Only the three types that
/// upload_review_photo accepts are ever served.
fn photo_response(photo: ReviewPhoto) -> Result<Response, ReviewError> {
    let content_type = match photo.photo_content_type.as_str() {
        "image/jpeg" => "image/jpeg",
        "image/png" => "image/png",
        "image/webp" => "image/webp",
        _ => return Err(ReviewError::NotFound),
    };

    let mut response = photo.photo_data.into_response();
    let response_headers = response.headers_mut();
    response_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response_headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}
