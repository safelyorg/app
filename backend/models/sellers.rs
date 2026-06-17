use super::helpers::{format_account_age, format_last_active};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Sellers {
    pub id: Uuid,
    pub platform: String,
    pub platform_id: String,
    pub name: Option<String>,
    pub handle: String,
    pub phone: Option<String>,
    pub profile_url: Option<String>,
    pub join_date: Option<NaiveDate>,
    pub verification: String,
    pub total_deals: i32,
    pub disputes: i32,
    pub completion_rate: Option<i64>,
    pub location: Option<String>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct SellersRequest {
    pub platform: String,
    pub platform_id: Option<String>,
    pub name: Option<String>,
    pub phone: Option<String>,
    pub profile_url: Option<String>,
    pub join_date: Option<String>, // raw text like "Member since 2021" — backend parses this
}

#[derive(Debug, Serialize)]
pub struct SellersResponse {
    pub id: Uuid,
    pub name: Option<String>,
    pub handle: String,
    pub account_age: String,
    pub verification: String, // "verified", "flagged", "unknown"
    pub total_deals: i32,
    pub disputes: i32,
    pub completion_rate: String,
    pub location: Option<String>,
    pub last_active: Option<String>, // "2 hours ago" — already formatted
    pub network_summary: String,     // "Clean record on Cover network..."
    pub platforms: Vec<PlatformResponse>,
    pub monthly_activity: Vec<i32>, // 12 numbers for the bar chart
}

#[derive(Debug, Serialize)]
pub struct PlatformResponse {
    pub name: String,
    pub status: String,        // "Active · 3 yr", "Not found"
    pub platform_type: String, // "active", "none"
}

impl From<Sellers> for SellersResponse {
    fn from(s: Sellers) -> Self {
        Self {
            id: s.id,
            name: s.name,
            handle: s.handle,
            account_age: s
                .join_date
                .map(|d| format_account_age(d))
                .unwrap_or_else(|| "unknown".to_string()),
            verification: s.verification,
            total_deals: s.total_deals,
            disputes: s.disputes,
            completion_rate: s
                .completion_rate
                .map(|c| format!("{}%", c))
                .unwrap_or_else(|| "N/A".to_string()),
            location: s.location,
            last_active: s.last_seen_at.map(|t| format_last_active(t)),
            network_summary: String::new(),
            platforms: vec![],
            monthly_activity: vec![],
        }
    }
}
