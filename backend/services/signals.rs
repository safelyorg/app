use crate::{
    models::{analysis::Signal, helpers::format_account_age, sellers::Sellers},
    services::{
        claude::{ClaudeAnalysis, Finding},
        store_scrapers::StorePageResult,
        whois::WhoisResult,
    },
};

/// It takes Claude's raw analysis and turns it into a real, ordered list of
/// 7 separate signal cards each one representing one specific thing that was checked.
///
/// It starts with an empty list, adds the price signal, built directly, by hand,
/// adds several signals using a shared helper, finding_to_signal, adds the
/// account age signal, built by hand again, pushes the image authenticity signal,
/// built by hand once more and returns the complete list.
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
        category: "listing".to_string(),
        check_type: "anomaly".to_string(),
    });
    signals.push(finding_to_signal(
        "Urgency language",
        &analysis.urgency_language.evidence,
        &analysis.urgency_language,
        "communication",
        "pattern",
    ));
    signals.push(finding_to_signal(
        "Advance payment request",
        &analysis.advance_payment_request.evidence,
        &analysis.advance_payment_request,
        "communication",
        "pattern",
    ));
    signals.push(Signal {
        label: "Account age".to_string(),
        sub: "Cross-referenced with Safely records".to_string(),
        value: seller
            .join_date
            .map(|d| format_account_age(d))
            .unwrap_or_else(|| "Unknown".to_string()),
        signal_type: "info".to_string(),
        category: "marketplace".to_string(),
        check_type: "anomaly".to_string(),
    });
    signals.push(finding_to_signal(
        "Duplicate listing",
        &analysis.duplicate_listing.evidence,
        &analysis.duplicate_listing,
        "listing",
        "pattern",
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
        category: "listing".to_string(),
        check_type: "existence".to_string(),
    });
    signals.push(finding_to_signal(
        "Fraud pattern match",
        &analysis.fraud_pattern_match.evidence,
        &analysis.fraud_pattern_match,
        "behavioral",
        "pattern",
    ));
    signals.push(finding_to_signal(
        "Contact info in listing",
        &analysis.contact_info_in_listing.evidence,
        &analysis.contact_info_in_listing,
        "communication",
        "existence",
    ));
    signals
}

/// It converts one of Claude's yes/no findings (like "was urgency language detected?")
/// into a properly-formatted Signal card, ready to display.
///
/// It takes three pieces of information, builds the value field, based on whether it was found,
/// builds the signal_type field, deciding how it should visually display.
pub fn finding_to_signal(
    label: &str,
    sub: &str,
    finding: &Finding,
    category: &str,
    check_type: &str,
) -> Signal {
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
        category: category.to_string(),
        check_type: check_type.to_string(),
    }
}

/// It checks whether the extension flagged this website as a fake,
/// lookalike domain, and if so, builds a real signal card explaining
/// exactly what's wrong — otherwise, it returns nothing at all.
///
/// It matches on the domain check's status. If the domain is genuinely legitimate,
/// if the domain is suspicious — the more involved case. If anything else,
/// genuinely nothing to report.
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
            category: "website".to_string(),
            check_type: "existence".to_string(),
        }),
        Some("suspicious") => {
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
                category: "website".to_string(),
                check_type: "existence".to_string(),
            })
        }
        _ => None,
    }
}

/// Builds a real signal from a WHOIS lookup, if one was performed.
/// Genuinely new information - Layer 3's first real "Active
/// collection" signal - correctly labeled as an Existence check under
/// the Website category, matching the same taxonomy every other
/// signal already uses.
pub fn build_whois_signal(whois: Option<&WhoisResult>) -> Option<Signal> {
    let whois = whois?;

    if !whois.registered {
        return Some(Signal {
            label: "Seller website check".to_string(),
            sub:
                "The seller's mentioned website domain does not appear to be genuinely registered."
                    .to_string(),
            value: "Unregistered".to_string(),
            signal_type: "bad".to_string(),
            category: "website".to_string(),
            check_type: "existence".to_string(),
        });
    }

    let registrar_note = whois.registrar.as_deref().unwrap_or("an unknown registrar");
    let created_note = whois.created.as_deref().unwrap_or("an unknown date");

    Some(Signal {
        label: "Seller website check".to_string(),
        sub: format!(
            "This domain is registered through {}, since {}.",
            registrar_note, created_note
        ),
        value: "Registered".to_string(),
        signal_type: "good".to_string(),
        category: "website".to_string(),
        check_type: "existence".to_string(),
    })
}

/// Builds signals from OLX's verified-seller "store" data - genuinely
/// new, real trust information (member duration, product count,
/// rating) that's already sitting on the listing page itself for
/// verified sellers, at zero extra cost.
pub fn build_seller_verification_signals(
    verified: bool,
    rating: Option<f64>,
    total_products: Option<i32>,
) -> Vec<Signal> {
    let mut signals = Vec::new();
    let is_seller_center_account = verified || rating.is_some() || total_products.is_some();

    if !is_seller_center_account {
        return signals;
    }

    signals.push(Signal {
        label: "Seller verification".to_string(),
        sub: if verified {
            "This is an OLX-verified business seller account.".to_string()
        } else {
            "This seller has a dedicated store page but is not OLX-verified.".to_string()
        },
        value: if verified {
            "Verified".to_string()
        } else {
            "Unverified store".to_string()
        },
        signal_type: if verified {
            "good".to_string()
        } else {
            "caution".to_string()
        },
        category: "identity".to_string(),
        check_type: "existence".to_string(),
    });

    if let (Some(r), Some(p)) = (rating, total_products) {
        signals.push(Signal {
            label: "Seller track record".to_string(),
            sub: format!(
                "This seller has listed {} products with a {:.1} average rating.",
                p, r
            ),
            value: format!("{:.1} rating, {} listings", r, p),
            signal_type: if r < 3.0 {
                "caution".to_string()
            } else {
                "good".to_string()
            },
            category: "reputation".to_string(),
            check_type: "anomaly".to_string(),
        });
    }

    signals
}

/// Builds a real signal from a seller's own store page, once visited
/// - Tier 2's real, direct proof (or disproof) of identity
/// consistency, genuinely stronger than anything Tier 1 alone could
/// confirm.
pub fn build_store_page_signal(
    result: &StorePageResult,
    claimed_website: Option<&str>,
) -> Option<Signal> {
    let website_cross_confirmed = match (claimed_website, &result.website) {
        (Some(claimed), Some(found_on_store)) => {
            claimed.to_lowercase() == found_on_store.to_lowercase()
        }
        _ => false,
    };

    let (value, signal_type, sub) = match (result.seller_name_confirmed, website_cross_confirmed) {
        (true, true) => (
            "Fully confirmed",
            "good",
            "The seller's name AND their claimed website both genuinely appear on their own store page - a strong, independent match.".to_string(),
        ),
        (true, false) => (
            "Name confirmed only",
            "caution",
            "The seller's name appears on their store page, but their claimed website could not be independently confirmed there.".to_string(),
        ),
        (false, _) => (
            "Unconfirmed",
            "caution",
            "The seller's name could not be confirmed on their own store page.".to_string(),
        ),
    };

    Some(Signal {
        label: "Store page check".to_string(),
        sub,
        value: value.to_string(),
        signal_type: signal_type.to_string(),
        category: "identity".to_string(),
        check_type: "consistency".to_string(),
    })
}
