use crate::models::analysis::Signal;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

/// Switch: when true, a seller Safely has never checked before still
/// gets a "Safely history - New to Safely" line (neutral, never counts
/// for or against the seller), so every scan shows the same set of
/// checks. Set to false to go back to leaving the line out entirely
/// on a first check.
const SHOW_FIRST_CHECK_HISTORY: bool = true;

/// Checks Safely's own memory - past evidence recorded for this exact
/// seller - and builds a signal summarizing it. This costs nothing to
/// query, needs no outside API, and gets more valuable the more times
/// Safely has seen this seller.
///
/// It looks up every past risk-score evidence row for this seller. If
/// prior scores exist, it averages them and builds a signal describing
/// what Safely already knows. If none exist, it returns a neutral
/// "New to Safely" signal (or nothing, if SHOW_FIRST_CHECK_HISTORY is
/// off) - a seller with no history is never described as "checked
/// before" when they haven't been.
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

    let scores: Vec<i16> = rows
        .iter()
        .filter_map(|(v,)| v.parse::<i16>().ok())
        .collect();

    summarize_history(&scores)
}

/// Turns the seller's earlier risk scores into the Safely history
/// signal. Kept separate from the database query so it can be tested
/// on its own.
fn summarize_history(scores: &[i16]) -> Option<Signal> {
    if scores.is_empty() {
        return if SHOW_FIRST_CHECK_HISTORY {
            Some(first_check_signal())
        } else {
            None
        };
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

    // "once" / "3 times" - plain words a buyer understands.
    let times = if count == 1 {
        "once".to_string()
    } else {
        format!("{} times", count)
    };

    Some(Signal {
        label: "Safely history".to_string(),
        sub: format!(
            "Safely has checked this seller {} before. Their average risk score was {} out of 100.",
            times, average
        ),
        value: format!("Checked {} before", times),
        signal_type: signal_type.to_string(),
        category: "reputation".to_string(),
        check_type: "existence".to_string(),
    })
}

/// No earlier checks: shown as neutral "info", so it never adds to the
/// risk score and never becomes a risk factor.
fn first_check_signal() -> Signal {
    Signal {
        label: "Safely history".to_string(),
        sub: "Safely has not checked this seller before. This does not count for or against them."
            .to_string(),
        value: "New to Safely".to_string(),
        signal_type: "info".to_string(),
        category: "reputation".to_string(),
        check_type: "existence".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_history_is_a_neutral_first_check() {
        let s = summarize_history(&[]).expect("first check is still shown");
        assert_eq!(s.label, "Safely history");
        assert_eq!(s.value, "New to Safely");
        assert_eq!(s.signal_type, "info", "must never count against the seller");
        assert!(s.sub.contains("has not checked this seller before"));
    }

    #[test]
    fn history_is_summarized_as_before() {
        let s = summarize_history(&[10, 20, 30]).unwrap();
        assert_eq!(s.value, "Checked 3 times before");
        assert_eq!(
            s.sub,
            "Safely has checked this seller 3 times before. Their average risk score was 20 out of 100."
        );
        assert_eq!(s.signal_type, "good");

        assert_eq!(summarize_history(&[67]).unwrap().signal_type, "bad");
        assert_eq!(summarize_history(&[66]).unwrap().signal_type, "caution");
        assert_eq!(summarize_history(&[34]).unwrap().signal_type, "caution");
        assert_eq!(summarize_history(&[33]).unwrap().signal_type, "good");
        let once = summarize_history(&[50]).unwrap();
        assert_eq!(once.value, "Checked once before");
        assert!(once.sub.contains("checked this seller once before"));
    }
}
