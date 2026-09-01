use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::prelude::FromRow;
use uuid::Uuid;

#[derive(Debug, Serialize, Clone, sqlx::Type)]
#[sqlx(type_name = "evidence_type", rename_all = "lowercase")]
pub enum EvidenceType {
    Signal,
    Check,
    Outcome,
}

#[derive(Debug, FromRow, Serialize)]
pub struct Evidence {
    pub id: Uuid,
    pub analysis_id: Uuid,
    pub seller_id: Uuid,
    pub evidence_type: EvidenceType,
    pub label: String,
    pub value: String,
    pub source: String,
    pub found_at: DateTime<Utc>,
}
