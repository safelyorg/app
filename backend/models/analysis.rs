use crate::models::{listings::ListingCategory, sellers::SellersResponse};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Type, prelude::FromRow};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "risk_level_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Caution,
    High,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Analysis {
    pub id: Uuid,
    pub listing_id: Uuid,
    pub risk_score: i16,
    pub risk_level: RiskLevel,
    pub signals: Value,
    pub price_analysis: Option<Value>,
    pub network_summary: Option<String>,
    pub claude_raw: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct AnalysisRequest {
    pub listing_id: Uuid,
    pub risk_score: i16,
    pub risk_level: RiskLevel,
    pub signals: Value,
    pub price_analysis: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct AnalyzeRequest {
    // shared
    pub platform: String,

    // listing fields
    pub seller_id: Option<Uuid>,
    pub listing_url: String,
    pub listing_id: Option<String>,
    pub title: Option<String>,
    pub price: Option<i64>,
    pub description: Option<String>,
    pub category: Option<ListingCategory>,
    pub image_urls: Option<Vec<String>>,
    pub posted_date: Option<NaiveDate>,

    // seller fields
    pub seller_platform_id: Option<String>,
    pub seller_name: Option<String>,
    pub seller_handle: Option<String>,
    pub seller_phone: Option<String>,
    pub seller_profile_url: Option<String>,
    pub seller_join_date: Option<String>,
    pub seller_location: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AnalyzeResponse {
    pub risk_score: i16,
    pub risk_level: RiskLevel,
    pub seller: SellersResponse,
    pub signals: Vec<Signal>,
    pub network_summary: String,
}

#[derive(Debug, Serialize)]
pub struct AnalysisResponse {
    pub id: Uuid,
    pub risk_score: i16,
    pub risk_level: RiskLevel,
    pub signals: Value,
    pub price_analysis: Option<Value>,
    pub network_summary: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct Signal {
    pub label: String,
    pub sub: String,
    pub value: String,
    #[serde(rename = "type")]
    pub signal_type: String,
}

impl From<Analysis> for AnalysisResponse {
    fn from(a: Analysis) -> Self {
        Self {
            id: a.id,
            risk_score: a.risk_score,
            risk_level: a.risk_level,
            signals: a.signals,
            price_analysis: a.price_analysis,
            network_summary: a.network_summary,
            created_at: a.created_at,
        }
    }
}
