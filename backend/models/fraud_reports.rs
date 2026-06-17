use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, sqlx::Type)]
#[sqlx()]
pub enum ReportTypes {
    Scam,
    FakeItem,
    NoDelivery,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct FraudReports {
    pub id: Uuid,
    pub seller_id: Uuid,
    pub platform: Option<String>,
    pub platform_id: Option<Uuid>,
    pub report_type: ReportTypes,
    pub description: Option<String>,
    pub reported_at: DateTime<Utc>,
}
