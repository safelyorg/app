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
    pub listing_url: String,
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
    pub listing_url: Option<String>,
}

/// Raw shape of the first query in get_history_detail - just enough to
/// then look up the full seller and this listing's reports separately.
#[derive(Debug, FromRow)]
pub struct AnalysisDetailRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub risk_score: i16,
    pub risk_level: RiskLevel,
    pub signals: serde_json::Value,
    pub listing_title: Option<String>,
    pub listing_url: String,
    pub platform: String,
    pub seller_id: Uuid,
}

/// One filed report, as shown inside a single listing's detail view -
/// there can be more than one of these for the same listing, since
/// nothing stops a person from reporting the same ad twice with
/// different reasons.
#[derive(Debug, FromRow, Serialize)]
pub struct ReportSummary {
    pub report_type: ReportTypes,
    pub reported_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct HistoryDetailResponse {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub listing_title: Option<String>,
    pub listing_url: String,
    pub platform: String,
    pub risk_score: i16,
    pub risk_level: RiskLevel,
    pub signals: serde_json::Value,
    pub seller: SellersResponse,
    pub fraud_report_count: i64,
    pub reported: bool,
    // All reports filed against THIS specific listing (by this user) -
    // not the seller's reports from other listings, and not just the
    // single most recent one.
    pub reports: Vec<ReportSummary>,
}
