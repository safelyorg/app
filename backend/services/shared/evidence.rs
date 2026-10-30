use crate::models::{analysis::Signal, risk_factors::RiskFactor};
use sqlx::{Pool, Postgres, query};
use uuid::Uuid;

/// Writes a permanent, append-only record of every signal that fired
/// for this analysis, plus the final risk score itself - this is the
/// real, defensible "what did Safely actually find, and when" trail.
///
/// It never blocks or fails the real analysis response if writing
/// evidence itself has a problem - evidence recording is a genuine
/// side effect, not something a user should ever see break their scan.
///
/// It writes one evidence row per signal that was found, then writes
/// one more row recording the final score itself, and logs (rather
/// than propagates) any real database error along the way.
pub async fn record_evidence(
    pool: &Pool<Postgres>,
    analysis_id: Uuid,
    seller_id: Uuid,
    signals: &[Signal],
    risk_score: i16,
) {
    for signal in signals {
        let result = query(
            "INSERT INTO evidence (id, analysis_id, seller_id, evidence_type, label, value, source, found_at)
             VALUES ($1, $2, $3, 'signal', $4, $5, $6, NOW())",
        )
        .bind(Uuid::now_v7())
        .bind(analysis_id)
        .bind(seller_id)
        .bind(&signal.label)
        .bind(&signal.value)
        .bind("signal_pipeline")
        .execute(pool)
        .await;

        if let Err(e) = result {
            eprintln!("Safely: failed to write signal evidence: {}", e);
        }
    }

    let score_result = query(
        "INSERT INTO evidence (id, analysis_id, seller_id, evidence_type, label, value, source, found_at)
         VALUES ($1, $2, $3, 'check', 'risk_score', $4, $5, NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(analysis_id)
    .bind(seller_id)
    .bind(risk_score.to_string())
    .bind("scoring_engine")
    .execute(pool)
    .await;

    if let Err(e) = score_result {
        eprintln!("Safely: failed to write score evidence: {}", e);
    }
}

/// Permanently records the named risk-factor conclusions derived from
/// this analysis's signals - Layer 7's real output, stored the same
/// permanent, append-only way as everything else in this table.
pub async fn record_risk_factors(
    pool: &Pool<Postgres>,
    analysis_id: Uuid,
    seller_id: Uuid,
    risk_factors: &[RiskFactor],
) {
    for factor in risk_factors {
        let result = sqlx::query(
            "INSERT INTO evidence (id, analysis_id, seller_id, evidence_type, label, value, source, found_at)
             VALUES ($1, $2, $3, 'check', $4, $5, $6, NOW())",
        )
        .bind(Uuid::now_v7())
        .bind(analysis_id)
        .bind(seller_id)
        .bind(format!("risk_factor:{}", factor.severity))
        .bind(&factor.name)
        .bind("risk_factor_engine")
        .execute(pool)
        .await;

        if let Err(e) = result {
            eprintln!("Safely: failed to write risk factor evidence: {}", e);
        }
    }
}
