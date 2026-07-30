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

// JSON text → struct = deserialize
#[derive(Debug, Deserialize)]
pub struct MagicLinkRequest {
    pub email: String,
}

// Struct → JSON text = serialize
#[derive(Debug, Serialize)]
pub struct MagicLinkAuthResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyMagicLinkToken {
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct GoogleAfterLoginQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleTokenEndpoint {
    pub access_token: String,
}

#[derive(Debug, Deserialize)]
pub struct GoogleUserInfoEndpoint {
    #[serde(rename = "sub")]
    pub id: String,
    pub email: String,
    pub name: Option<String>,
}
