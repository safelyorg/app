use crate::services::auth::extract_user_id;
use crate::services::sellers::find_seller;
use crate::{errors::fraud_reports::FraudReportError, models::fraud_reports::FraudReportsRequest};
use axum::{Json, extract::State, http::HeaderMap};
use serde_json::{Value, json};
use sqlx::{Pool, Postgres, query};
use uuid::Uuid;

/// The platform's stored name ("thomasnet", "b2bmap", "olx") from what
/// the extension sends. The extension shows the platform with capitals
/// ("ThomasNet", "B2BMap", "OLX") and has been sending that shown name,
/// while sellers are saved under the lower-case name - so the seller was
/// never found and every report failed.
fn platform_key(platform: &str) -> String {
    platform.trim().to_lowercase()
}

/// POST /api/v1/reports
///
/// It records a real, permanent fraud report against a seller, and
/// immediately updates that seller's stored verification status to
/// "reported" - not just a temporary, in-memory display change, but a
/// genuine, lasting update to their real record.
///
/// It confirms who's genuinely signed in - reporting doesn't cost real
/// money the way /analyze does, but still requires a real account, so a
/// stranger's website can't silently submit fake reports through an
/// innocent visitor's browser, and every report stays tied to a genuine
/// person. It then finds the real seller being reported, saves the
/// report itself, and updates the seller's status to reflect it.
pub async fn create_fraud_report(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(request): Json<FraudReportsRequest>,
) -> Result<Json<Value>, FraudReportError> {
    let user_id = extract_user_id(&headers, &pool)
        .await
        .map_err(|_| FraudReportError::InternalError("Failed to verify session".to_string()))?
        .ok_or(FraudReportError::Unauthorized)?;

    let platform = platform_key(&request.platform);
    let platform_id = request.platform_id.as_deref().unwrap_or("");

    let seller = find_seller(&pool, &platform, platform_id)
        .await
        .map_err(|e| FraudReportError::InternalError(e.to_string()))?
        .ok_or_else(|| {
            eprintln!(
                "Safely: report failed - no seller saved for platform \"{}\" and id \"{}\"",
                platform, platform_id
            );
            FraudReportError::NotFound("Seller not found".to_string())
        })?;

    let id = Uuid::now_v7();
    query(
        "INSERT INTO fraud_reports (
            id, seller_id, platform, platform_id,
            report_type, description, listing_url, user_id, reported_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())",
    )
    .bind(id)
    .bind(seller.id)
    .bind(&platform)
    .bind(platform_id)
    .bind(&request.report_type)
    .bind(&request.description)
    .bind(&request.listing_url)
    .bind(user_id)
    .execute(&pool)
    .await
    .map_err(|e| FraudReportError::InternalError(e.to_string()))?;

    query("UPDATE sellers SET verification = 'reported' WHERE id = $1")
        .bind(seller.id)
        .execute(&pool)
        .await
        .map_err(|e| FraudReportError::InternalError(e.to_string()))?;

    Ok(Json(json!({ "success": true })))
}

#[cfg(test)]
mod tests {
    use super::platform_key;

    #[test]
    fn shown_platform_names_become_the_stored_names() {
        for (shown, stored) in [
            ("ThomasNet", "thomasnet"),
            ("B2BMap", "b2bmap"),
            ("B2Brazil", "b2brazil"),
            ("TradeWheel", "tradewheel"),
            ("ExportHub", "exporthub"),
            ("Kompass", "kompass"),
            ("Alibaba", "alibaba"),
            ("OLX", "olx"),
            ("olx", "olx"),
        ] {
            assert_eq!(platform_key(shown), stored);
        }
    }
}
