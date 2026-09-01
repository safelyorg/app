use crate::models::outcomes::{Outcome, OutcomeAction};
use sqlx::{Error, Pool, Postgres, query_as};
use uuid::Uuid;

/// Permanently records what the person actually decided to do after
/// seeing Safely's analysis - proceeded or backed out. This is the
/// real, honest feedback Layer 12 depends on, since it's the person
/// deliberately telling Safely their choice, not a guessed behavior.
pub async fn record_outcome(
    pool: &Pool<Postgres>,
    analysis_id: Uuid,
    user_id: Uuid,
    action: OutcomeAction,
) -> Result<Outcome, Error> {
    let id = Uuid::now_v7();
    let outcome = query_as::<_, Outcome>(
        "INSERT INTO outcomes (id, analysis_id, user_id, action, recorded_at)
         VALUES ($1, $2, $3, $4, NOW())
         RETURNING *",
    )
    .bind(id)
    .bind(analysis_id)
    .bind(user_id)
    .bind(action)
    .fetch_one(pool)
    .await?;

    Ok(outcome)
}
