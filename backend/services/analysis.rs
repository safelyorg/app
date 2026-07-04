use crate::models::analysis::{Analysis, RiskLevel};
use sqlx::{Error, Pool, Postgres};
use uuid::Uuid;

pub async fn create_analysis(
    pool: &Pool<Postgres>,
    listing_id: Uuid,
    risk_score: i16,
    risk_level: RiskLevel,
    signals: serde_json::Value,
    network_summary: String,
    claude_raw: String,
    user_id: Option<Uuid>,
) -> Result<Analysis, Error> {
    let id = Uuid::now_v7();
    let analysis = sqlx::query_as::<_, Analysis>(
        "
        INSERT INTO analysis (
            id,
            listing_id,
            risk_score,
            risk_level,
            signals,
            price_analysis,
            network_summary,
            claude_raw,
            user_id,
            created_at
        )
        VALUES (
            $1,  $2,  $3,  $4,   $5,
            $6,  $7,  $8,  $9,   NOW()
        )
        RETURNING *
        ",
    )
    .bind(id)
    .bind(&listing_id)
    .bind(&risk_score)
    .bind(&risk_level)
    .bind(&signals)
    .bind(None::<serde_json::Value>)
    .bind(&network_summary)
    .bind(&claude_raw)
    .bind(&user_id)
    .fetch_one(pool)
    .await?;
    Ok(analysis)
}
