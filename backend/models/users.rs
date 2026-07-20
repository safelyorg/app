use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub google_id: Option<String>,
    pub name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
pub struct MagicLink {
    pub id: Uuid,
    pub email: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct MagicLinkRequest {
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct AuthSuccessResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct GoogleCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct VerifyQuery {
    pub token: String,
}

// Shape of Google's token endpoint response
#[derive(Debug, Deserialize)]
pub struct GoogleTokenResponse {
    pub access_token: String,
}

// Shape of Google's userinfo endpoint response
#[derive(Debug, Deserialize)]
pub struct GoogleUserInfo {
    pub sub: String,
    pub email: String,
    pub name: Option<String>,
}
