use crate::models::analysis::Signal;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

/// Checks Safely's own memory - past evidence recorded for this exact
/// seller - and, if any prior history exists, builds a real signal
/// summarizing it. This costs nothing to query, needs no outside API,
/// and gets more valuable the more times Safely has seen this seller.
///
/// It looks up every past risk-score evidence row for this seller. If
/// none exist, it returns nothing - a seller with no history
/// shouldn't be described as "checked before" when they haven't been.
/// If prior scores do exist, it averages them and builds a signal
/// describing what Safely already knows.
pub async fn build_network_memory_signal(pool: &Pool<Postgres>, seller_id: Uuid) -> Option<Signal> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT value FROM evidence
         WHERE seller_id = $1 AND evidence_type = 'check' AND label = 'risk_score'
         ORDER BY found_at DESC",
    )
    .bind(seller_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    if rows.is_empty() {
        return None;
    }

    let scores: Vec<i16> = rows
        .iter()
        .filter_map(|(v,)| v.parse::<i16>().ok())
        .collect();

    if scores.is_empty() {
        return None;
    }

    let count = scores.len();
    let average = scores.iter().map(|&s| s as i32).sum::<i32>() / count as i32;

    let signal_type = if average > 66 {
        "bad"
    } else if average >= 34 {
        "caution"
    } else {
        "good"
    };

    let checked_phrase = if count == 1 {
        "1 time".to_string()
    } else {
        format!("{} times", count)
    };

    Some(Signal {
        label: "Safely history".to_string(),
        sub: format!(
            "This seller has been checked by Safely {} before. Average risk score: {}.",
            checked_phrase, average
        ),
        value: format!("{} prior checks", count),
        signal_type: signal_type.to_string(),
        category: "reputation".to_string(),
        check_type: "existence".to_string(),
    })
}
