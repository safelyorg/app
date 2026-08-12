use sqlx::{Error, Pool, Postgres, query_as};
use uuid::Uuid;

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
