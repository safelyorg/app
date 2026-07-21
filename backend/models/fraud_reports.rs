use serde::{Deserialize, Serialize};
use sqlx::Type;

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
