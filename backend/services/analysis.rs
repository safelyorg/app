use crate::models::analysis::{Analysis, RiskLevel};
use serde_json::Value;
use sqlx::{Error, Pool, Postgres, query_as};
use uuid::Uuid;

pub struct CreateAnalysisData<'a> {
    pub pool: &'a Pool<Postgres>,
    pub listing_id: Uuid,
    pub risk_score: i16,
    pub risk_level: RiskLevel,
    pub signals: Value,
    pub network_summary: String,
    pub claude_raw: String,
    pub user_id: Uuid,
}

pub async fn create_analysis(data: CreateAnalysisData<'_>) -> Result<Analysis, Error> {
    let id = Uuid::now_v7();
    let analysis = query_as::<_, Analysis>(
        "
        INSERT INTO analysis (
            id,
            listing_id,
            risk_score,
            risk_level,
            signals,
            network_summary,
            claude_raw,
            user_id,
            created_at
        )
        VALUES (
            $1,  $2,  $3,  $4,   $5,
            $6,  $7,  $8,  NOW()
        )
        RETURNING *
        ",
    )
    .bind(id)
    .bind(&data.listing_id)
    .bind(&data.risk_score)
    .bind(&data.risk_level)
    .bind(&data.signals)
    .bind(&data.network_summary)
    .bind(&data.claude_raw)
    .bind(&data.user_id)
    .fetch_one(data.pool)
    .await?;

    Ok(analysis)
}
