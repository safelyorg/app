//! What a scan result shows on each plan.
//!
//! - Team and Enterprise (and their invited members): the full result.
//! - Free: the score, the verdict, how many checks need a look, and the
//!   top 3 reasons - the same 3 rows the extension's Risk tab shows under
//!   "What we found". Every other check keeps only its name and colour
//!   (the counts stay right), without its value or explanation. The
//!   online presence links are left out. The Details tab, the full
//!   dashboard view and the PDF are for paid plans.
//!
//! Every scan is still run and saved in full, so the score is the same
//! on every plan and an old scan shows in full as soon as someone pays.
//! The cut is made here on the server, so the hidden part never reaches
//! the browser.

use crate::models::{analysis::AnalyzeResponse, risk_factors::RiskFactor};
use crate::services::features::billing::is_unlimited_user;
use crate::services::features::team::plan_holder;
use serde_json::Value;
use sqlx::{Pool, Postgres, query_scalar};
use uuid::Uuid;

/// How many reasons a Free plan sees.
pub const FREE_REASONS: usize = 3;

/// Severity order of the risk factors, most serious first (the same as
/// the extension's risk.ts).
const SEVERITY_ORDER: [&str; 3] = ["hard", "compound", "soft"];

/// The factor that only says Safely could check too little. It changes
/// the score card, so it is always kept.
const NOT_ENOUGH_INFORMATION: &str = "not_enough_information";

/// True when this person sees full results: their plan (or the plan of
/// the team they are in) is paid, or they are on the unlimited list.
pub async fn sees_full_results(pool: &Pool<Postgres>, user_id: Uuid) -> bool {
    if is_unlimited_user(pool, user_id).await {
        return true;
    }
    let holder = plan_holder(pool, user_id).await;
    query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM subscriptions WHERE user_id = $1 AND status = 'active')",
    )
    .bind(holder)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

fn is_flagged(signal_type: &str) -> bool {
    signal_type != "good" && signal_type != "info"
}

fn severity_rank(severity: &str) -> usize {
    SEVERITY_ORDER
        .iter()
        .position(|x| *x == severity)
        .unwrap_or(SEVERITY_ORDER.len())
}

/// Which risk factors the Free plan keeps, in the order to show them:
/// the most serious ones (at most FREE_REASONS), then the "not enough
/// information" one. Each factor is (severity, name).
pub fn free_factors(factors: &[(&str, &str)]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..factors.len()).collect();
    order.sort_by_key(|&i| severity_rank(factors[i].0));
    let (always, rest): (Vec<usize>, Vec<usize>) = order
        .into_iter()
        .partition(|&i| factors[i].1 == NOT_ENOUGH_INFORMATION);
    rest.into_iter().take(FREE_REASONS).chain(always).collect()
}

/// Which checks the Free plan sees in full, given each check's type and
/// how many risk factors are shown. These are the rows the Risk tab's
/// "What we found" shows: the risk factors, or without them the checks
/// that need a look; then checks that passed fill the rest.
pub fn free_signals(types: &[&str], factors_shown: usize) -> Vec<usize> {
    let mut keep: Vec<usize> = Vec::new();
    if factors_shown == 0 {
        keep.extend(
            types
                .iter()
                .enumerate()
                .filter(|(_, t)| is_flagged(t))
                .map(|(i, _)| i)
                .take(FREE_REASONS),
        );
    }
    let room = FREE_REASONS.saturating_sub(factors_shown.max(keep.len()));
    keep.extend(
        types
            .iter()
            .enumerate()
            .filter(|(_, t)| **t == "good")
            .map(|(i, _)| i)
            .take(room),
    );
    keep
}

