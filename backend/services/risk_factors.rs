use crate::models::{analysis::Signal, risk_factors::RiskFactor};

/// True only for a genuinely young account/company - an age measured in
/// days, weeks or months ("This month", "3 months"). Anything with
/// "year" in it is not new, and "Unknown", "Not provided" or
/// "Invalid date" mean the age is missing, which is not the same as new.
pub fn is_new_account(account_age: &str) -> bool {
    let age = account_age.to_lowercase();
    if age.contains("year") {
        return false;
    }
    age == "this month" || age.contains("month") || age.contains("week") || age.contains("day")
}

pub fn find_signal<'a>(signals: &'a [Signal], label: &str) -> Option<&'a Signal> {
    signals.iter().find(|s| s.label == label)
}

/// A signal counts as a problem only when it is "caution" or "bad".
/// "info" (e.g. an image that was not checked, a missing founding
/// year) is neutral and must never trigger a risk factor.
fn is_flagged(signal: &Signal) -> bool {
    signal.signal_type == "caution" || signal.signal_type == "bad"
}

/// How many earlier Safely checks are needed before a high average
/// score is treated as a Serious, network-confirmed problem.
const MIN_PRIOR_CHECKS_FOR_SERIOUS: u32 = 3;

/// Reads the number of earlier checks from the Safely history signal
/// ("1 prior checks", "4 prior checks. Average risk score: 70").
/// Returns 0 when no number can be found.
fn prior_check_count(signal: &Signal) -> u32 {
    let text = format!("{} {}", signal.value, signal.sub).to_lowercase();
    let Some(pos) = text.find("prior check") else {
        return 0;
    };
    text[..pos]
        .split_whitespace()
        .last()
        .and_then(|w| w.parse::<u32>().ok())
        .unwrap_or(0)
}

/// Claude's own explanation when there is one, otherwise the fallback.
fn evidence_or(signal: &Signal, fallback: &str) -> String {
    if signal.sub.trim().is_empty() {
        fallback.to_string()
    } else {
        signal.sub.clone()
    }
}

