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
///
/// It signs someone up for the Safely newsletter, using nothing but a
/// real email address - no account or sign-in required, since this is
/// deliberately open to anyone, not just existing Safely users.
///
/// It cleans up and validates the email address, hands it off to Kit
/// to actually create the subscription, and reports back a friendly
/// confirmation message - or the specific reason it failed, if the
/// email was invalid or the subscription itself couldn't be created.
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