/// Cuts a saved result (JSON, as the dashboard reads it) down to what
/// the Free plan shows: the kept risk factors, every check with only its
/// name and colour except the ones shown in full, and no online
/// presence links.
pub fn trim_to_free(signals: &mut Value, risk_factors: &mut Value, social: &mut Value) {
    let mut factors_shown = 0;
    if let Some(factors) = risk_factors.as_array_mut() {
        let keys: Vec<(String, String)> = factors
            .iter()
            .map(|f| {
                (
                    f["severity"].as_str().unwrap_or("").to_string(),
                    f["name"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect();
        let key_refs: Vec<(&str, &str)> =
            keys.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let keep = free_factors(&key_refs);
        factors_shown = keep
            .iter()
            .filter(|&&i| keys[i].1 != NOT_ENOUGH_INFORMATION)
            .count();
        let kept: Vec<Value> = keep.iter().map(|&i| factors[i].clone()).collect();
        *factors = kept;
    }
    if let Some(list) = signals.as_array_mut() {
        let types: Vec<String> = list
            .iter()
            .map(|s| s["type"].as_str().unwrap_or("").to_string())
            .collect();
        let type_refs: Vec<&str> = types.iter().map(String::as_str).collect();
        let keep = free_signals(&type_refs, factors_shown);
        for (i, signal) in list.iter_mut().enumerate() {
            if keep.contains(&i) {
                continue;
            }
            if let Some(obj) = signal.as_object_mut() {
                obj.insert("sub".to_string(), Value::String(String::new()));
                obj.insert("value".to_string(), Value::String(String::new()));
            }
        }
    }
    *social = Value::Array(Vec::new());
}

/// The same cut for a new scan's answer (the extension).
pub fn trim_response_to_free(response: &mut AnalyzeResponse) {
    let keys: Vec<(String, String)> = response
        .risk_factors
        .iter()
        .map(|f| (f.severity.clone(), f.name.clone()))
        .collect();
    let key_refs: Vec<(&str, &str)> = keys.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let keep = free_factors(&key_refs);
    let factors_shown = keep
        .iter()
        .filter(|&&i| keys[i].1 != NOT_ENOUGH_INFORMATION)
        .count();
    let mut all: Vec<Option<RiskFactor>> = response.risk_factors.drain(..).map(Some).collect();
    response.risk_factors = keep.iter().filter_map(|&i| all[i].take()).collect();

    let types: Vec<String> = response
        .signals
        .iter()
        .map(|s| s.signal_type.clone())
        .collect();
    let type_refs: Vec<&str> = types.iter().map(String::as_str).collect();
    let keep = free_signals(&type_refs, factors_shown);
    for (i, signal) in response.signals.iter_mut().enumerate() {
        if !keep.contains(&i) {
            signal.sub.clear();
            signal.value.clear();
        }
    }
    response.social_candidates.clear();
    response.full_result = false;
}

/// Sets what a new scan's answer shows for this person's plan.
pub async fn apply_plan_to_response(
    pool: &Pool<Postgres>,
    user_id: Uuid,
    response: &mut AnalyzeResponse,
) {
    if sees_full_results(pool, user_id).await {
        response.full_result = true;
    } else {
        trim_response_to_free(response);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sig(label: &str, t: &str) -> Value {
        json!({ "label": label, "type": t, "value": format!("{label} value"), "sub": format!("{label} text"), "category": "c", "check_type": "k" })
    }

    fn shown(signals: &Value) -> Vec<String> {
        signals
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| !s["sub"].as_str().unwrap().is_empty())
            .map(|s| s["label"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn without_risk_factors_the_first_three_warnings_stay() {
        let mut signals = json!([
            sig("A", "good"),
            sig("B", "bad"),
            sig("C", "warning"),
            sig("D", "bad"),
            sig("E", "bad"),
            sig("F", "info"),
        ]);
        let mut factors = json!([]);
        let mut social = json!([{ "platform": "Facebook" }]);
        trim_to_free(&mut signals, &mut factors, &mut social);
        assert_eq!(shown(&signals), vec!["B", "C", "D"]);
        // Every check is still there, with its name and colour.
        let all = signals.as_array().unwrap();
        assert_eq!(all.len(), 6);
        assert_eq!(all[4]["label"], "E");
        assert_eq!(all[4]["type"], "bad");
        assert_eq!(all[4]["value"], "");
        assert_eq!(social, json!([]));
    }

    #[test]
    fn checks_that_passed_fill_the_rest() {
        let mut signals = json!([
            sig("A", "good"),
            sig("B", "bad"),
            sig("C", "good"),
            sig("D", "good")
        ]);
        let mut factors = json!([]);
        let mut social = json!([]);
        trim_to_free(&mut signals, &mut factors, &mut social);
        // Shown in their own order: the warning, then two checks that passed.
        assert_eq!(shown(&signals), vec!["A", "B", "C"]);
    }

    #[test]
    fn the_most_serious_risk_factors_stay() {
        let mut signals = json!([sig("A", "bad"), sig("B", "good"), sig("C", "good")]);
        let mut factors = json!([
            { "severity": "soft", "name": "s1", "description": "soft one", "contributing_signals": [] },
            { "severity": "hard", "name": "h1", "description": "hard one", "contributing_signals": [] },
            { "severity": "compound", "name": "c1", "description": "pattern", "contributing_signals": [] },
            { "severity": "soft", "name": "s2", "description": "soft two", "contributing_signals": [] },
            { "severity": "soft", "name": "not_enough_information", "description": "x", "contributing_signals": [] },
        ]);
        let mut social = json!([]);
        trim_to_free(&mut signals, &mut factors, &mut social);
        let names: Vec<&str> = factors
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["h1", "c1", "s1", "not_enough_information"]);
        // Three factors fill "What we found": no check is shown in full.
        assert_eq!(shown(&signals), Vec::<String>::new());
    }

    #[test]
    fn one_factor_and_two_passed_checks() {
        let mut signals = json!([
            sig("A", "bad"),
            sig("B", "good"),
            sig("C", "good"),
            sig("D", "good")
        ]);
        let mut factors = json!([{ "severity": "hard", "name": "h", "description": "d", "contributing_signals": ["A"] }]);
        let mut social = json!([]);
        trim_to_free(&mut signals, &mut factors, &mut social);
        assert_eq!(shown(&signals), vec!["B", "C"]);
    }
}
