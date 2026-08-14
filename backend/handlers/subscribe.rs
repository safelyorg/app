use crate::{
    errors::auth::AuthError,
    services::{auth::validate_email_format, email::subscribe_to_newsletter},
};
use axum::Json;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
pub struct NewsletterSubscribeRequest {
    pub email: String,
}

/// POST /api/v1/newsletter/subscribe
pub async fn newsletter_subscribe(
    Json(req): Json<NewsletterSubscribeRequest>,
) -> Result<Json<Value>, AuthError> {
    let email = validate_email_format(&req.email).map_err(|_| AuthError::BadRequest)?;

    subscribe_to_newsletter(&email).await?;

    Ok(Json(json!({
        "success": true,
        "message": "You're subscribed! Check your inbox to confirm."
    })))
}
