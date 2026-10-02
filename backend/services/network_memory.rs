use crate::models::analysis::Signal;
use crate::services::fraud_reports::count_fraud_reports;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

/// Switch: what the "Safely history" line is based on.
/// true (default): scam reports filed by Safely users against this
/// seller - "No scam reports" / "Reported 3 times". How often the seller
/// was scanned, and the scores of those scans, are not used: scanning a
/// seller is not evidence against it.
/// false: the old line - "Checked 5 times before. Their average risk
/// score was 70" (see PAST_SCORES_COUNT_TOWARD_RISK for whether it counts).
const HISTORY_FROM_FRAUD_REPORTS: bool = true;

/// Switch: when true, a seller Safely has never checked before still
/// gets a "Safely history - New to Safely" line (neutral, never counts
/// for or against the seller), so every scan shows the same set of
/// checks. Set to false to go back to leaving the line out entirely
/// on a first check.
const SHOW_FIRST_CHECK_HISTORY: bool = true;

/// Switch: whether Safely's OWN earlier scores for a seller count toward
/// today's risk score.
/// false (default): the "Safely history" line is shown for information
/// only ("Checked 4 times before. Their average risk score was 71") and
/// never counts for or against the seller. Reason: those earlier scores
/// are Safely's own opinion, not new evidence. Counting them made every
/// repeat scan push the score up again (one high score -> history says
/// "high-risk" -> next score higher), even when the earlier scans had
/// read the page wrongly. Real fraud reports from users still count -
/// they are handled separately (fraud_reports), not by this line.
/// true: the old behaviour - a high average is a warning, and 3+ high
/// checks become a "Serious" risk factor.
const PAST_SCORES_COUNT_TOWARD_RISK: bool = false;

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
    if HISTORY_FROM_FRAUD_REPORTS {
        let reports = count_fraud_reports(pool, seller_id).await.unwrap_or(0);
        return Some(fraud_report_history(reports));
    }

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

/// The "Safely history" line from the number of scam reports Safely
/// users filed against this seller.
/// - 0 reports: neutral ("info").
/// - 1 or more: shown in red ("bad") so the buyer sees it, and listed
///   under risk factors as "Worth noting". It does not add to the score
///   itself: each report already adds +5 to the score elsewhere (up to
///   +20), so counting it here too would count the same reports twice.
fn fraud_report_history(reports: i64) -> Signal {
    let (value, sub, signal_type) = match reports {
        n if n <= 0 => (
            "No scam reports".to_string(),
            "No Safely user has reported this seller as a scam.".to_string(),
            "info",
        ),
        1 => (
            "Reported once".to_string(),
            "1 Safely user has reported this seller as a scam.".to_string(),
            "bad",
        ),
        n => (
            format!("Reported {} times", n),
            format!("{} Safely users have reported this seller as a scam.", n),
            "bad",
        ),
    };
    Signal {
        label: "Safely history".to_string(),
        sub,
        value,
        signal_type: signal_type.to_string(),
        category: "reputation".to_string(),
        check_type: "existence".to_string(),
    }
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

    let signal_type = if !PAST_SCORES_COUNT_TOWARD_RISK {
        "info"
    } else if average > 66 {
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
            "Safely has checked this seller {} before. Their average risk score was {} out of 100.{}",
            times,
            average,
            if PAST_SCORES_COUNT_TOWARD_RISK {
                ""
            } else {
                " This does not count for or against them."
            }
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
    fn history_comes_from_scam_reports() {
        let none = fraud_report_history(0);
        assert_eq!(none.label, "Safely history");
        assert_eq!(none.value, "No scam reports");
        assert_eq!(none.signal_type, "info");
        let one = fraud_report_history(1);
        assert_eq!(one.value, "Reported once");
        assert_eq!(one.sub, "1 Safely user has reported this seller as a scam.");
        assert_eq!(one.signal_type, "bad");
        let three = fraud_report_history(3);
        assert_eq!(three.value, "Reported 3 times");
        assert_eq!(
            three.sub,
            "3 Safely users have reported this seller as a scam."
        );
    }

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
        assert!(s.sub.starts_with(
            "Safely has checked this seller 3 times before. Their average risk score was 20 out of 100."
        ));
        let once = summarize_history(&[50]).unwrap();
        assert_eq!(once.value, "Checked once before");
        assert!(once.sub.contains("checked this seller once before"));
    }

    #[test]
    fn past_scores_never_count_while_the_switch_is_off() {
        if PAST_SCORES_COUNT_TOWARD_RISK {
            return;
        }
        // Even a seller Safely scored very high 4 times stays neutral.
        let s = summarize_history(&[90, 75, 67, 60]).unwrap();
        assert_eq!(s.value, "Checked 4 times before");
        assert_eq!(s.signal_type, "info", "never a warning or a risk factor");
        assert_eq!(
            s.sub,
            "Safely has checked this seller 4 times before. Their average risk score was 73 out of 100. This does not count for or against them."
        );
        assert_eq!(summarize_history(&[10]).unwrap().signal_type, "info");
    }

    #[test]
    fn with_the_switch_on_the_old_bands_apply() {
        if !PAST_SCORES_COUNT_TOWARD_RISK {
            return;
        }
        assert_eq!(summarize_history(&[67]).unwrap().signal_type, "bad");
        assert_eq!(summarize_history(&[66]).unwrap().signal_type, "caution");
        assert_eq!(summarize_history(&[34]).unwrap().signal_type, "caution");
        assert_eq!(summarize_history(&[33]).unwrap().signal_type, "good");
    }
}
