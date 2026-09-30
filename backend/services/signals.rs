use crate::{
    models::{analysis::Signal, helpers::format_account_age, sellers::Sellers},
    services::{
        b2b_scrapers::{B2bListingProfile, B2bSupplierProfile},
        b2c_scrapers::B2cProfileResult,
        claude::{
            B2bClaudeAnalysis, ClaudeAnalysis, Finding, IMAGE_ANALYSIS_ENABLED, ImageAssessment,
        },
        whois::WhoisResult,
    },
};
use chrono::{Datelike, NaiveDate, Utc};

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

    signals.push(image_authenticity_signal(&analysis.image_authenticity));

    signals.push(finding_to_signal(
        "Overall legitimacy check",
        &analysis.fraud_pattern_match.evidence,
        &analysis.fraud_pattern_match,
        "behavioral",
        "pattern",
    ));

    signals.push(Signal {
        label: "Contact info".to_string(),
        sub: analysis.contact_info_in_listing.evidence.clone(),
        value: if analysis.contact_info_in_listing.found {
            "Confirmed".to_string()
        } else {
            "Not confirmed".to_string()
        },
        signal_type: if analysis.contact_info_in_listing.found {
            "caution".to_string()
        } else {
            "good".to_string()
        },
        category: "communication".to_string(),
        check_type: "existence".to_string(),
    });

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

    signals.push(Signal {
        label: "Platform verification".to_string(),
        sub: if verified {
            "This seller is verified on the platform.".to_string()
        } else {
            "This seller is not verified on the platform.".to_string()
        },
        value: if verified {
            "Verified".to_string()
        } else {
            "Not verified".to_string()
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
    result: &B2cProfileResult,
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

/// Builds a signal from B2Brazil's own "Verified company" badge - a
/// real, direct trust indicator, since the platform's own disclaimer
/// text says an unverified company's info isn't guaranteed accurate.
pub fn build_b2b_verification_signal(supplier: &B2bSupplierProfile) -> Signal {
    let name = supplier.company_name.as_deref().unwrap_or("This company");

    // ExportHub has no company verification at all - its seals
    // ("Standard Membership", "Free Member") are paid tiers. Every
    // ExportHub seller would otherwise get the same "Unverified"
    // caution, which says nothing about this particular seller.
    if supplier.source_platform == "exporthub" {
        let tier = supplier
            .badge_honorific
            .as_deref()
            .map(|t| {
                let kind = if t.to_lowercase().contains("free") {
                    "ExportHub's free membership level"
                } else {
                    "a paid membership level"
                };
                format!(
                    " {} shows \"{}\", which is {}, not a check on the company.",
                    name, t, kind
                )
            })
            .unwrap_or_default();
        return Signal {
            label: "Platform verification".to_string(),
            sub: format!("ExportHub does not verify companies.{}", tier),
            value: "Not offered".to_string(),
            signal_type: "info".to_string(),
            category: "identity".to_string(),
            check_type: "existence".to_string(),
        };
    }

    if supplier.platform_verified_badge {
        Signal {
            label: "Platform verification".to_string(),
            sub: format!(
                "{} has a verified badge on {}.",
                supplier.company_name.as_deref().unwrap_or("This company"),
                supplier.source_platform
            ),
            value: "Verified".to_string(),
            signal_type: "good".to_string(),
            category: "identity".to_string(),
            check_type: "existence".to_string(),
        }
    } else {
        Signal {
            label: "Platform verification".to_string(),
            // The "not guaranteed accurate" wording is B2Brazil's own
            // disclaimer, so it is only quoted for B2Brazil.
            sub: if supplier.source_platform == "b2brazil" {
                format!(
                    "{} does not have a verified badge - B2Brazil itself states unverified company info is not guaranteed accurate.",
                    name
                )
            } else {
                format!(
                    "{} does not have a verified badge on {}.",
                    name, supplier.source_platform
                )
            },
            value: "Unverified".to_string(),
            signal_type: "caution".to_string(),
            category: "identity".to_string(),
            check_type: "existence".to_string(),
        }
    }
}

/// Checks how long ago this company claims to have been established -
/// a genuinely new company is not inherently fraudulent, but it's a
/// real, honest anomaly worth noting, the same logic already applied
/// to OLX account age.
pub fn build_b2b_company_age_signal(supplier: &B2bSupplierProfile) -> Signal {
    let Some(year_str) = supplier.year_established.as_deref() else {
        return Signal {
            label: "Account age".to_string(),
            sub: "No founding year was provided by this company.".to_string(),
            value: "Not provided".to_string(),
            signal_type: "info".to_string(),
            category: "company".to_string(),
            check_type: "anomaly".to_string(),
        };
    };
    let Ok(established_year) = year_str.trim().parse::<i32>() else {
        return Signal {
            label: "Account age".to_string(),
            sub: format!(
                "The stated founding year ('{}') could not be understood.",
                year_str
            ),
            value: "Invalid date".to_string(),
            signal_type: "caution".to_string(),
            category: "company".to_string(),
            check_type: "anomaly".to_string(),
        };
    };

    let current_year = Utc::now().year();
    if established_year > current_year {
        return Signal {
            label: "Account age".to_string(),
            sub: format!(
                "The stated founding year ('{}') is in the future.",
                established_year
            ),
            value: "Invalid date".to_string(),
            signal_type: "caution".to_string(),
            category: "company".to_string(),
            check_type: "anomaly".to_string(),
        };
    }

    let established_date = NaiveDate::from_ymd_opt(established_year, 1, 1)
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(current_year, 1, 1).unwrap());
    let value = format_account_age(established_date);
    let age_years = current_year - established_year;
    let signal_type = if age_years <= 1 { "caution" } else { "good" };
    Signal {
        label: "Account age".to_string(),
        sub: format!(
            "This company states it was established in {}.",
            established_year
        ),
        value,
        signal_type: signal_type.to_string(),
        category: "company".to_string(),
        check_type: "anomaly".to_string(),
    }
}

/// Checks whether the company has genuinely filled in transparency
/// fields (employee count, sales revenue, export percentage). Missing
/// these isn't inherently suspicious - many legitimate businesses
/// leave them blank for privacy - so this stays a mild, informational
/// signal, not a harsh one.
pub fn build_b2b_transparency_signal(supplier: &B2bSupplierProfile) -> Signal {
    let fields: [(&str, bool); 3] = [
        ("Employee count", supplier.employee_count.is_some()),
        ("Sales revenue", supplier.sales_revenue.is_some()),
        ("Export percentage", supplier.export_percentage.is_some()),
    ];
    let filled_count = fields.iter().filter(|(_, present)| *present).count();
    let signal_type = if filled_count >= 2 { "good" } else { "info" };

    let checklist = fields
        .iter()
        .map(|(name, present)| format!("{}|{}", name, present))
        .collect::<Vec<_>>()
        .join(";");

    Signal {
        label: "Company profile completeness".to_string(),
        sub: format!(
            "{} of 3 transparency fields (employees, sales volume, export percentage) are filled in.###CHECKLIST###{}",
            filled_count, checklist
        ),
        value: format!("{}/3 fields provided", filled_count),
        signal_type: signal_type.to_string(),
        category: "company".to_string(),
        check_type: "existence".to_string(),
    }
}

/// Checks how many of the real, listing-specific fields (price, MOQ,
/// Incoterms, etc.) were genuinely filled in versus left as "Not
/// informed." Incomplete listings are common in B2B and not
/// inherently suspicious, so this stays a mild pattern check, not a
/// harsh red flag.
pub fn build_b2b_listing_completeness_signal(listing: &B2bListingProfile) -> Signal {
    let fields: [(&str, &Option<String>); 9] = [
        ("Unit price", &listing.unit_price),
        ("FOB price", &listing.fob_price),
        ("Minimum order quantity", &listing.minimum_order_quantity),
        ("Payment type", &listing.payment_type),
        ("Preferred port", &listing.preferred_port),
        ("Production capacity", &listing.production_capacity),
        ("Delivery timeframe", &listing.delivery_timeframe),
        ("Incoterms", &listing.incoterms),
        ("Packaging details", &listing.packaging_details),
    ];
    let filled_count = fields.iter().filter(|(_, f)| f.is_some()).count();
    let total = fields.len();
    let signal_type = if filled_count == 0 { "caution" } else { "info" };

    // Real, delimited checklist the frontend parses to build the
    // dropdown - "Field Name|true" or "Field Name|false" per line,
    // same, simple encoding style used elsewhere in this file.
    let checklist = fields
        .iter()
        .map(|(name, value)| format!("{}|{}", name, value.is_some()))
        .collect::<Vec<_>>()
        .join(";");

    Signal {
        label: "Listing completeness".to_string(),
        sub: format!(
            "{} of {} listing details (price, MOQ, Incoterms, etc.) were provided by the supplier.###CHECKLIST###{}",
            filled_count, total, checklist
        ),
        value: format!("{}/{} fields provided", filled_count, total),
        signal_type: signal_type.to_string(),
        category: "listing".to_string(),
        check_type: "pattern".to_string(),
    }
}

fn b2b_finding_to_signal(
    label: &str,
    category: &str,
    check_type: &str,
    finding: &Finding,
) -> Signal {
    Signal {
        label: label.to_string(),
        sub: finding.evidence.clone(),
        value: if finding.found {
            "Confirmed".to_string()
        } else {
            "Not confirmed".to_string()
        },
        signal_type: if finding.found {
            "good".to_string()
        } else {
            "caution".to_string()
        },
        category: category.to_string(),
        check_type: check_type.to_string(),
    }
}

/// B2B's "Duplicate listing" card is fed by Claude's listing_specificity
/// finding, whose `found: true` means the listing IS specific (good).
/// The card must read the same way as the B2C one ("None found" =
/// good, "Detected" = templated/generic), not "Confirmed", which a user
/// reads as "yes, this is a duplicate".
fn b2b_listing_specificity_signal(finding: &Finding) -> Signal {
    Signal {
        label: "Duplicate listing".to_string(),
        sub: finding.evidence.clone(),
        value: if finding.found {
            "None found".to_string()
        } else {
            "Detected".to_string()
        },
        signal_type: if finding.found {
            "good".to_string()
        } else {
            "caution".to_string()
        },
        category: "listing".to_string(),
        check_type: "pattern".to_string(),
    }
}

pub fn build_b2b_claude_signals(analysis: &B2bClaudeAnalysis) -> Vec<Signal> {
    vec![
        b2b_finding_to_signal(
            "Overall legitimacy check",
            "company",
            "existence",
            &analysis.business_legitimacy,
        ),
        b2b_finding_to_signal(
            "Registration consistency",
            "company",
            "consistency",
            &analysis.registration_consistency,
        ),
        b2b_listing_specificity_signal(&analysis.listing_specificity),
        Signal {
            label: "Price analysis".to_string(),
            sub: analysis.pricing_transparency.reasoning.clone(),
            value: analysis.pricing_transparency.verdict.clone(),
            signal_type: if analysis.pricing_transparency.verdict == "normal" {
                "good".to_string()
            } else {
                "caution".to_string()
            },
            category: "listing".to_string(),
            check_type: "anomaly".to_string(),
        },
        b2b_finding_to_signal(
            "Contact info",
            "identity",
            "existence",
            &analysis.contact_verifiability,
        ),
        finding_to_signal(
            "Urgency language",
            &analysis.urgency_language.evidence,
            &analysis.urgency_language,
            "communication",
            "pattern",
        ),
        finding_to_signal(
            "Advance payment request",
            &analysis.advance_payment_request.evidence,
            &analysis.advance_payment_request,
            "communication",
            "pattern",
        ),
        image_authenticity_signal(&analysis.image_authenticity),
    ]
}

/// Image authenticity card, shared by B2C and B2B. While image checking
/// is switched off (IMAGE_ANALYSIS_ENABLED in claude.rs), Claude never
/// sees the photos, so "not verified" says nothing about the seller -
/// the card then reads "Not checked" and does not count as a caution.
fn image_authenticity_signal(assessment: &ImageAssessment) -> Signal {
    if !IMAGE_ANALYSIS_ENABLED {
        return Signal {
            label: "Image authenticity".to_string(),
            sub: "Image checking is switched off in Safely for now, so this listing's photos were not reviewed. This does not count against the seller.".to_string(),
            value: "Not checked".to_string(),
            signal_type: "info".to_string(),
            category: "listing".to_string(),
            check_type: "existence".to_string(),
        };
    }
    Signal {
        label: "Image authenticity".to_string(),
        sub: assessment.reasoning.clone(),
        value: assessment.verdict.clone(),
        signal_type: if assessment.verdict == "original" {
            "good".to_string()
        } else {
            "caution".to_string()
        },
        category: "listing".to_string(),
        check_type: "existence".to_string(),
    }
}

/// The real, fixed display order: Table 1 (unified signals) always
/// first, then Table 2 (the contact info/verifiability pair, which
/// looks similar but means opposite things per platform), then Table
/// 3 (genuinely platform-specific signals).
fn table_rank(label: &str) -> u8 {
    match label {
        "Domain check" => 0,
        "Price analysis" => 1,
        "Urgency language" => 2,
        "Advance payment request" => 3,
        "Account age" => 4,
        "Duplicate listing" => 5,
        "Image authenticity" => 6,
        "Overall legitimacy check" => 7,
        "Safely history" => 8,
        "Seller website check" => 9,
        "Platform verification" => 10,
        "Contact info" => 11,
        "Store page check" => 12,
        "Seller track record" => 13,
        "Registration consistency" => 14,
        "Company profile completeness" => 15,
        "Listing completeness" => 16,

        _ => 255,
    }
}

pub fn sort_signals_by_table(signals: &mut [Signal]) {
    signals.sort_by_key(|s| table_rank(&s.label));
}

#[cfg(test)]
mod b2b_signal_tests {
    use super::*;
    use crate::services::claude::{ImageAssessment, PriceAssessment};

    fn f(found: bool) -> Finding {
        Finding {
            found,
            evidence: "e".into(),
        }
    }

    fn analysis(specific: bool, consistent: bool) -> B2bClaudeAnalysis {
        B2bClaudeAnalysis {
            business_legitimacy: f(true),
            registration_consistency: f(consistent),
            listing_specificity: f(specific),
            pricing_transparency: PriceAssessment {
                verdict: "normal".into(),
                reasoning: "r".into(),
            },
            contact_verifiability: f(true),
            urgency_language: f(false),
            advance_payment_request: f(false),
            image_authenticity: ImageAssessment {
                verdict: "not verified".into(),
                reasoning: "r".into(),
            },
            overall_risk_notes: String::new(),
        }
    }

    fn get<'a>(signals: &'a [Signal], label: &str) -> &'a Signal {
        signals.iter().find(|s| s.label == label).unwrap()
    }

    #[test]
    fn specific_listing_reads_none_found_and_good() {
        let s = build_b2b_claude_signals(&analysis(true, true));
        let d = get(&s, "Duplicate listing");
        assert_eq!(
            (d.value.as_str(), d.signal_type.as_str()),
            ("None found", "good")
        );
    }

    #[test]
    fn templated_listing_reads_detected_and_caution() {
        let s = build_b2b_claude_signals(&analysis(false, true));
        let d = get(&s, "Duplicate listing");
        assert_eq!(
            (d.value.as_str(), d.signal_type.as_str()),
            ("Detected", "caution")
        );
    }

    #[test]
    fn image_not_checked_is_info_while_images_are_off() {
        let s = build_b2b_claude_signals(&analysis(true, true));
        let i = get(&s, "Image authenticity");
        if !IMAGE_ANALYSIS_ENABLED {
            assert_eq!(
                (i.value.as_str(), i.signal_type.as_str()),
                ("Not checked", "info")
            );
        }
    }

    fn supplier(platform: &str, verified: bool, tier: Option<&str>) -> B2bSupplierProfile {
        B2bSupplierProfile {
            company_name: Some("Acme".into()),
            logo_url: None,
            year_established: None,
            country: None,
            platform_verified_badge: verified,
            employee_count: None,
            sales_revenue: None,
            export_percentage: None,
            profile_url: String::new(),
            source_platform: platform.into(),
            contact_name: None,
            contact_phone: None,
            badge_honorific: tier.map(|t| t.to_string()),
            company_description: None,
            website_url: None,
        }
    }

    #[test]
    fn exporthub_verification_is_not_offered() {
        let v = build_b2b_verification_signal(&supplier(
            "exporthub",
            false,
            Some("Standard Membership"),
        ));
        assert_eq!(
            (v.value.as_str(), v.signal_type.as_str()),
            ("Not offered", "info")
        );
        assert!(v.sub.contains("Standard Membership"));
        assert!(v.sub.contains("a paid membership level"));
        let free =
            build_b2b_verification_signal(&supplier("exporthub", false, Some("Free Member")));
        assert!(free.sub.contains("free membership level"));
        assert!(!free.sub.contains("paid"));
    }

    #[test]
    fn unverified_elsewhere_is_still_a_caution() {
        let v = build_b2b_verification_signal(&supplier("alibaba", false, None));
        assert_eq!(
            (v.value.as_str(), v.signal_type.as_str()),
            ("Unverified", "caution")
        );
        assert!(!v.sub.contains("B2Brazil"));
        let b = build_b2b_verification_signal(&supplier("b2brazil", false, None));
        assert!(b.sub.contains("not guaranteed accurate"));
    }

    #[test]
    fn consistent_registration_is_good() {
        let s = build_b2b_claude_signals(&analysis(true, true));
        let r = get(&s, "Registration consistency");
        assert_eq!(
            (r.value.as_str(), r.signal_type.as_str()),
            ("Confirmed", "good")
        );
    }
}
