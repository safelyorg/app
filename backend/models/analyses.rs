use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Analyses {
    pub id: Uuid,
    pub listing_id: Uuid,
    pub risk_score: i32,
    pub risk_level: String,
    pub signals: String,
    pub price_analysis: String,
    pub network_summary: String,
    pub claude_raw: String,
    pub created_at: DateTime<Utc>,
}
