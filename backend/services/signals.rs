use crate::{
    models::{analysis::Signal, helpers::format_account_age, sellers::Sellers},
    services::claude::{ClaudeAnalysis, Finding},
};
pub fn finding_to_signal(label: &str, sub: &str, finding: &Finding) -> Signal {
    Signal {
        label: label.to_string(),
        sub: sub.to_string(),
        value: if finding.found {
            "Detected".to_string()
        } else {
            "None found".to_string()
        },
        signal_type: if finding.found {
            "caution".to_string()
        } else {
            "good".to_string()
        },
    }
}

/// Turns the extension's client-side domain-lookalike check into the
/// same Signal shape as every other fraud signal, so it renders through
/// the exact same Risk/Intelligence rendering code already in place -
/// no separate UI, no separate table, just one more entry in the list
/// that already exists. Returns None when there's nothing to report
/// (the extension didn't send domain check data, or the domain wasn't
/// close enough to any protected marketplace to be worth mentioning
/// either way).
pub fn build_domain_signal(
    status: Option<&str>,
    real_name: Option<&str>,
    real_domain: Option<&str>,
    current_domain: Option<&str>,
    current_domain_html: Option<&str>,
    real_domain_html: Option<&str>,
) -> Option<Signal> {
    match status {
        Some("legitimate") => Some(Signal {
            label: "Domain check".to_string(),
            sub: format!(
                "This matches {}'s real, verified domain.",
                real_name.unwrap_or("the marketplace")
            ),
            value: "Verified".to_string(),
            signal_type: "good".to_string(),
        }),
        Some("suspicious") => {
            // Prefer the character-highlighted versions when the
            // extension provided them - the specific differing letter
            // (e.g. "l" swapped for "I", or "0" for "o") is marked
            // directly, since those characters are deliberately designed
            // to look near-identical in plain text otherwise. Falls back
            // to the plain domain strings if highlighting wasn't sent.
            let current_display = current_domain_html
                .or(current_domain)
                .unwrap_or("an unrecognized domain");
            let real_display = real_domain_html.or(real_domain).unwrap_or("unknown");

            Some(Signal {
                label: "Domain check".to_string(),
                sub: format!(
                    "This does not match {}'s real domain ({}). You're currently on {} instead.",
                    real_name.unwrap_or("the marketplace"),
                    real_display,
                    current_display,
                ),
                value: "Suspicious".to_string(),
                signal_type: "bad".to_string(),
            })
        }
        _ => None,
    }
}

pub fn build_signals(analysis: &ClaudeAnalysis, seller: &Sellers) -> Vec<Signal> {
    let mut signals = Vec::new();
    signals.push(Signal {
        label: "Price analysis".to_string(),
        sub: analysis.price_assessment.reasoning.clone(),
        value: analysis.price_assessment.verdict.clone(),
        signal_type: if analysis.price_assessment.verdict == "normal" {
            "good".to_string()
        } else {
            "caution".to_string()
        },
    });
    signals.push(finding_to_signal(
        "Urgency language",
        &analysis.urgency_language.evidence,
        &analysis.urgency_language,
    ));
    signals.push(finding_to_signal(
        "Advance payment request",
        &analysis.advance_payment_request.evidence,
        &analysis.advance_payment_request,
    ));
    signals.push(Signal {
        label: "Account age".to_string(),
        sub: "Cross-referenced with Safely records".to_string(),
        value: seller
            .join_date
            .map(|d| format_account_age(d))
            .unwrap_or_else(|| "Unknown".to_string()),
        signal_type: "info".to_string(),
    });
    signals.push(finding_to_signal(
        "Duplicate listing",
        &analysis.duplicate_listing.evidence,
        &analysis.duplicate_listing,
    ));
    signals.push(Signal {
        label: "Image authenticity".to_string(),
        sub: analysis.image_authenticity.reasoning.clone(),
        value: analysis.image_authenticity.verdict.clone(),
        signal_type: if analysis.image_authenticity.verdict == "original" {
            "good".to_string()
        } else {
            "caution".to_string()
        },
    });
    signals.push(finding_to_signal(
        "Fraud pattern match",
        &analysis.fraud_pattern_match.evidence,
        &analysis.fraud_pattern_match,
    ));
    signals.push(finding_to_signal(
        "Contact info in listing",
        &analysis.contact_info_in_listing.evidence,
        &analysis.contact_info_in_listing,
    ));
    signals
}
