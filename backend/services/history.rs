use crate::models::history::{HistoryItem, ReportItem};
use sqlx::{Error, Pool, Postgres};
use uuid::Uuid;

/// Every listing this user has analyzed, most recent first. "reported"
/// tells the dashboard whether THIS user has ever reported THIS seller -
/// not whether anyone has, since that's a community-wide count already
/// shown elsewhere (fraud_report_count on the seller itself).
pub async fn get_user_history(
    pool: &Pool<Postgres>,
    user_id: Uuid,
) -> Result<Vec<HistoryItem>, Error> {
    sqlx::query_as::<_, HistoryItem>(
        "
        SELECT
            a.id,
            a.created_at,
            a.risk_score,
            a.risk_level,
            l.platform,
            l.title AS listing_title,
            s.name AS seller_name,
            s.id AS seller_id,
            EXISTS (
                SELECT 1 FROM fraud_reports fr
                WHERE fr.seller_id = s.id AND fr.user_id = a.user_id
            ) AS reported
        FROM analysis a
        JOIN listings l ON a.listing_id = l.id
        JOIN sellers s ON l.seller_id = s.id
        WHERE a.user_id = $1
        ORDER BY a.created_at DESC
        LIMIT 200
        ",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// Every fraud report this user has personally filed, most recent first.
pub async fn get_user_reports(
    pool: &Pool<Postgres>,
    user_id: Uuid,
) -> Result<Vec<ReportItem>, Error> {
    sqlx::query_as::<_, ReportItem>(
        "
        SELECT
            fr.id,
            fr.reported_at,
            fr.report_type,
            fr.platform,
            s.name AS seller_name,
            s.id AS seller_id
        FROM fraud_reports fr
        JOIN sellers s ON fr.seller_id = s.id
        WHERE fr.user_id = $1
        ORDER BY fr.reported_at DESC
        LIMIT 200
        ",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}
