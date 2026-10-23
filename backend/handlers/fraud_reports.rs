use crate::models::analysis::{RiskLevel, Signal};
use crate::services::analysis::b2b_score_from_signals;
use crate::services::auth::extract_user_id;
use crate::services::b2b_scrapers::get_scraper_for_platform;
use crate::services::fraud_reports::{
    already_reported, count_fraud_reports, report_floor, report_points,
};
use crate::services::network_memory::fraud_report_history;
use crate::services::risk_factors::{derive_risk_factors, not_enough_information_factor};
use crate::services::sellers::find_seller;
use crate::{errors::fraud_reports::FraudReportError, models::fraud_reports::FraudReportsRequest};
use axum::{Json, extract::State, http::HeaderMap};
use serde_json::{Value, from_value, json, to_value};
use sqlx::{Pool, Postgres, query, query_as};
use uuid::Uuid;

/// The platform's stored name ("thomasnet", "b2bmap", "olx") from what
/// the extension sends. The extension shows the platform with capitals
/// ("ThomasNet", "B2BMap", "OLX") and has been sending that shown name,
/// while sellers are saved under the lower-case name - so the seller was
/// never found and every report failed. Spaces are removed too:
/// "Wholesale Central" is saved as "wholesalecentral".
fn platform_key(platform: &str) -> String {
    platform
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

/// The risk level for a score: the same bands as the scan (analyze.rs).
pub fn risk_level_for(score: i16) -> RiskLevel {
    match score {
        i16::MIN..=33 => RiskLevel::Low,
        34..=66 => RiskLevel::Caution,
        _ => RiskLevel::High,
    }
}

/// True for the "Safely history" line built from scam reports ("No scam
/// reports", "Reported once", "Reported 3 times"). The line built from
/// past scores ("New to Safely", "Checked before") is left alone.
fn is_report_history(signal: &Signal) -> bool {
    signal.label == "Safely history"
        && (signal.value == "No scam reports" || signal.value.starts_with("Reported"))
}

/// Puts the new report count into the "Safely history" line.
fn refresh_report_history(signals: &mut [Signal], total_reports: i64) {
    for signal in signals.iter_mut().filter(|s| is_report_history(s)) {
        *signal = fraud_report_history(total_reports);
    }
}

/// The "Company details: Couldn't be loaded" note. The scan adds it
/// after scoring (analyze.rs), so it is left out when scoring again.
fn is_company_page_note(signal: &Signal) -> bool {
    signal.label == "Company details" && signal.value == "Couldn't be loaded"
}

/// A saved scan's new score after a report.
/// - Supplier listings: scored again from the saved check results with
///   the new report count - exactly the score a new scan would give.
/// - Consumer listings (OLX): the old score plus what this one report
///   adds (the checks there come from the AI and aren't saved in a form
///   that can be scored again).
fn new_score(old_score: i16, signals: Option<&[Signal]>, total_reports: i64, is_b2b: bool) -> i16 {
    match signals {
        Some(signals) if is_b2b => {
            let scored: Vec<Signal> = signals
                .iter()
                .filter(|s| !is_company_page_note(s))
                .cloned()
                .collect();
            b2b_score_from_signals(&scored, total_reports)
        }
        _ => {
            let added =
                report_points(total_reports, is_b2b) - report_points(total_reports - 1, is_b2b);
            let floor = if is_b2b {
                report_floor(total_reports)
            } else {
                0
            };
            (old_score + added).max(floor).clamp(0, 100)
        }
    }
}

/// Updates every saved scan of this seller, from every user - score,
/// level, the "Safely history" line and the reasons - so everyone's
/// dashboard shows the new report count without scanning again.
///
/// Returns the new score and level of the reporter's latest scan of the
/// seller, or None when they have no saved scan of it.
async fn refresh_seller_scans(
    pool: &Pool<Postgres>,
    seller_id: Uuid,
    user_id: Uuid,
    total_reports: i64,
    is_b2b: bool,
) -> Result<Option<(i16, RiskLevel)>, FraudReportError> {
    let internal = |e: String| FraudReportError::InternalError(e);
    let scans: Vec<(Uuid, Option<Uuid>, i16, Value)> = query_as(
        "SELECT a.id, a.user_id, a.risk_score, a.signals
         FROM analysis a
         JOIN listings l ON a.listing_id = l.id
         WHERE l.seller_id = $1
         ORDER BY a.created_at DESC",
    )
    .bind(seller_id)
    .fetch_all(pool)
    .await
    .map_err(|e| internal(e.to_string()))?;

    let mut latest = None;
    for (id, scan_user, old_score, signals_json) in scans {
        // A scan whose saved checks can't be read keeps its checks and
        // reasons; only its score changes.
        let mut parsed: Option<Vec<Signal>> = from_value(signals_json).ok();
        // The "Safely history" line gets the new count first, so the
        // score is worked out from the same checks a new scan would see.
        if let Some(signals) = parsed.as_mut() {
            refresh_report_history(signals, total_reports);
        }
        let score = new_score(old_score, parsed.as_deref(), total_reports, is_b2b);
        let level = risk_level_for(score);

        if let Some(signals) = parsed {
            let mut factors = derive_risk_factors(&signals);
            if is_b2b {
                if let Some(note) = not_enough_information_factor(&signals) {
                    factors.push(note);
                }
            }
            let signals_json = to_value(&signals).map_err(|e| internal(e.to_string()))?;
            let factors_json = to_value(&factors).map_err(|e| internal(e.to_string()))?;
            query(
                "UPDATE analysis
                 SET risk_score = $1, risk_level = $2, signals = $3, risk_factors = $4
                 WHERE id = $5",
            )
            .bind(score)
            .bind(&level)
            .bind(signals_json)
            .bind(factors_json)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| internal(e.to_string()))?;
        } else {
            query("UPDATE analysis SET risk_score = $1, risk_level = $2 WHERE id = $3")
                .bind(score)
                .bind(&level)
                .bind(id)
                .execute(pool)
                .await
                .map_err(|e| internal(e.to_string()))?;
        }

        // Newest first, so the first scan of the reporter is their latest.
        if latest.is_none() && scan_user == Some(user_id) {
            latest = Some((score, level));
        }
    }
    Ok(latest)
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
///
/// Each person can report a seller once: a second report gets 409
/// "already_reported" (checked here, and by a database rule from
/// migration 0015 in case two reports arrive at the same moment).
///
/// It also updates every saved scan of the seller, from every user
/// (refresh_seller_scans), and returns the new score and level of the
/// latest one ("risk_score" / "risk_level", null when there is none),
/// which the extension shows instead of working it out itself.
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

    let reported = already_reported(&pool, user_id, seller.id)
        .await
        .map_err(|e| FraudReportError::InternalError(e.to_string()))?;
    if reported {
        return Err(FraudReportError::AlreadyReported);
    }

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
    .map_err(|e| match e.as_database_error() {
        // Two reports from the same person arrived at once and the
        // database rule stopped the second one.
        Some(db) if db.is_unique_violation() => FraudReportError::AlreadyReported,
        _ => FraudReportError::InternalError(e.to_string()),
    })?;

    query("UPDATE sellers SET verification = 'reported' WHERE id = $1")
        .bind(seller.id)
        .execute(&pool)
        .await
        .map_err(|e| FraudReportError::InternalError(e.to_string()))?;

    let total_reports = count_fraud_reports(&pool, seller.id)
        .await
        .map_err(|e| FraudReportError::InternalError(e.to_string()))?;
    let is_b2b = get_scraper_for_platform(&platform).is_some();
    let latest = refresh_seller_scans(&pool, seller.id, user_id, total_reports, is_b2b).await?;

    Ok(Json(match latest {
        Some((risk_score, risk_level)) => json!({
            "success": true,
            "risk_score": risk_score,
            "risk_level": risk_level,
        }),
        None => json!({ "success": true, "risk_score": null, "risk_level": null }),
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        is_report_history, new_score, platform_key, refresh_report_history, risk_level_for,
    };
    use crate::models::analysis::{RiskLevel, Signal};

    fn sig(label: &str, value: &str, signal_type: &str) -> Signal {
        Signal {
            label: label.to_string(),
            sub: String::new(),
            value: value.to_string(),
            signal_type: signal_type.to_string(),
            category: String::new(),
            check_type: String::new(),
        }
    }

    #[test]
    fn supplier_scores_follow_the_report_table() {
        // A clean supplier with only the history line.
        let signals = vec![sig("Safely history", "No scam reports", "info")];
        let score = |n| new_score(0, Some(&signals), n, true);
        // One report adds a little, never a jump.
        assert!(score(1) >= score(0) && score(1) <= score(0) + 5);
        // Then the minimum rises step by step.
        assert_eq!([2, 3, 4, 5, 6, 9].map(score), [35, 67, 77, 87, 95, 95]);
    }

    #[test]
    fn a_higher_supplier_score_is_kept() {
        let signals = vec![
            sig("Payment terms", "Western Union only", "bad"),
            sig("Safely history", "No scam reports", "info"),
        ];
        let before = new_score(0, Some(&signals), 0, true);
        assert!(new_score(0, Some(&signals), 2, true) >= before.max(35));
    }

    #[test]
    fn the_company_details_note_does_not_change_the_score() {
        let plain = vec![sig("Safely history", "No scam reports", "info")];
        let mut with_note = plain.clone();
        with_note.push(sig("Company details", "Couldn't be loaded", "info"));
        assert_eq!(
            new_score(0, Some(&plain), 1, true),
            new_score(0, Some(&with_note), 1, true)
        );
    }

    #[test]
    fn consumer_scores_add_only_this_reports_share() {
        // OLX: 0 / 20 / 35 / 50 - the second report adds 15.
        assert_eq!(new_score(40, None, 1, false), 60);
        assert_eq!(new_score(60, None, 2, false), 75);
        assert_eq!(new_score(90, None, 3, false), 100);
    }

    #[test]
    fn unreadable_supplier_checks_still_reach_the_minimum() {
        assert_eq!(new_score(20, None, 2, true), 35);
        assert_eq!(new_score(20, None, 1, true), 25);
    }

    #[test]
    fn the_history_line_shows_the_new_count() {
        let mut signals = vec![
            sig("Safely history", "Reported once", "bad"),
            sig("Founded", "2010", "good"),
        ];
        refresh_report_history(&mut signals, 3);
        assert_eq!(signals[0].value, "Reported 3 times");
        assert_eq!(signals[1].value, "2010");
        // The past-scores version of the line is left alone.
        assert!(!is_report_history(&sig(
            "Safely history",
            "New to Safely",
            "info"
        )));
    }

    #[test]
    fn risk_levels_use_the_scan_bands() {
        assert!(matches!(risk_level_for(33), RiskLevel::Low));
        assert!(matches!(risk_level_for(34), RiskLevel::Caution));
        assert!(matches!(risk_level_for(66), RiskLevel::Caution));
        assert!(matches!(risk_level_for(67), RiskLevel::High));
    }

    #[test]
    fn shown_platform_names_become_the_stored_names() {
        for (shown, stored) in [
            ("ThomasNet", "thomasnet"),
            ("B2BMap", "b2bmap"),
            ("B2Brazil", "b2brazil"),
            ("TradeWheel", "tradewheel"),
            ("ExportHub", "exporthub"),
            ("Kompass", "kompass"),
            ("Wholesale Central", "wholesalecentral"),
            (" wholesalecentral ", "wholesalecentral"),
            ("Alibaba", "alibaba"),
            ("OLX", "olx"),
            ("olx", "olx"),
        ] {
            assert_eq!(platform_key(shown), stored);
        }
    }
}
