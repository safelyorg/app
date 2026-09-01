use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::prelude::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::Type, Deserialize)]
#[sqlx(type_name = "outcome_action", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum OutcomeAction {
    Proceeded,
    Aborted,
}

#[derive(Debug, Deserialize)]
pub struct OutcomeRequest {
    pub analysis_id: Uuid,
    pub action: OutcomeAction,
}

#[derive(Debug, FromRow)]
pub struct Outcome {
    pub id: Uuid,
    pub analysis_id: Uuid,
    pub user_id: Uuid,
    pub action: OutcomeAction,
    pub recorded_at: DateTime<Utc>,
}
