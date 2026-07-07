use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Type, prelude::FromRow};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "report_types", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ReportTypes {
    Scam,
    FakeItem,
    NoDelivery,
    WrongItem,
    Counterfeit,
    NonResponsive,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct FraudReports {
    pub id: Uuid,
    pub seller_id: Option<Uuid>,
    pub platform: Option<String>,
    pub platform_id: Option<String>,
    pub report_type: ReportTypes,
    pub description: Option<String>,
    pub listing_url: Option<String>,
    pub reported_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct FraudReportsRequest {
    pub platform: String,
    pub platform_id: Option<String>,
    pub report_type: ReportTypes,
    pub description: Option<String>,
    // The page the person was actually looking at when they clicked
    // "Report" - captured client-side as window.location.href, since
    // fraud reports are filed against a seller, not a specific listing,
    // and there's no other way to know which ad prompted the report.
    pub listing_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FraudReportsResponse {
    pub id: Uuid,
    pub seller_id: Option<Uuid>,
    pub platform: Option<String>,
    pub report_type: ReportTypes,
    pub description: Option<String>,
    pub listing_url: Option<String>,
    pub reported_at: DateTime<Utc>,
}

impl From<FraudReports> for FraudReportsResponse {
    fn from(f: FraudReports) -> Self {
        Self {
            id: f.id,
            seller_id: f.seller_id,
            platform: f.platform,
            report_type: f.report_type,
            description: f.description,
            listing_url: f.listing_url,
            reported_at: f.reported_at,
        }
    }
}