/// Translates the now-labeled, categorized signals into real, named
/// conclusions - the actual "what does this mean" layer, sitting on
/// top of the raw "what did we find" signals.
///
/// Uses signal_type ("good"/"caution"/"bad") rather than exact value
/// text to decide severity - this is what makes these rules genuinely
/// platform-agnostic, since OLX and B2B use different literal words
/// ("Detected" vs "Not confirmed") for the same underlying bad outcome.
pub fn derive_risk_factors(signals: &[Signal]) -> Vec<RiskFactor> {
    let mut factors = Vec::new();
    let mut covered_labels: Vec<&str> = Vec::new();

    // Hard factors
    if let Some(s) = find_signal(signals, "Overall legitimacy check") {
        if is_flagged(s) {
            // A very young account on top of a legitimacy concern is a
            // well-known scam combination - named in the same factor
            // rather than as a second one.
            let young =
                find_signal(signals, "Account age").filter(|age| is_new_account(&age.value));
            let mut contributing = vec!["Overall legitimacy check".to_string()];
            let mut description = evidence_or(
                s,
                "A specific, known fraud pattern or legitimacy concern was identified in this listing.",
            );
            if let Some(age) = young {
                contributing.push("Account age".to_string());
                description = format!(
                    "{} The account is also very new ({}), a combination often seen in scam listings.",
                    description, age.value
                );
                covered_labels.push("Account age");
            }
            factors.push(RiskFactor {
                severity: "hard".to_string(),
                name: "confirmed_legitimacy_concern".to_string(),
                description,
                contributing_signals: contributing,
            });
            covered_labels.push("Overall legitimacy check");
        }
    }
    // Safely's own past scores only become a Serious flag when there is
    // a real track record (several earlier checks). A single earlier
    // scan is just Safely's own opinion from one run - it may even have
    // been a wrong result - so on its own it stays "Worth noting" at
    // most (it falls through to the soft factors below).
    if let Some(s) = find_signal(signals, "Safely history") {
        let enough_history = prior_check_count(s) >= MIN_PRIOR_CHECKS_FOR_SERIOUS;
        if s.signal_type == "bad" && enough_history {
            factors.push(RiskFactor {
                severity: "hard".to_string(),
                name: "network_confirmed_high_risk_seller".to_string(),
                description: "Safely's own network has previously scored this seller as high-risk."
                    .to_string(),
                contributing_signals: vec!["Safely history".to_string()],
            });
            covered_labels.push("Safely history");
        }
    }

    // Compound factors
    let duplicate = find_signal(signals, "Duplicate listing");
    let image_auth = find_signal(signals, "Image authenticity");
    if let (Some(d), Some(i)) = (duplicate, image_auth) {
        // Both must be real problems. An image that was simply not
        // checked (images switched off) is "info" and does not count.
        if is_flagged(d) && is_flagged(i) {
            factors.push(RiskFactor {
                severity: "compound".to_string(),
                name: "likely_counterfeit_or_nonexistent_product".to_string(),
                description: "A templated, duplicate-style listing combined with unverifiable images suggests the product itself may not genuinely exist or be authentic.".to_string(),
                contributing_signals: vec!["Duplicate listing".to_string(), "Image authenticity".to_string()],
            });
            covered_labels.push("Duplicate listing");
            covered_labels.push("Image authenticity");
        }
    }

    let urgency = find_signal(signals, "Urgency language");
    let advance_payment = find_signal(signals, "Advance payment request");
    if let (Some(u), Some(a)) = (urgency, advance_payment) {
        if is_flagged(u) && is_flagged(a) {
            factors.push(RiskFactor {
                severity: "compound".to_string(),
                name: "advance_fee_scam_pattern".to_string(),
                description: "This listing combines pressure/urgency language with a request for payment before delivery - a classic advance-fee scam pattern.".to_string(),
                contributing_signals: vec!["Urgency language".to_string(), "Advance payment request".to_string()],
            });
            covered_labels.push("Urgency language");
            covered_labels.push("Advance payment request");
        }
    }

    // Risky payment terms on their own (upfront-only payment, or
    // Western Union / MoneyGram / crypto on a B2B listing) are serious
    // even without urgency language: once paid, the money cannot be
    // recovered.
    if let Some(a) = advance_payment {
        if is_flagged(a) && !covered_labels.contains(&"Advance payment request") {
            factors.push(RiskFactor {
                severity: "hard".to_string(),
                name: "unsafe_payment_terms".to_string(),
                description: evidence_or(
                    a,
                    "The payment terms asked for would leave the buyer with no way to recover the money.",
                ),
                contributing_signals: vec!["Advance payment request".to_string()],
            });
            covered_labels.push("Advance payment request");
        }
    }

    // Soft factors - anything caution/bad-type not already covered above
    for signal in signals {
        if is_flagged(signal) && !covered_labels.contains(&signal.label.as_str()) {
            factors.push(RiskFactor {
                severity: "soft".to_string(),
                name: format!("{}_flagged", signal.label.to_lowercase().replace(' ', "_")),
                description: signal.sub.clone(),
                contributing_signals: vec![signal.label.clone()],
            });
        }
    }

    factors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(label: &str, value: &str, signal_type: &str, sub: &str) -> Signal {
        Signal {
            label: label.to_string(),
            sub: sub.to_string(),
            value: value.to_string(),
            signal_type: signal_type.to_string(),
            category: String::new(),
            check_type: String::new(),
        }
    }

    fn find<'a>(f: &'a [RiskFactor], name: &str) -> Option<&'a RiskFactor> {
        f.iter().find(|x| x.name == name)
    }

    #[test]
    fn missing_age_is_not_new() {
        assert!(!is_new_account("Unknown"));
        assert!(!is_new_account("Not provided"));
        assert!(!is_new_account("Invalid date"));
        assert!(!is_new_account("15 years 8 months"));
        assert!(is_new_account("This month"));
        assert!(is_new_account("3 months"));
    }

    #[test]
    fn western_union_alone_is_serious() {
        let s = vec![
            sig("Urgency language", "None found", "good", ""),
            sig(
                "Advance payment request",
                "Detected",
                "caution",
                "Accepts Western Union.",
            ),
        ];
        let f = derive_risk_factors(&s);
        let p = find(&f, "unsafe_payment_terms").expect("payment factor");
        assert_eq!(p.severity, "hard");
        assert_eq!(p.description, "Accepts Western Union.");
        assert!(find(&f, "advance_payment_request_flagged").is_none());
    }

    #[test]
    fn urgency_plus_payment_stays_one_compound_factor() {
        let s = vec![
            sig("Urgency language", "Detected", "caution", ""),
            sig("Advance payment request", "Detected", "caution", ""),
        ];
        let f = derive_risk_factors(&s);
        assert!(find(&f, "advance_fee_scam_pattern").is_some());
        assert!(find(&f, "unsafe_payment_terms").is_none());
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn unchecked_image_never_triggers_counterfeit() {
        let s = vec![
            sig("Duplicate listing", "Detected", "caution", "d"),
            sig("Image authenticity", "Not checked", "info", "off"),
        ];
        let f = derive_risk_factors(&s);
        assert!(find(&f, "likely_counterfeit_or_nonexistent_product").is_none());
        // the duplicate listing is still reported on its own
        assert!(find(&f, "duplicate_listing_flagged").is_some());
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn legitimacy_uses_claude_reason_and_names_young_account() {
        let s = vec![
            sig(
                "Overall legitimacy check",
                "Not confirmed",
                "caution",
                "Name does not match products.",
            ),
            sig("Account age", "This month", "caution", ""),
        ];
        let f = derive_risk_factors(&s);
        assert_eq!(f.len(), 1);
        let l = &f[0];
        assert_eq!(l.severity, "hard");
        assert!(l.description.starts_with("Name does not match products."));
        assert!(l.description.contains("very new (This month)"));
        assert_eq!(
            l.contributing_signals,
            vec!["Overall legitimacy check", "Account age"]
        );
    }

    #[test]
    fn info_signals_are_never_risk_factors() {
        let s = vec![
            sig("Platform verification", "Not offered", "info", ""),
            sig("Account age", "Not provided", "info", ""),
            sig("Image authenticity", "Not checked", "info", ""),
        ];
        assert!(derive_risk_factors(&s).is_empty());
    }

    #[test]
    fn one_prior_scan_is_never_serious() {
        let signals = vec![sig(
            "Safely history",
            "1 prior checks",
            "bad",
            "Average risk score: 67",
        )];
        let f = derive_risk_factors(&signals);
        assert!(find(&f, "network_confirmed_high_risk_seller").is_none());
        let soft = find(&f, "safely_history_flagged").expect("still worth noting");
        assert_eq!(soft.severity, "soft");
    }

    #[test]
    fn several_high_prior_scans_are_serious() {
        let signals = vec![sig(
            "Safely history",
            "4 prior checks. Average risk score: 72",
            "bad",
            "",
        )];
        let f = derive_risk_factors(&signals);
        assert_eq!(
            find(&f, "network_confirmed_high_risk_seller")
                .unwrap()
                .severity,
            "hard"
        );
    }
}
