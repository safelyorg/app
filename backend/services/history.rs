use crate::models::history::{AnalysisDetailRow, HistoryDetailResponse, HistoryItem, ReportItem};
use crate::models::sellers::{Sellers, SellersResponse};
use crate::services::fraud_reports::{build_network_summary, count_fraud_reports};
use crate::services::listings::get_monthly_visit_activity;
use sqlx::{Error, Pool, Postgres, Row};
use uuid::Uuid;

/// Every LISTING this user has analyzed, most recent first - deduplicated by
/// the ad's actual identity (its scraped listing_id, e.g. OLX's "iid-..."
/// number, falling back to the full listing_url if that wasn't captured),
/// NOT by the listings table's internal row id.
pub async fn get_user_history(
    pool: &Pool<Postgres>,
    user_id: Uuid,
) -> Result<Vec<HistoryItem>, Error> {
    sqlx::query_as::<_, HistoryItem>(
        "
        SELECT * FROM (
            SELECT DISTINCT ON (s.id, COALESCE(l.listing_id, l.listing_url))
                a.id,
                a.created_at,
                a.risk_score,
                a.risk_level,
                l.platform,
                l.title AS listing_title,
                l.listing_url,
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
            ORDER BY s.id, COALESCE(l.listing_id, l.listing_url), a.created_at DESC
        ) sub
        ORDER BY created_at DESC
        LIMIT 200
        ",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// Every fraud report this user has personally filed, most recent first.
/// listing_url comes straight off the report itself (captured at the
/// moment it was filed), not from a join to `listings` - a report isn't
/// tied to a specific listing row, only to a seller.
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
            s.id AS seller_id,
            fr.listing_url
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

/// Full detail for one specific listing analysis.
pub async fn get_history_detail(
    pool: &Pool<Postgres>,
    analysis_id: Uuid,
    user_id: Uuid,
) -> Result<Option<HistoryDetailResponse>, Error> {
    let row = sqlx::query_as::<_, AnalysisDetailRow>(
        "
        SELECT
            a.id,
            a.created_at,
            a.risk_score,
            a.risk_level,
            a.signals,
            l.title AS listing_title,
            l.listing_url,
            l.platform,
            l.seller_id
        FROM analysis a
        JOIN listings l ON a.listing_id = l.id
        WHERE a.id = $1 AND a.user_id = $2
        ",
    )
    .bind(analysis_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let seller = sqlx::query_as::<_, Sellers>("SELECT * FROM sellers WHERE id = $1")
        .bind(row.seller_id)
        .fetch_one(pool)
        .await?;

    let fraud_count = count_fraud_reports(pool, seller.id).await?;
    let network_summary = build_network_summary(fraud_count);
    let monthly_activity = get_monthly_visit_activity(pool, seller.id)
        .await
        .unwrap_or_else(|_| vec![0i32; 12]);

    let report_row = sqlx::query(
        "SELECT report_type, reported_at FROM fraud_reports
         WHERE seller_id = $1 AND user_id = $2
         ORDER BY reported_at DESC LIMIT 1",
    )
    .bind(seller.id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    let (reported, report_reason, report_date) = match report_row {
        Some(r) => (true, Some(r.get("report_type")), Some(r.get("reported_at"))),
        None => (false, None, None),
    };

    let mut seller_response = SellersResponse::from(seller);
    seller_response.network_summary = network_summary;
    seller_response.monthly_activity = monthly_activity;

    Ok(Some(HistoryDetailResponse {
        id: row.id,
        created_at: row.created_at,
        listing_title: row.listing_title,
        listing_url: row.listing_url,
        platform: row.platform,
        risk_score: row.risk_score,
        risk_level: row.risk_level,
        signals: row.signals,
        seller: seller_response,
        fraud_report_count: fraud_count,
        reported,
        report_reason,
        report_date,
    }))
}
