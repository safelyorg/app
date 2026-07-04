use crate::models::{analysis::RiskLevel, fraud_reports::ReportTypes, sellers::SellersResponse};
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

/// Raw shape of the first query in get_history_detail - just enough to
/// then look up the full seller separately. Not sent to the frontend
/// directly; HistoryDetailResponse below is the real API response shape.
#[derive(Debug, FromRow)]
pub struct AnalysisDetailRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub risk_score: i16,
    pub risk_level: RiskLevel,
    pub signals: serde_json::Value,
    pub listing_title: Option<String>,
    pub platform: String,
    pub seller_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct HistoryDetailResponse {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub listing_title: Option<String>,
    pub platform: String,
    pub risk_score: i16,
    pub risk_level: RiskLevel,
    pub signals: serde_json::Value,
    pub seller: SellersResponse,
    pub fraud_report_count: i64,
    pub reported: bool,
    pub report_reason: Option<ReportTypes>,
    pub report_date: Option<DateTime<Utc>>,
}
