use crate::{
    models::{analysis::Signal, helpers::format_account_age, sellers::Sellers},
    services::{
        b2b_scrapers::{B2bListingProfile, B2bSupplierProfile, SupplierRecord},
        b2c_scrapers::B2cProfileResult,
        claude::{
            B2bClaudeAnalysis, ClaudeAnalysis, Finding, IMAGE_ANALYSIS_ENABLED, ImageAssessment,
        },
        whois::WhoisResult,
    },
};
use chrono::{Datelike, Utc};

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

    // TradeWheel's badges (Gold, Platinum) are paid membership levels.
    // Shown for information only, never as verification and never as
    // a caution - a company without one has simply not paid for it.
    if supplier.source_platform == "tradewheel" {
        let (value, sub) = match supplier.badge_honorific.as_deref() {
            Some(tier) => (
                format!("{} member", tier),
                format!(
                    "{} is a {} member on TradeWheel. This is a paid membership level, not a check on the company.",
                    name, tier
                ),
            ),
            None => (
                "No badge".to_string(),
                format!(
                    "{} has no membership badge on TradeWheel. TradeWheel's badges are paid membership levels, so this says little about the company.",
                    name
                ),
            ),
        };
        return Signal {
            label: "Platform verification".to_string(),
            sub,
            value,
            signal_type: "info".to_string(),
            category: "identity".to_string(),
            check_type: "existence".to_string(),
        };
    }

    // b2bmap does not verify companies either. Its "Free Member", paid
    // plans and the paid "B2BMAP Verified Seal" are memberships, so the
    // plan is shown for information only - never as verified, never as
    // a caution.
    if supplier.source_platform == "b2bmap" {
        let (value, sub) = match supplier.badge_honorific.as_deref() {
            Some(plan) => (
                plan.to_string(),
                format!(
                    "{} is a {} on b2bmap. b2bmap does not verify companies; this is only the membership plan.",
                    name, plan
                ),
            ),
            None => (
                "Not offered".to_string(),
                "b2bmap does not verify companies, so there is no verification to show."
                    .to_string(),
            ),
        };
        return Signal {
            label: "Platform verification".to_string(),
            sub,
            value,
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
///
/// A company that gives no founding year at all is a small warning:
/// scam companies often leave it out so buyers can't see how new they
/// are. It counts only 10 points (see warning_points in analysis.rs),
/// less than other warnings, since honest suppliers leave it out too.
pub fn build_b2b_company_age_signal(supplier: &B2bSupplierProfile) -> Signal {
    let Some(year_str) = supplier.year_established.as_deref() else {
        return Signal {
            label: "Account age".to_string(),
            sub: "This company does not say when it was founded, so you cannot tell how new it is. Ask the supplier for its founding year and business licence.".to_string(),
            value: "Not provided".to_string(),
            signal_type: "caution".to_string(),
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

    // Only the founding YEAR is known, so the age is shown in whole
    // years ("About 11 years"), not "11 years 9 months" - the months
    // would just be counted from 1 January and mean nothing.
    let age_years = current_year - established_year;
    let value = company_age_from_year(age_years);
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

/// Uses the platform's own record of a supplier (see SupplierRecord in
/// b2b_scrapers - only Alibaba has one for now) to correct two cards
/// and add one. Runs after the normal B2B cards are built and before
/// the score is added up. With no record, nothing changes.
///
/// - "Platform verification": a company without the Verified badge
///   that the platform has still checked (on-site check / third-party
///   assessment) reads "Checked by Alibaba" and is good. One that only
///   pays for a membership (e.g. Gold Supplier) reads "Paid membership"
///   and is info - paid, so it counts neither for nor against it.
///   Neither: stays "Unverified" (a caution), as before.
/// - "Account age": when no founding year was found but the account
///   has been on the platform for 2 years or more, it shows those
///   years as info instead of the "Not provided" warning. Under 2
///   years the warning stays - a brand-new account is worth noting.
/// - "Seller track record" (new): the platform's own figures - orders
///   in the last 6 months, rating, on-time rate, reorder rate.
pub fn apply_supplier_record(signals: &mut Vec<Signal>, record: Option<&SupplierRecord>) {
    let Some(r) = record else {
        return;
    };
    let platform = r.platform.as_str();
    let years_text = |y: u32| format!("{} {}", y, if y == 1 { "year" } else { "years" });

    if let Some(s) = signals
        .iter_mut()
        .find(|s| s.label == "Platform verification" && s.value == "Unverified")
    {
        let membership = match (&r.member_label, r.years_on_platform) {
            (Some(label), Some(y)) => format!(
                " It has also been a paying {} on {} for {}.",
                label,
                platform,
                years_text(y)
            ),
            (Some(label), None) => format!(" It is also a paying {} on {}.", label, platform),
            _ => String::new(),
        };
        if r.checked_by_platform {
            s.value = format!("Checked by {}", platform);
            s.signal_type = "good".to_string();
            s.sub = format!(
                "This company does not have {p}'s Verified badge, but {p} has checked the company itself (an on-site check or a third-party assessment).{m}",
                p = platform,
                m = membership
            );
        } else if let Some(label) = &r.member_label {
            s.value = "Paid membership".to_string();
            s.signal_type = "info".to_string();
            let since = r
                .years_on_platform
                .map(|y| format!(" for {}", years_text(y)))
                .unwrap_or_default();
            s.sub = format!(
                "This company is a paying {label} on {p}{since}. That is a paid membership, not a check on the company, so it counts neither for nor against it. It does not have {p}'s Verified badge.",
                label = label,
                p = platform,
                since = since
            );
        }
    }

    if let Some(y) = r.years_on_platform.filter(|y| *y >= 2) {
        if let Some(s) = signals
            .iter_mut()
            .find(|s| s.label == "Account age" && s.value == "Not provided")
        {
            s.value = years_text(y);
            s.signal_type = "info".to_string();
            s.sub = format!(
                "This company does not show its founding year, but its account has been on {p} for {y}. That is how long it has sold on {p}, not how old the company is - ask the supplier for its business licence to see the founding year.",
                p = platform,
                y = years_text(y)
            );
        }
    }

    // The company says it is much older than its account here (B2Brazil:
    // "Established 2015" but "Since 2024"). Not a warning - many real
    // companies join late - but the buyer should know the platform has
    // no record of the years before.
    if let (Some(joined), Some(on_platform)) = (r.joined_platform_year, r.years_on_platform) {
        if let Some(s) = signals.iter_mut().find(|s| s.label == "Account age") {
            let company_years = s
                .value
                .strip_prefix("About ")
                .and_then(|v| v.split_whitespace().next())
                .and_then(|n| n.parse::<u32>().ok());
            if company_years.is_some_and(|y| y >= on_platform + 3) {
                s.sub = format!(
                    "{} It joined {p} only in {joined}, so {p} has no record of the years before that.",
                    s.sub.trim(),
                    p = platform,
                    joined = joined
                );
            }
        }
    }

    if let Some(track) = track_record_signal(r) {
        signals.push(track);
    }
}

/// "Product range" card values. The card is only added when the
/// company offers a product that is common bait in fake commodity
/// deals, so ordinary listings do not get it.
/// - "Commodity scam pattern" (caution): two or more different bait
///   products, or one bait product from a company that says it sells
///   "all kinds of products". risk_factors.rs makes this Serious.
/// - "Often used in scams" (info): one bait product only. Real sellers
///   of it exist, so it only adds a note.
pub const COMMODITY_SCAM_PATTERN: &str = "Commodity scam pattern";
pub const SCAM_BAIT_PRODUCT: &str = "Often used in scams";

/// Products that are the classic bait in fake commodity deals (the
/// buyer pays fees, a deposit or "documents" and nothing ships), as
/// (text to find, name to show). Lower case.
const BAIT_PRODUCTS: &[(&str, &str)] = &[
    ("icumsa", "ICUMSA 45 sugar"),
    ("urea 46", "Urea 46 fertilizer"),
    ("urea46", "Urea 46 fertilizer"),
    ("urea n46", "Urea 46 fertilizer"),
    ("en590", "EN590 diesel"),
    ("en 590", "EN590 diesel"),
    ("d2 diesel", "D2 diesel"),
    ("d2 gas oil", "D2 diesel"),
    ("d6 fuel", "D6 fuel oil"),
    ("d6 virgin", "D6 fuel oil"),
    ("jp54", "JP54 jet fuel"),
    ("jp 54", "JP54 jet fuel"),
    ("jp-54", "JP54 jet fuel"),
    ("jet a1", "Jet A-1 fuel"),
    ("jet a-1", "Jet A-1 fuel"),
    ("mazut", "mazut fuel oil"),
    ("chicken paws", "chicken paws"),
    ("chicken feet", "chicken paws"),
    ("copper cathode", "copper cathodes"),
    ("gold dust", "gold"),
    ("gold bar", "gold"),
    ("gold nugget", "gold"),
];

/// How a front company describes itself: it sells anything at all.
const CATCH_ALL_PHRASES: &[&str] = &[
    "all kinds of products",
    "all kinds of commodities",
    "all kinds of goods",
    "all types of products",
    "all types of commodities",
    "you name it",
    "any product you need",
];

/// Words in a certificate image's link that show it is a picture
/// downloaded from a free image or clipart site, not a scan of a real
/// certificate. Lower case.
const COPIED_IMAGE_MARKERS: &[&str] = &[
    "png-transparent",
    "pngtree",
    "pngwing",
    "pngegg",
    "pngitem",
    "kisspng",
    "freepik",
    "clipart",
    "clip-art",
    "shutterstock",
    "istockphoto",
    "dreamstime",
    "depositphotos",
    "123rf",
    "vecteezy",
    "stock-photo",
    "stock-vector",
];

/// ISO versions that were replaced long ago, so a certificate for them
/// can no longer be valid, as (text in the link, name to show).
const OUTDATED_ISO: &[(&str, &str)] = &[
    ("90012000", "ISO 9001:2000"),
    ("9001-2000", "ISO 9001:2000"),
    ("9001_2000", "ISO 9001:2000"),
    ("9001:2000", "ISO 9001:2000"),
    ("90012008", "ISO 9001:2008"),
    ("9001-2008", "ISO 9001:2008"),
    ("9001_2008", "ISO 9001:2008"),
    ("9001:2008", "ISO 9001:2008"),
    ("140012004", "ISO 14001:2004"),
    ("14001-2004", "ISO 14001:2004"),
    ("14001_2004", "ISO 14001:2004"),
    ("14001:2004", "ISO 14001:2004"),
];

pub const CERTIFICATES_COPIED: &str = "Look copied";
pub const CERTIFICATES_OUTDATED: &str = "Out of date";

/// Adds the cards that come from what the company lists about itself
/// (see SupplierRecord): "Product range" when it offers scam-bait
/// products, "Certificates" when its certificate images look copied or
/// name an ISO version that no longer exists. Nothing is added when
/// there is nothing to warn about.
pub fn apply_supplier_claims(
    signals: &mut Vec<Signal>,
    record: Option<&SupplierRecord>,
    supplier: &B2bSupplierProfile,
    listing: &B2bListingProfile,
) {
    if let Some(card) = product_range_signal(record, supplier, listing) {
        signals.push(card);
    }
    if let Some(card) = record.and_then(certificates_signal) {
        signals.push(card);
    }
}

fn product_range_signal(
    record: Option<&SupplierRecord>,
    supplier: &B2bSupplierProfile,
    listing: &B2bListingProfile,
) -> Option<Signal> {
    let mut texts: Vec<&str> = Vec::new();
    if let Some(r) = record {
        texts.extend(r.products_offered.iter().map(String::as_str));
    }
    texts.extend(listing.title.as_deref());
    texts.extend(listing.description.as_deref());
    texts.extend(supplier.company_description.as_deref());
    let all = texts.join("\n").to_lowercase();

    let mut bait: Vec<&str> = Vec::new();
    for (needle, name) in BAIT_PRODUCTS {
        if all.contains(needle) && !bait.contains(name) {
            bait.push(name);
        }
    }
    if bait.is_empty() {
        return None;
    }
    let catch_all = CATCH_ALL_PHRASES.iter().any(|p| all.contains(p));
    let name = supplier.company_name.as_deref().unwrap_or("This company");
    let items = join_names(&bait);
    let small = small_company_note(supplier);

    let (value, signal_type, sub) = if bait.len() >= 2 || catch_all {
        let sells_all = if catch_all {
            " and says it sells all kinds of products"
        } else {
            ""
        };
        (
            COMMODITY_SCAM_PATTERN,
            "caution",
            format!(
                "{name} offers {items}{sells_all}. These are some of the most common products in fake commodity deals: the buyer is asked to pay fees, a deposit or for documents first, and the goods never ship.{small} Never pay anything before the goods are inspected by a company you choose (such as SGS), and pay only by Letter of Credit."
            ),
        )
    } else {
        (
            SCAM_BAIT_PRODUCT,
            "info",
            format!(
                "{name} offers {items}. This product is often used in fake commodity deals, although real sellers exist.{small} Do not pay any fees or deposits before the goods are inspected by a company you choose (such as SGS)."
            ),
        )
    };
    Some(Signal {
        label: "Product range".to_string(),
        sub,
        value: value.to_string(),
        signal_type: signal_type.to_string(),
        category: "listing".to_string(),
        check_type: "pattern".to_string(),
    })
}

/// " The company also says it has 0-10 employees and yearly sales of
/// US$ 0 - 100K, which is very small for trading goods like these." -
/// or "" when the company does not look small.
fn small_company_note(supplier: &B2bSupplierProfile) -> String {
    let squash = |v: &str| v.to_lowercase().replace(' ', "");
    let small_staff = supplier
        .employee_count
        .as_deref()
        .filter(|e| ["0-10", "1-10", "0-5", "1-5"].contains(&squash(e).as_str()));
    let small_sales = supplier.sales_revenue.as_deref().filter(|v| {
        let v = squash(v);
        v.starts_with("0-") || v.starts_with("below") || v.starts_with("lessthan")
    });
    match (small_staff, small_sales) {
        (Some(e), Some(v)) => format!(
            " The company also says it has {} employees and yearly sales of US$ {}, which is very small for trading goods like these.",
            e, v
        ),
        (Some(e), None) => format!(
            " The company also says it has only {} employees, which is very small for trading goods like these.",
            e
        ),
        (None, Some(v)) => format!(
            " The company also says its yearly sales are US$ {}, which is very small for trading goods like these.",
            v
        ),
        (None, None) => String::new(),
    }
}

fn certificates_signal(record: &SupplierRecord) -> Option<Signal> {
    let total = record.certificate_images.len();
    let mut copied: Vec<String> = Vec::new();
    let mut outdated: Vec<&str> = Vec::new();
    for link in &record.certificate_images {
        let file = link.rsplit('/').next().unwrap_or(link).to_string();
        let lower = file.to_lowercase();
        if COPIED_IMAGE_MARKERS.iter().any(|m| lower.contains(m)) {
            copied.push(file.chars().take(60).collect());
        }
        for (needle, name) in OUTDATED_ISO {
            if lower.contains(needle) && !outdated.contains(name) {
                outdated.push(name);
            }
        }
    }
    if copied.is_empty() && outdated.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    if !copied.is_empty() {
        parts.push(format!(
            "{} of {} certificate {} {} named like a picture downloaded from a free image website (\"{}\"), not a scan of a real certificate.",
            copied.len(),
            total,
            if total == 1 { "image" } else { "images" },
            if copied.len() == 1 { "is" } else { "are" },
            copied[0]
        ));
    }
    if !outdated.is_empty() {
        parts.push(format!(
            "A certificate names {}, a version that was replaced years ago, so a certificate for it can no longer be valid.",
            join_names(&outdated)
        ));
    }
    parts.push(
        "Ask for each certificate's number and check it directly with the body that issued it."
            .to_string(),
    );
    Some(Signal {
        label: "Certificates".to_string(),
        sub: parts.join(" "),
        value: if copied.is_empty() {
            CERTIFICATES_OUTDATED
        } else {
            CERTIFICATES_COPIED
        }
        .to_string(),
        signal_type: "caution".to_string(),
        category: "company".to_string(),
        check_type: "consistency".to_string(),
    })
}

/// Value of the price card when the listing shows no price at all.
pub const PRICE_NOT_LISTED: &str = "Not listed";

/// When a listing shows no price, the price card cannot be "Normal" -
/// there was nothing to compare. It then reads "Not listed" (info).
/// Directory platforms that never show prices are left alone, and a
/// price Claude flagged stays flagged.
pub fn apply_missing_price_note(signals: &mut [Signal], listing: &B2bListingProfile) {
    if NO_ORDER_DETAILS_PLATFORMS.contains(&listing.source_platform.as_str())
        || listing.unit_price.is_some()
        || listing.fob_price.is_some()
    {
        return;
    }
    if let Some(card) = signals
        .iter_mut()
        .find(|s| s.label == "Price analysis" && s.value.eq_ignore_ascii_case("normal"))
    {
        card.value = PRICE_NOT_LISTED.to_string();
        card.signal_type = "info".to_string();
        card.sub = "No price is shown on this listing, so it could not be compared with the market. That is common in B2B trade - ask the supplier for a written quote before ordering.".to_string();
    }
}

/// Value of the website card when the company's own website was not on
/// the listing but turned up in the web search (social presence check).
pub const POSSIBLE_WEBSITE: &str = "Possible website found";

/// Endings of a company name that never appear in its web address
/// ("Co., Ltd", "Inc"...), lower case.
const NAME_ENDINGS: &[&str] = &[
    "co",
    "company",
    "ltd",
    "limited",
    "inc",
    "incorporated",
    "llc",
    "corp",
    "corporation",
    "gmbh",
    "ag",
    "kg",
    "pvt",
    "private",
    "plc",
    "sa",
    "sas",
    "srl",
    "spa",
    "bv",
    "nv",
    "ltda",
    "eireli",
    "me",
    "epp",
    "sdn",
    "bhd",
    "pte",
    "jsc",
    "the",
    "and",
    "of",
];

/// The main part of a web address: "https://www.goldensteelmill.com/about-us"
/// -> "goldensteelmill"; "shop.example.com.pk" -> "example". Sub-sites
/// of other sites ("goldensteel.tradeindia.com" -> "tradeindia") never
/// count as the company's own.
fn site_name(url: &str) -> Option<String> {
    let after_scheme = url.split("://").nth(1).unwrap_or(url);
    let host = after_scheme
        .split(['/', '?', '#', ':'])
        .next()?
        .trim()
        .to_lowercase();
    let parts: Vec<&str> = host.split('.').filter(|p| !p.is_empty()).collect();
    if parts.len() < 2 {
        return None;
    }
    let last = parts[parts.len() - 1];
    let second = parts[parts.len() - 2];
    // "example.com.pk", "example.co.uk": the name is one step further left.
    let two_part_ending =
        last.len() == 2 && ["com", "co", "net", "org", "gov", "edu", "ac"].contains(&second);
    let name = if two_part_ending {
        parts.get(parts.len().checked_sub(3)?)?
    } else {
        &second
    };
    Some(name.to_string())
}

/// "goldensteelmill.com" for "https://www.goldensteelmill.com/about-us".
fn site_root(url: &str) -> Option<String> {
    let after_scheme = url.split("://").nth(1).unwrap_or(url);
    let host = after_scheme
        .split(['/', '?', '#'])
        .next()?
        .trim()
        .to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    (!host.is_empty()).then_some(host)
}

/// True when every word of the company name is in the site's name:
/// "Golden Steel Mills" matches "goldensteelmill" ("mills" may lose its
/// "s"), but not "goldensteel" - that could be a different company.
fn site_matches_company(site: &str, company_name: &str) -> bool {
    let words: Vec<String> = company_name
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && !NAME_ENDINGS.contains(w))
        .map(str::to_string)
        .collect();
    if words.is_empty() || words.concat().len() < 4 {
        return false;
    }
    words.iter().all(|w| {
        site.contains(w.as_str())
            || (w.len() > 4 && w.ends_with('s') && site.contains(&w[..w.len() - 1]))
    })
}

/// When the listing showed no website but the web search found a site
/// whose address is the company's own name (e.g. "goldensteelmill.com"
/// for Golden Steel Mills), the website card shows it as a possible
/// website. Information only - it came from a search, not from the
/// supplier - so it never changes the score. A website the listing
/// already showed is never replaced.
pub fn apply_found_website(
    signals: &mut [Signal],
    company_name: Option<&str>,
    found_links: &[&str],
) {
    let Some(name) = company_name.filter(|n| !n.trim().is_empty()) else {
        return;
    };
    let Some(card) = signals
        .iter_mut()
        .find(|s| s.label == "Seller website check" && s.value == "No website found")
    else {
        return;
    };
    let Some(link) = found_links
        .iter()
        .find(|link| site_name(link).is_some_and(|site| site_matches_company(&site, name)))
    else {
        return;
    };
    let Some(root) = site_root(link) else {
        return;
    };
    card.value = POSSIBLE_WEBSITE.to_string();
    card.signal_type = "info".to_string();
    card.sub = format!(
        "The listing shows no website, but a web search found {root}, whose address matches this company's name (found at {link}). It was not given by the supplier, so check that it really belongs to them before trusting it."
    );
}

/// Below this average rating (with enough reviews to mean something),
/// the track record is a warning.
const LOW_RATING: f64 = 3.5;
const MIN_REVIEWS_FOR_RATING: u64 = 5;

/// The "Seller track record" card from the platform's own figures.
/// None when the platform shows no order count.
fn track_record_signal(r: &SupplierRecord) -> Option<Signal> {
    let orders = r.orders_6_months?;
    let platform = r.platform.as_str();

    let mut facts = vec![format!(
        "{} {} paid through {} in the last 6 months{}",
        orders,
        if orders == 1 { "order" } else { "orders" },
        platform,
        r.order_value_6_months
            .as_deref()
            .map(|v| format!(" (US$ {})", v))
            .unwrap_or_default()
    )];
    if let Some(rating) = r.rating {
        facts.push(match r.review_count {
            Some(n) => format!(
                "a rating of {:.1} out of 5 from {} {}",
                rating,
                n,
                if n == 1 { "review" } else { "reviews" }
            ),
            None => format!("a rating of {:.1} out of 5", rating),
        });
    }
    if let Some(on_time) = &r.on_time_rate {
        facts.push(format!("{} on-time dispatch", on_time));
    }
    if let Some(reorder) = &r.reorder_rate {
        facts.push(format!("{} of buyers order again", reorder));
    }
    let listed = match facts.len() {
        1 => facts[0].clone(),
        n => format!("{} and {}", facts[..n - 1].join(", "), facts[n - 1]),
    };
    let source = format!(
        "These are {p}'s own figures for orders paid through {p}.",
        p = platform
    );

    let low_rating = r.rating.map_or(false, |x| x < LOW_RATING)
        && r.review_count.unwrap_or(0) >= MIN_REVIEWS_FOR_RATING;

    let (value, signal_type, sub) = if low_rating {
        (
            format!(
                "{} orders, {:.1} rating",
                orders,
                r.rating.unwrap_or_default()
            ),
            "caution",
            format!(
                "Buyers rate this supplier low. {} shows {}. {} Read the reviews before ordering.",
                platform, listed, source
            ),
        )
    } else if orders == 0 {
        (
            "No recent orders".to_string(),
            "info",
            format!(
                "{} shows {}. {} No recent orders is common for new or mostly offline suppliers, but it means there is less proof that this supplier delivers.",
                platform, listed, source
            ),
        )
    } else {
        (
            match r.rating {
                Some(x) => format!("{} orders, {:.1} rating", orders, x),
                None => format!("{} orders", orders),
            },
            "good",
            format!("{} shows {}. {}", platform, listed, source),
        )
    };

    Some(Signal {
        label: "Seller track record".to_string(),
        sub,
        value,
        signal_type: signal_type.to_string(),
        category: "reputation".to_string(),
        check_type: "anomaly".to_string(),
    })
}

/// "Founded this year", "About 1 year", "About 11 years".
pub fn company_age_from_year(age_years: i32) -> String {
    match age_years {
        i32::MIN..=0 => "Founded this year".to_string(),
        1 => "About 1 year".to_string(),
        n => format!("About {} years", n),
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

/// Platforms that are supplier directories, not marketplaces: they
/// never show price, MOQ, Incoterms, packaging or delivery details for
/// any supplier. Add a platform here only when NO listing on it can
/// ever have these fields.
pub const NO_ORDER_DETAILS_PLATFORMS: [&str; 2] = ["thomasnet", "kompass"];

/// Checks how many of the real, listing-specific fields (price, MOQ,
/// Incoterms, etc.) were genuinely filled in versus left as "Not
/// informed." Incomplete listings are common in B2B and not
/// inherently suspicious, so this stays a mild pattern check, not a
/// harsh red flag.
/// How many of the 9 listing details make the listing card "good".
const LISTING_FIELDS_FOR_GOOD: usize = 5;

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

    // A seller can't be marked down for details the platform has no
    // place for. Directory platforms (see NO_ORDER_DETAILS_PLATFORMS)
    // never show price, MOQ, Incoterms etc., so on them an empty
    // listing is normal and stays information only.
    if NO_ORDER_DETAILS_PLATFORMS.contains(&listing.source_platform.as_str()) {
        return Signal {
            label: "Listing completeness".to_string(),
            sub: "This platform is a supplier directory and does not show price, minimum order or delivery details for any supplier, so this is not counted against them.".to_string(),
            value: "Not shown on this platform".to_string(),
            signal_type: "info".to_string(),
            category: "listing".to_string(),
            check_type: "pattern".to_string(),
        };
    }

    // Nothing filled in is a warning; most of it filled in (5 of 9 or
    // more) is good, the same idea as the company profile card (2 of 3);
    // anything in between is information only.
    let signal_type = if filled_count == 0 {
        "caution"
    } else if filled_count >= LISTING_FIELDS_FOR_GOOD {
        "good"
    } else {
        "info"
    };

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

/// B2B's listing card is fed by Claude's listing_specificity finding,
/// whose `found: true` means the listing IS specific (good). Claude
/// only judges whether the description is specific or vague and
/// template-like - it never searches for copies of the listing - so on
/// B2B the card is called "Listing detail" ("Specific" / "Vague"),
/// not "Duplicate listing", which read as "a copy was found".
fn b2b_listing_specificity_signal(finding: &Finding) -> Signal {
    Signal {
        label: "Listing detail".to_string(),
        sub: finding.evidence.clone(),
        value: if finding.found {
            "Specific".to_string()
        } else {
            "Vague".to_string()
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
    let mut signals = vec![
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
        b2b_payment_signal(
            &analysis.advance_payment_request,
            &analysis.untraceable_payment_method,
        ),
        image_authenticity_signal(&analysis.image_authenticity),
    ];
    if let Some(regulated) =
        b2b_regulated_product_signal(&analysis.regulated_product, &analysis.maker_claim_mismatch)
    {
        signals.push(regulated);
    }
    signals
}

/// "Regulated product" card - shown ONLY when the product needs a
/// licence or prescription (botox, fillers, prescription medicines...).
/// Ordinary listings do not get this card, so their signal count does
/// not change.
/// - "Licence needed": regulated product. Worth noting.
/// - "Licence needed, not the maker": regulated product AND the seller
///   calls itself the manufacturer of another company's brand.
///   risk_factors.rs turns this into a combined flag (Pattern match).
pub const REGULATED_PRODUCT: &str = "Licence needed";
pub const REGULATED_NOT_MAKER: &str = "Licence needed, not the maker";

fn b2b_regulated_product_signal(regulated: &Finding, maker_mismatch: &Finding) -> Option<Signal> {
    if !regulated.found {
        return None;
    }
    let (value, sub) = if maker_mismatch.found {
        (
            REGULATED_NOT_MAKER,
            format!(
                "{} {}",
                regulated.evidence.trim(),
                maker_mismatch.evidence.trim()
            )
            .trim()
            .to_string(),
        )
    } else {
        (REGULATED_PRODUCT, regulated.evidence.trim().to_string())
    };
    Some(Signal {
        label: "Regulated product".to_string(),
        sub,
        value: value.to_string(),
        signal_type: "caution".to_string(),
        category: "listing".to_string(),
        check_type: "pattern".to_string(),
    })
}

/// B2B payment card. Two different problems get two different values,
/// because they are not equally dangerous:
/// - "Untraceable payment": Western Union, MoneyGram, crypto, gift
///   cards or a personal account - money that cannot be got back.
///   risk_factors.rs treats this as Serious.
/// - "Full prepayment": 100% before shipment by a normal method (bank
///   transfer). A real risk but common - Worth noting, or a combined
///   flag (Pattern match) when the company is brand new.
/// Both are "caution"; "None found" is good.
pub const UNTRACEABLE_PAYMENT: &str = "Untraceable payment";
pub const FULL_PREPAYMENT: &str = "Full prepayment";

fn b2b_payment_signal(advance: &Finding, untraceable: &Finding) -> Signal {
    let (value, sub, signal_type) = if untraceable.found {
        let mut sub = untraceable.evidence.clone();
        if advance.found && !advance.evidence.trim().is_empty() {
            sub = format!("{} {}", sub, advance.evidence).trim().to_string();
        }
        (UNTRACEABLE_PAYMENT, sub, "caution")
    } else if advance.found {
        (FULL_PREPAYMENT, advance.evidence.clone(), "caution")
    } else {
        ("None found", advance.evidence.clone(), "good")
    };
    Signal {
        label: "Advance payment request".to_string(),
        sub,
        value: value.to_string(),
        signal_type: signal_type.to_string(),
        category: "communication".to_string(),
        check_type: "pattern".to_string(),
    }
}

/// Value of the payment card when a risky way to pay (Western Union,
/// MoneyGram, crypto, gift cards) is listed next to protected ones.
pub const RISKY_OPTION_LISTED: &str = "Risky option listed";

/// Ways to pay that cannot be traced or reversed, as (text to find,
/// name to show). Matched case-insensitively on the listing's payment
/// methods.
const RISKY_METHODS: &[(&str, &str)] = &[
    ("western union", "Western Union"),
    ("moneygram", "MoneyGram"),
    ("money gram", "MoneyGram"),
    ("bitcoin", "cryptocurrency"),
    ("usdt", "cryptocurrency"),
    ("crypto", "cryptocurrency"),
    ("gift card", "gift cards"),
];

/// Ways to pay that protect the buyer or can be traced to a company,
/// as (text to find, name to show).
const PROTECTED_METHODS: &[(&str, &str)] = &[
    ("trade assurance", "Trade Assurance"),
    ("escrow", "escrow"),
    ("paypal", "PayPal"),
    ("credit card", "credit card"),
    ("visa", "credit card"),
    ("mastercard", "credit card"),
    ("l/c", "L/C"),
    ("letter of credit", "L/C"),
    ("d/p", "D/P"),
    ("d/a", "D/A"),
    ("t/t", "bank transfer (T/T)"),
    ("bank", "bank transfer (T/T)"),
    ("wire", "bank transfer (T/T)"),
];

/// Names from `methods` found in `text`, each once, in list order.
fn methods_in(text: &str, methods: &[(&str, &'static str)]) -> Vec<&'static str> {
    let mut found: Vec<&'static str> = Vec::new();
    for (needle, name) in methods {
        if text.contains(needle) && !found.contains(name) {
            found.push(name);
        }
    }
    found
}

/// "A", "A and B", "A, B and C".
fn join_names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} and {}", rest.join(", "), last),
    }
}

/// Checks the listing's own payment methods for risky ones, so the
/// buyer is always told about them - not only when Claude flags them.
/// It only changes the payment card when it reads "None found":
/// - a risky method next to protected ones: "Risky option listed",
///   info (no points). The card names the risky method, says why it is
///   dangerous and which protected option to use instead.
/// - risky methods only, nothing protected: "Untraceable payment",
///   caution - the same as when Claude flags it (Serious risk factor).
/// A card Claude already flagged (full prepayment / untraceable) stays
/// as it is.
pub fn apply_risky_payment_note(signals: &mut [Signal], payment_methods: Option<&str>) {
    let Some(text) = payment_methods else {
        return;
    };
    let text = text.to_lowercase();
    let risky = methods_in(&text, RISKY_METHODS);
    if risky.is_empty() {
        return;
    }
    let Some(card) = signals
        .iter_mut()
        .find(|s| s.label == "Advance payment request")
    else {
        return;
    };
    if card.value != "None found" {
        return;
    }
    let mut protected = methods_in(&text, PROTECTED_METHODS);
    // Platform order protection is the safest choice - recommend only it.
    if let Some(best) = protected
        .iter()
        .find(|m| **m == "Trade Assurance" || **m == "escrow")
        .copied()
    {
        protected = vec![best];
    }
    let risky_names = join_names(&risky);
    let verb = if risky.len() == 1 && !risky_names.ends_with('s') {
        "is"
    } else {
        "are"
    };
    let (value, signal_type, warning) = if protected.is_empty() {
        (
            UNTRACEABLE_PAYMENT,
            "caution",
            format!(
                "The only ways to pay listed are {risky_names}. Money sent this way cannot be traced or got back, and scammers often ask for it. Do not pay this way - ask for a protected payment method first."
            ),
        )
    } else {
        (
            RISKY_OPTION_LISTED,
            "info",
            format!(
                "{risky_names} {verb} listed as a way to pay. Money sent this way cannot be traced or got back, and scammers often ask for it. Avoid it and pay with {} instead.",
                join_names(&protected)
            ),
        )
    };
    card.sub = format!("{} {}", warning, card.sub.trim())
        .trim()
        .to_string();
    card.value = value.to_string();
    card.signal_type = signal_type.to_string();
}

/// Image authenticity card, shared by B2C and B2B. While image checking
/// is switched off (IMAGE_ANALYSIS_ENABLED in claude.rs), Claude never
/// sees the photos, so "not verified" says nothing about the seller -
/// the card then reads "Not checked" and does not count as a caution.
///
/// With image checking on, "original" is good. "not verified" means
/// Claude could not confirm the photos are the supplier's own - most
/// real suppliers use catalogue photos, so on its own this is only
/// "info" and adds no points. Together with a vague or duplicate
/// listing it still makes a combined flag (see risk_factors.rs).
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
            "info".to_string()
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
        "Duplicate listing" | "Listing detail" => 5,
        // Shown right after Listing detail (same rank, added after it).
        "Regulated product" => 5,
        "Product range" => 5,
        "Image authenticity" => 6,
        "Overall legitimacy check" => 7,
        "Safely history" => 8,
        "Seller website check" => 9,
        "Platform verification" => 10,
        "Contact info" => 11,
        "Store page check" => 12,
        "Seller track record" => 13,
        "Registration consistency" => 14,
        // Shown right after Registration consistency.
        "Certificates" => 14,
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

    fn empty_listing(platform: &str) -> B2bListingProfile {
        B2bListingProfile {
            title: None,
            description: None,
            image_urls: Vec::new(),
            unit_price: None,
            fob_price: None,
            minimum_order_quantity: None,
            payment_type: None,
            preferred_port: None,
            reference: None,
            production_capacity: None,
            delivery_timeframe: None,
            incoterms: None,
            packaging_details: None,
            listing_url: "u".to_string(),
            source_platform: platform.to_string(),
        }
    }

    #[test]
    fn empty_listing_is_caution_on_a_marketplace() {
        let s = build_b2b_listing_completeness_signal(&empty_listing("alibaba"));
        assert_eq!(s.signal_type, "caution");
        assert_eq!(s.value, "0/9 fields provided");
    }

    #[test]
    fn mostly_filled_listing_is_good_and_a_few_fields_are_info() {
        let mut l = empty_listing("b2bmap");
        l.unit_price = Some("USD 62500".into());
        l.minimum_order_quantity = Some("1 Sets".into());
        l.payment_type = Some("L/C, T/T".into());
        l.packaging_details = Some("Export packing".into());
        let s = build_b2b_listing_completeness_signal(&l);
        assert_eq!(
            (s.value.as_str(), s.signal_type.as_str()),
            ("4/9 fields provided", "info")
        );
        l.incoterms = Some("FOB, CIF".into());
        let s = build_b2b_listing_completeness_signal(&l);
        assert_eq!(
            (s.value.as_str(), s.signal_type.as_str()),
            ("5/9 fields provided", "good")
        );
    }

    #[test]
    fn empty_listing_is_only_info_on_a_directory_platform() {
        for platform in ["thomasnet", "kompass"] {
            let s = build_b2b_listing_completeness_signal(&empty_listing(platform));
            assert_eq!(s.signal_type, "info");
            assert_eq!(s.value, "Not shown on this platform");
        }
    }
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
            untraceable_payment_method: f(false),
            regulated_product: f(false),
            maker_claim_mismatch: f(false),
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

    fn payment(advance: bool, untraceable: bool) -> Signal {
        let mut a = analysis(true, true);
        a.advance_payment_request = f(advance);
        a.untraceable_payment_method = f(untraceable);
        build_b2b_claude_signals(&a)
            .into_iter()
            .find(|s| s.label == "Advance payment request")
            .unwrap()
    }

    fn regulated(found: bool, mismatch: bool) -> Option<Signal> {
        let mut a = analysis(true, true);
        a.regulated_product = f(found);
        a.maker_claim_mismatch = f(mismatch);
        build_b2b_claude_signals(&a)
            .into_iter()
            .find(|s| s.label == "Regulated product")
    }

    #[test]
    fn regulated_product_card_only_appears_for_licence_only_products() {
        assert!(
            regulated(false, false).is_none(),
            "ordinary products get no card"
        );
        assert!(
            regulated(false, true).is_none(),
            "a maker claim alone does not add the card"
        );
        let r = regulated(true, false).unwrap();
        assert_eq!(
            (r.value.as_str(), r.signal_type.as_str()),
            (REGULATED_PRODUCT, "caution")
        );
        let r = regulated(true, true).unwrap();
        assert_eq!(
            (r.value.as_str(), r.signal_type.as_str()),
            (REGULATED_NOT_MAKER, "caution")
        );
        assert_eq!(r.sub, "e e");
    }

    #[test]
    fn payment_card_tells_full_prepayment_and_untraceable_methods_apart() {
        let none = payment(false, false);
        assert_eq!(
            (none.value.as_str(), none.signal_type.as_str()),
            ("None found", "good")
        );
        let prepay = payment(true, false);
        assert_eq!(
            (prepay.value.as_str(), prepay.signal_type.as_str()),
            (FULL_PREPAYMENT, "caution")
        );
        let wu = payment(false, true);
        assert_eq!(
            (wu.value.as_str(), wu.signal_type.as_str()),
            (UNTRACEABLE_PAYMENT, "caution")
        );
        let both = payment(true, true);
        assert_eq!(
            both.value, UNTRACEABLE_PAYMENT,
            "the more serious problem wins"
        );
        assert_eq!(both.sub, "e e");
    }

    fn none_found_card() -> Vec<Signal> {
        vec![payment(false, false)]
    }

    #[test]
    fn risky_method_next_to_protected_ones_is_info_with_a_warning() {
        let mut s = none_found_card();
        apply_risky_payment_note(&mut s, Some("Trade Assurance/Paypal/Western Union/T/T"));
        assert_eq!(
            (s[0].value.as_str(), s[0].signal_type.as_str()),
            (RISKY_OPTION_LISTED, "info")
        );
        assert!(
            s[0].sub
                .starts_with("Western Union is listed as a way to pay.")
        );
        assert!(
            s[0].sub
                .contains("Avoid it and pay with Trade Assurance instead.")
        );
        let mut s = none_found_card();
        apply_risky_payment_note(&mut s, Some("PayPal, Western Union, T/T"));
        assert!(
            s[0].sub
                .contains("pay with PayPal and bank transfer (T/T) instead")
        );
    }

    #[test]
    fn only_risky_methods_is_untraceable_caution() {
        let mut s = none_found_card();
        apply_risky_payment_note(&mut s, Some("Western Union, MoneyGram"));
        assert_eq!(
            (s[0].value.as_str(), s[0].signal_type.as_str()),
            (UNTRACEABLE_PAYMENT, "caution")
        );
        assert!(
            s[0].sub
                .starts_with("The only ways to pay listed are Western Union and MoneyGram.")
        );
    }

    #[test]
    fn safe_methods_only_leave_the_card_alone() {
        let mut s = none_found_card();
        apply_risky_payment_note(&mut s, Some("T/T, L/C, PayPal"));
        assert_eq!(
            (s[0].value.as_str(), s[0].signal_type.as_str()),
            ("None found", "good")
        );
        let mut s = none_found_card();
        apply_risky_payment_note(&mut s, None);
        assert_eq!(s[0].value, "None found");
    }

    #[test]
    fn a_card_claude_already_flagged_is_not_changed() {
        let mut s = vec![payment(true, false)];
        apply_risky_payment_note(&mut s, Some("Western Union, T/T"));
        assert_eq!(s[0].value, FULL_PREPAYMENT);
        assert_eq!(s[0].sub, "e");
    }

    #[test]
    fn company_age_is_shown_in_whole_years() {
        assert_eq!(company_age_from_year(0), "Founded this year");
        assert_eq!(company_age_from_year(1), "About 1 year");
        assert_eq!(company_age_from_year(11), "About 11 years");
    }

    #[test]
    fn specific_listing_reads_specific_and_good() {
        let s = build_b2b_claude_signals(&analysis(true, true));
        let d = get(&s, "Listing detail");
        assert_eq!(
            (d.value.as_str(), d.signal_type.as_str()),
            ("Specific", "good")
        );
    }

    #[test]
    fn templated_listing_reads_vague_and_caution() {
        let s = build_b2b_claude_signals(&analysis(false, true));
        let d = get(&s, "Listing detail");
        assert_eq!(
            (d.value.as_str(), d.signal_type.as_str()),
            ("Vague", "caution")
        );
        assert!(s.iter().all(|x| x.label != "Duplicate listing"));
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

    #[test]
    fn unconfirmed_photos_are_info_not_a_warning() {
        if !IMAGE_ANALYSIS_ENABLED {
            return;
        }
        let s = build_b2b_claude_signals(&analysis(true, true));
        let i = get(&s, "Image authenticity");
        assert_eq!(
            (i.value.as_str(), i.signal_type.as_str()),
            ("not verified", "info")
        );
        let mut a = analysis(true, true);
        a.image_authenticity.verdict = "original".into();
        let s = build_b2b_claude_signals(&a);
        assert_eq!(get(&s, "Image authenticity").signal_type, "good");
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
    fn a_hidden_founding_year_is_a_small_warning() {
        let s = build_b2b_company_age_signal(&supplier("alibaba", true, None));
        assert_eq!(
            (s.value.as_str(), s.signal_type.as_str()),
            ("Not provided", "caution")
        );
        assert!(s.sub.contains("does not say when it was founded"));
    }

    fn yingmo_record() -> SupplierRecord {
        SupplierRecord {
            platform: "Alibaba".into(),
            checked_by_platform: true,
            member_label: Some("Gold Supplier".into()),
            years_on_platform: Some(7),
            orders_6_months: Some(197),
            order_value_6_months: Some("260,000+".into()),
            rating: Some(4.8),
            review_count: Some(466),
            on_time_rate: Some("100.0%".into()),
            reorder_rate: Some("21%".into()),
            ..Default::default()
        }
    }

    fn alibaba_cards() -> Vec<Signal> {
        let s = supplier("alibaba", false, None);
        vec![
            build_b2b_verification_signal(&s),
            build_b2b_company_age_signal(&s),
        ]
    }

    #[test]
    fn no_record_changes_nothing() {
        let mut cards = alibaba_cards();
        apply_supplier_record(&mut cards, None);
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].value, "Unverified");
        assert_eq!(cards[1].value, "Not provided");
    }

    #[test]
    fn checked_supplier_with_history_has_no_warnings() {
        let mut cards = alibaba_cards();
        apply_supplier_record(&mut cards, Some(&yingmo_record()));
        let v = get(&cards, "Platform verification");
        assert_eq!(
            (v.value.as_str(), v.signal_type.as_str()),
            ("Checked by Alibaba", "good")
        );
        assert!(
            v.sub
                .contains("paying Gold Supplier on Alibaba for 7 years")
        );
        let a = get(&cards, "Account age");
        assert_eq!(
            (a.value.as_str(), a.signal_type.as_str()),
            ("7 years", "info")
        );
        assert!(a.sub.contains("not how old the company is"));
        let t = get(&cards, "Seller track record");
        assert_eq!(
            (t.value.as_str(), t.signal_type.as_str()),
            ("197 orders, 4.8 rating", "good")
        );
        assert_eq!(
            t.sub,
            "Alibaba shows 197 orders paid through Alibaba in the last 6 months (US$ 260,000+), a rating of 4.8 out of 5 from 466 reviews, 100.0% on-time dispatch and 21% of buyers order again. These are Alibaba's own figures for orders paid through Alibaba."
        );
        assert!(cards.iter().all(|c| c.signal_type != "caution"));
    }

    #[test]
    fn paid_membership_alone_is_info_not_verified() {
        let mut r = yingmo_record();
        r.checked_by_platform = false;
        let mut cards = alibaba_cards();
        apply_supplier_record(&mut cards, Some(&r));
        let v = get(&cards, "Platform verification");
        assert_eq!(
            (v.value.as_str(), v.signal_type.as_str()),
            ("Paid membership", "info")
        );
        assert!(v.sub.contains("not a check on the company"));
    }

    #[test]
    fn no_check_and_no_membership_stays_unverified() {
        let mut r = yingmo_record();
        r.checked_by_platform = false;
        r.member_label = None;
        let mut cards = alibaba_cards();
        apply_supplier_record(&mut cards, Some(&r));
        assert_eq!(get(&cards, "Platform verification").signal_type, "caution");
    }

    #[test]
    fn verified_badge_is_never_replaced() {
        let s = supplier("alibaba", true, None);
        let mut cards = vec![build_b2b_verification_signal(&s)];
        apply_supplier_record(&mut cards, Some(&yingmo_record()));
        assert_eq!(cards[0].value, "Verified");
    }

    #[test]
    fn a_new_account_without_a_founding_year_is_still_a_warning() {
        let mut r = yingmo_record();
        r.years_on_platform = Some(1);
        let mut cards = alibaba_cards();
        apply_supplier_record(&mut cards, Some(&r));
        let a = get(&cards, "Account age");
        assert_eq!(
            (a.value.as_str(), a.signal_type.as_str()),
            ("Not provided", "caution")
        );
    }

    #[test]
    fn a_real_founding_year_is_never_replaced() {
        let mut s = supplier("alibaba", false, None);
        s.year_established = Some("2020".into());
        let mut cards = vec![build_b2b_company_age_signal(&s)];
        apply_supplier_record(&mut cards, Some(&yingmo_record()));
        assert!(cards[0].value.starts_with("About"));
    }

    #[test]
    fn low_rating_is_a_warning_and_no_orders_is_info() {
        let mut r = yingmo_record();
        r.rating = Some(2.9);
        r.review_count = Some(40);
        let t = track_record_signal(&r).unwrap();
        assert_eq!(
            (t.value.as_str(), t.signal_type.as_str()),
            ("197 orders, 2.9 rating", "caution")
        );
        assert!(t.sub.starts_with("Buyers rate this supplier low."));

        // A low rating from only a few reviews is not enough.
        r.review_count = Some(2);
        assert_eq!(track_record_signal(&r).unwrap().signal_type, "good");

        let mut r = yingmo_record();
        r.orders_6_months = Some(0);
        r.rating = None;
        let t = track_record_signal(&r).unwrap();
        assert_eq!(
            (t.value.as_str(), t.signal_type.as_str()),
            ("No recent orders", "info")
        );

        r.orders_6_months = None;
        assert!(track_record_signal(&r).is_none());
    }

    fn daakiiya() -> (SupplierRecord, B2bSupplierProfile, B2bListingProfile) {
        let record = SupplierRecord {
            platform: "B2Brazil".into(),
            joined_platform_year: Some(2024),
            years_on_platform: Some(2),
            products_offered: vec![
                "SUGAR".into(),
                "CHICKEN PAWS".into(),
                "ICUMSA 45 Sugar".into(),
                "Urea 46 Fertilizer".into(),
            ],
            certificate_images: vec![
                "https://cdn.b2brazil.com/certs/thumb_sem_titulo54-52c3a5.png.webp".into(),
                "https://cdn.b2brazil.com/certs/395_sgssystemcertiso90012000-13bb15.jpg.webp"
                    .into(),
                "https://cdn.b2brazil.com/certs/png-transparent-halal-tourism-logo-e3ed98.png.webp"
                    .into(),
            ],
            ..Default::default()
        };
        let mut s = supplier("b2brazil", true, None);
        s.company_name = Some("DAAKIIYA".into());
        s.employee_count = Some("0-10".into());
        s.sales_revenue = Some("0 - 100K".into());
        s.company_description = Some(
            "Welcome to DAAKIIYA, your one-stop Commodity store for all kinds of products.".into(),
        );
        let mut l = empty_listing("b2brazil");
        l.title = Some("Bethel Nut".into());
        (record, s, l)
    }

    #[test]
    fn bait_products_from_a_small_sell_everything_company_are_a_scam_pattern() {
        let (r, s, l) = daakiiya();
        let mut cards = Vec::new();
        apply_supplier_claims(&mut cards, Some(&r), &s, &l);
        let p = get(&cards, "Product range");
        assert_eq!(
            (p.value.as_str(), p.signal_type.as_str()),
            (COMMODITY_SCAM_PATTERN, "caution")
        );
        assert!(p.sub.starts_with(
            "DAAKIIYA offers ICUMSA 45 sugar, Urea 46 fertilizer and chicken paws and says it sells all kinds of products."
        ));
        assert!(
            p.sub
                .contains("0-10 employees and yearly sales of US$ 0 - 100K")
        );
        let c = get(&cards, "Certificates");
        assert_eq!(
            (c.value.as_str(), c.signal_type.as_str()),
            (CERTIFICATES_COPIED, "caution")
        );
        assert!(
            c.sub
                .starts_with("1 of 3 certificate images is named like a picture")
        );
        assert!(c.sub.contains("ISO 9001:2000"));
    }

    #[test]
    fn one_bait_product_alone_is_only_info() {
        let (_, mut s, mut l) = daakiiya();
        s.company_description = None;
        s.employee_count = None;
        s.sales_revenue = None;
        l.title = Some("Urea 46% Granular".into());
        let mut cards = Vec::new();
        apply_supplier_claims(&mut cards, None, &s, &l);
        let p = get(&cards, "Product range");
        assert_eq!(
            (p.value.as_str(), p.signal_type.as_str()),
            (SCAM_BAIT_PRODUCT, "info")
        );
        assert!(cards.iter().all(|c| c.label != "Certificates"));
    }

    #[test]
    fn ordinary_products_and_certificates_add_no_cards() {
        let (mut r, mut s, mut l) = daakiiya();
        r.products_offered = vec!["Reading glasses".into(), "Sunglasses".into()];
        r.certificate_images = vec!["https://cdn.example.com/certs/ce-certificate-2023.jpg".into()];
        s.company_description = Some("We make optical frames.".into());
        l.title = Some("TR90 reading glasses".into());
        let mut cards = Vec::new();
        apply_supplier_claims(&mut cards, Some(&r), &s, &l);
        assert!(cards.is_empty());
        apply_supplier_claims(&mut cards, None, &s, &l);
        assert!(cards.is_empty());
    }

    #[test]
    fn a_late_join_is_noted_on_the_account_age_card() {
        let (r, mut s, _) = daakiiya();
        s.year_established = Some((Utc::now().year() - 11).to_string());
        let mut cards = vec![build_b2b_company_age_signal(&s)];
        apply_supplier_record(&mut cards, Some(&r));
        let a = get(&cards, "Account age");
        assert_eq!(a.signal_type, "good", "not a warning");
        assert!(a.sub.ends_with(
            "It joined B2Brazil only in 2024, so B2Brazil has no record of the years before that."
        ));
        // A company that joined soon after it was founded gets no note.
        s.year_established = Some((Utc::now().year() - 3).to_string());
        let mut cards = vec![build_b2b_company_age_signal(&s)];
        apply_supplier_record(&mut cards, Some(&r));
        assert!(!get(&cards, "Account age").sub.contains("joined"));
    }

    #[test]
    fn no_price_on_the_listing_reads_not_listed() {
        let price = || {
            vec![Signal {
                label: "Price analysis".into(),
                sub: "Pricing on request is standard.".into(),
                value: "normal".into(),
                signal_type: "good".into(),
                category: String::new(),
                check_type: String::new(),
            }]
        };
        let mut cards = price();
        apply_missing_price_note(&mut cards, &empty_listing("b2brazil"));
        assert_eq!(
            (cards[0].value.as_str(), cards[0].signal_type.as_str()),
            (PRICE_NOT_LISTED, "info")
        );
        let mut priced = empty_listing("b2brazil");
        priced.unit_price = Some("US$ 5".into());
        let mut cards = price();
        apply_missing_price_note(&mut cards, &priced);
        assert_eq!(cards[0].value, "normal");
        let mut cards = price();
        apply_missing_price_note(&mut cards, &empty_listing("kompass"));
        assert_eq!(cards[0].value, "normal", "directories never show prices");
    }

    fn no_website_card() -> Vec<Signal> {
        vec![Signal {
            label: "Seller website check".into(),
            sub: "No website was found for this supplier on this platform.".into(),
            value: "No website found".into(),
            signal_type: "info".into(),
            category: "website".into(),
            check_type: "existence".into(),
        }]
    }

    #[test]
    fn a_found_site_with_the_company_name_is_a_possible_website() {
        let mut cards = no_website_card();
        apply_found_website(
            &mut cards,
            Some("Golden Steel Mills"),
            &[
                "https://www.facebook.com/goldensteelmillsofficial/",
                "https://www.exportersindia.com/golden-steel-mills/",
                "https://goldensteelmill.com/about-us/",
            ],
        );
        assert_eq!(
            (cards[0].value.as_str(), cards[0].signal_type.as_str()),
            (POSSIBLE_WEBSITE, "info")
        );
        assert!(cards[0].sub.contains("found goldensteelmill.com"));
        assert!(
            cards[0]
                .sub
                .contains("https://goldensteelmill.com/about-us/")
        );
    }

    #[test]
    fn a_site_missing_part_of_the_name_or_on_another_site_is_ignored() {
        for link in [
            "https://goldensteel.com/",                           // "mill" missing
            "https://goldensteelmills.tradeindia.com/",           // a page on another site
            "https://www.facebook.com/goldensteelmills/",         // a social network
            "https://www.exportersindia.com/golden-steel-mills/", // name only in the path
        ] {
            let mut cards = no_website_card();
            apply_found_website(&mut cards, Some("Golden Steel Mills"), &[link]);
            assert_eq!(cards[0].value, "No website found", "{link}");
        }
    }

    #[test]
    fn legal_endings_and_country_domains_are_handled() {
        let mut cards = no_website_card();
        apply_found_website(
            &mut cards,
            Some("Loyal Vina Co., Ltd"),
            &["http://www.loyalvina.com.vn/en/"],
        );
        assert_eq!(cards[0].value, POSSIBLE_WEBSITE);
        assert!(cards[0].sub.contains("found loyalvina.com.vn"));
    }

    #[test]
    fn a_website_from_the_listing_is_never_replaced() {
        let mut cards = no_website_card();
        cards[0].value = "Website found".into();
        apply_found_website(
            &mut cards,
            Some("Golden Steel Mills"),
            &["https://goldensteelmill.com/"],
        );
        assert_eq!(cards[0].value, "Website found");
        let mut cards = no_website_card();
        apply_found_website(&mut cards, None, &["https://goldensteelmill.com/"]);
        assert_eq!(cards[0].value, "No website found");
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
    fn tradewheel_tier_is_info_never_verified_or_caution() {
        let gold = build_b2b_verification_signal(&supplier("tradewheel", false, Some("Gold")));
        assert_eq!(
            (gold.value.as_str(), gold.signal_type.as_str()),
            ("Gold member", "info")
        );
        assert!(gold.sub.contains("paid membership level"));
        let none = build_b2b_verification_signal(&supplier("tradewheel", false, None));
        assert_eq!(
            (none.value.as_str(), none.signal_type.as_str()),
            ("No badge", "info")
        );
    }

    #[test]
    fn b2bmap_plan_is_info_never_verified_or_caution() {
        let free = build_b2b_verification_signal(&supplier("b2bmap", false, Some("Free Member")));
        assert_eq!(
            (free.value.as_str(), free.signal_type.as_str()),
            ("Free Member", "info")
        );
        assert!(free.sub.contains("b2bmap does not verify companies"));
        // Even if a badge flag were set, b2bmap is never shown as verified.
        let paid = build_b2b_verification_signal(&supplier("b2bmap", true, Some("Gold Member")));
        assert_eq!(
            (paid.value.as_str(), paid.signal_type.as_str()),
            ("Gold Member", "info")
        );
        let none = build_b2b_verification_signal(&supplier("b2bmap", false, None));
        assert_eq!(
            (none.value.as_str(), none.signal_type.as_str()),
            ("Not offered", "info")
        );
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
