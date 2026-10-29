use sqlx::{Error, Pool, Postgres, query_as, query_scalar};
use uuid::Uuid;

/// Points a supplier's fraud reports add to the warning score: 5 per
/// report, 20 at most. Suppliers also get a minimum score from
/// report_floor.
pub fn report_points(count: i64) -> i16 {
    (count.clamp(0, 4) as i16) * 5
}

/// The lowest score a supplier (B2B) can have, from how many different
/// Safely users reported it as a scam. One report can be a mistake or a
/// dispute, so it sets no minimum (it still adds 5 points). Each report
/// after that raises the minimum step by step instead of jumping:
/// 2 reports Caution (35), 3 High (67), then +10 each, 95 at most.
/// A supplier that already scores higher for other reasons keeps the
/// higher score.
pub fn report_floor(count: i64) -> i16 {
    match count {
        i64::MIN..=1 => 0,
        2 => 35,
        3 => 67,
        4 => 77,
        5 => 87,
        _ => 95,
    }
}

/// True when this person has already reported this seller. Each person
/// can report a seller only once, so one person can't push a seller's
/// score up alone.
pub async fn already_reported(
    pool: &Pool<Postgres>,
    user_id: Uuid,
    seller_id: Uuid,
) -> Result<bool, Error> {
    query_scalar("SELECT EXISTS(SELECT 1 FROM fraud_reports WHERE user_id = $1 AND seller_id = $2)")
        .bind(user_id)
        .bind(seller_id)
        .fetch_one(pool)
        .await
}

/// It counts how many real fraud reports exist against a specific seller.
///
/// It runs a counting query and pulls the actual number out of that one-item tuple.
pub async fn count_fraud_reports(pool: &Pool<Postgres>, seller_id: Uuid) -> Result<i64, Error> {
    let count: (i64,) = query_as("SELECT COUNT(*) FROM fraud_reports WHERE seller_id = $1")
        .bind(seller_id)
        .fetch_one(pool)
        .await?;

    Ok(count.0)
}

/// It turns a plain number (how many fraud reports exist) into a real,
/// readable sentence someone can actually understand at a glance.
///
/// It checks the fraud count, and matches it against three possible cases.
pub fn build_network_summary(fraud_count: i64) -> String {
    match fraud_count {
        0 => "Clean record on Safely network. No fraud reports found.".to_string(),
        1 => "1 fraud report found on Safely network. Proceed with caution.".to_string(),
        _ => format!(
            "{} fraud reports found on Safely network. High risk seller.",
            fraud_count
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{report_floor, report_points};

    #[test]
    fn report_points_follow_the_scan_rules() {
        assert_eq!([0, 1, 2, 4, 9].map(report_points), [0, 5, 10, 20, 20]);
    }

    #[test]
    fn the_minimum_score_rises_step_by_step() {
        assert_eq!(
            [0, 1, 2, 3, 4, 5, 6, 20].map(report_floor),
            [0, 0, 35, 67, 77, 87, 95, 95]
        );
    }
}
