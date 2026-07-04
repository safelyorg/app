use crate::models::{analysis::RiskLevel, fraud_reports::ReportTypes};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::prelude::FromRow;
use uuid::Uuid;

#[derive(Debug, Serialize, FromRow)]
pub struct HistoryItem {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub risk_score: i16,
    pub risk_level: RiskLevel,
    pub platform: String,
    pub listing_title: Option<String>,
    pub seller_name: Option<String>,
    pub seller_id: Uuid,
    pub reported: bool,
}

#[derive(Debug, Serialize, FromRow)]
pub struct ReportItem {
    pub id: Uuid,
    pub reported_at: DateTime<Utc>,
    pub report_type: ReportTypes,
    pub platform: String,
    pub seller_name: Option<String>,
    pub seller_id: Uuid,
}
