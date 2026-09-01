use crate::models::{analysis::Signal, risk_factors::RiskFactor};

pub fn is_new_account(account_age: &str) -> bool {
    account_age == "This month" || (!account_age.contains("year") && account_age != "Unknown")
}

pub fn find_signal<'a>(signals: &'a [Signal], label: &str) -> Option<&'a Signal> {
    signals.iter().find(|s| s.label == label)
}

/// Translates the now-labeled, categorized signals into real, named
/// conclusions - the actual "what does this mean" layer, sitting on
/// top of the raw "what did we find" signals.
///
/// It checks for hard factors first (things serious enough alone),
/// then checks for specific, known compound combinations, then treats
/// any remaining individual caution signal as a soft factor.
pub fn derive_risk_factors(signals: &[Signal]) -> Vec<RiskFactor> {
    let mut factors = Vec::new();
    let mut covered_labels: Vec<&str> = Vec::new();

    // Hard factors
    if let Some(s) = find_signal(signals, "Fraud pattern match") {
        if s.value == "Detected" {
            factors.push(RiskFactor {
                severity: "hard".to_string(),
                name: "confirmed_fraud_pattern".to_string(),
                description: "Claude identified a specific, known fraud pattern in this listing."
                    .to_string(),
                contributing_signals: vec!["Fraud pattern match".to_string()],
            });
            covered_labels.push("Fraud pattern match");
        }
    }

    if let Some(s) = find_signal(signals, "Safely history") {
        if s.signal_type == "bad" {
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
        if d.value == "Detected" && (i.value == "unverifiable" || i.value == "suspicious") {
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
        if u.value == "Detected" && a.value == "Detected" {
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

    if let (Some(age), Some(fraud)) = (
        find_signal(signals, "Account age"),
        find_signal(signals, "Fraud pattern match"),
    ) {
        if is_new_account(&age.value)
            && fraud.value == "Detected"
            && !covered_labels.contains(&"Fraud pattern match")
        {
            factors.push(RiskFactor {
                severity: "compound".to_string(),
                name: "newly_created_high_risk_account".to_string(),
                description: "A very recently created account combined with a matched fraud pattern is a strong, well-known combination seen in scam listings.".to_string(),
                contributing_signals: vec!["Account age".to_string(), "Fraud pattern match".to_string()],
            });
            covered_labels.push("Account age");
        }
    }

    // Soft factors - anything caution-type not already covered above
    for signal in signals {
        if signal.signal_type == "caution" && !covered_labels.contains(&signal.label.as_str()) {
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
