//! Sellers' own shop websites (not a marketplace): the "Check this
//! website" scan. The extension sends a few of the shop's own pages
//! (home, about, contact, shipping, returns, terms, privacy), read in the
//! buyer's browser after they click the button. Safely then:
//! - reads emails, phone links, company numbers and policies itself
//!   (pages.rs) and asks the AI the rest (ai.rs);
//! - checks the domain: a copied brand name, its age (WHOIS / RDAP), and
//!   whether it can receive email (DNS);
//! - runs the outside records (sanctions, GLEIF, company number, official
//!   register) - the same checker as marketplace scans.
//!
//! What it reads and checks, and the points of each card, are in
//! backend/standards/websites.json (build step 1 cards).

pub mod ai;
pub mod pages;

use crate::{
    errors::analyze::AnalyzeError,
    models::analysis::Signal,
    services::{
        features::{
            analysis::serper_enabled,
            domain_age::{LookupError, lookup, normalize_domain},
            fraud_reports::{report_floor, report_points},
            network_memory::build_network_memory_signal,
            osint::{PlatformCheckResult, build_social_presence_matrix},
            risk_factors::{
                NETWORK_REPORTS_FACTOR, derive_risk_factors, not_enough_information_factor,
            },
        },
        shared::{claude::send_with_photo_fallback, standards},
        sources::databases::{OUTSIDE_RECORDS, SellerFacts, check_outside_records},
    },
};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::time::Duration;
use uuid::Uuid;

/// The platform name website checks are saved under.
pub const PLATFORM: &str = "website";

/// Most pages read from one shop (standards/websites.json "max_pages").
pub const MAX_PAGES: usize = 12;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PageLink {
    pub href: String,
    #[serde(default)]
    pub text: String,
}

/// One page of the shop, as the extension read it.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebsitePage {
    pub url: String,
    #[serde(default)]
    pub title: String,
    /// The text the page shows.
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub links: Vec<PageLink>,
    /// Product photo web addresses found on the page (e.g. `<img>`
    /// tags), for the "Image authenticity" check. `#[serde(default)]`
    /// so older extension versions that do not send this yet still
    /// deserialize - the check then honestly reads "not verified": no
    /// photos were found, same as a marketplace listing with none.
    #[serde(default)]
    pub images: Vec<String>,
}

/// POST /api/v1/check-website
#[derive(Debug, Deserialize)]
pub struct WebsiteCheckRequest {
    /// The page the buyer was on.
    pub url: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub pages: Vec<WebsitePage>,
}

/// Only the shop's own pages are read (the same domain, or a part of
/// it), at most MAX_PAGES.
pub fn own_pages(pages: &[WebsitePage], domain: &str) -> Vec<WebsitePage> {
    pages
        .iter()
        .filter(|p| {
            normalize_domain(&p.url)
                .is_some_and(|d| d == domain || d.ends_with(&format!(".{domain}")))
        })
        .take(MAX_PAGES)
        .cloned()
        .collect()
}

/// For checking what the extension read from a shop: when
/// SAFELY_SAVE_WEBSITE_PAGES=true, the pages sent for a website check
/// are saved to website_pages/<domain>.json (next to where the backend
/// runs). Off by default - only for testing, never in production.
pub fn save_pages_for_testing(domain: &str, pages: &[WebsitePage]) {
    let on = std::env::var("SAFELY_SAVE_WEBSITE_PAGES")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(false);
    if !on {
        return;
    }
    let file = format!(
        "website_pages/{}.json",
        domain.replace(['/', '\\', ':'], "_")
    );
    let saved = std::fs::create_dir_all("website_pages")
        .map_err(|e| e.to_string())
        .and_then(|_| serde_json::to_string_pretty(pages).map_err(|e| e.to_string()))
        .and_then(|json| std::fs::write(&file, json).map_err(|e| e.to_string()));
    match saved {
        Ok(()) => eprintln!(
            "Safely: saved the {} pages read from {domain} to {file}",
            pages.len()
        ),
        Err(e) => eprintln!("Safely: could not save the pages read from {domain}: {e}"),
    }
}

/// Product photo addresses found across the pages read, for "Image
/// authenticity" - up to 3, the same cap a marketplace scan uses.
/// De-duplicated while keeping the order they were found in (the home
/// page's own photos first), not a HashSet's arbitrary order.
pub fn collect_image_urls(pages: &[WebsitePage]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    pages
        .iter()
        .flat_map(|p| p.images.iter().cloned())
        .filter(|url| !is_icon_or_tracker(url))
        .filter(|url| seen.insert(url.clone()))
        .take(3)
        .collect()
}

/// A drawing (.svg: an icon, never a product photo, and the AI cannot
/// read it) or an image made by a script (.php: a tracker, e.g.
/// webtraxs.php on xtoinc.com). The extension skips these too; this
/// also covers an older extension that still sends them.
fn is_icon_or_tracker(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or("").to_lowercase();
    path.ends_with(".svg") || path.ends_with(".php")
}

fn card(label: &str, value: &str, signal_type: &str, sub: String, category: &str) -> Signal {
    Signal {
        label: label.to_string(),
        sub,
        value: value.to_string(),
        signal_type: signal_type.to_string(),
        category: category.to_string(),
        check_type: "existence".to_string(),
    }
}

/// "Domain check": does the domain copy a famous brand?
pub fn domain_card(domain: &str) -> Signal {
    match pages::copied_brand(domain) {
        Some(brand) => card(
            "Domain check",
            &format!("Looks like {brand}"),
            "caution",
            format!(
                "\"{domain}\" uses the name of {brand} but is not {brand}'s own website. Scam shops copy famous names to look trusted - check you are on the brand's real site before paying."
            ),
            "website",
        ),
        None => card(
            "Domain check",
            "No lookalike found",
            "good",
            format!("\"{domain}\" does not copy the name of a famous brand."),
            "website",
        ),
    }
}

/// "Website age", from the domain's registration date.
pub fn age_card(
    domain: &str,
    answer: &Result<crate::models::domain_age::DomainAge, LookupError>,
) -> Signal {
    let label = "Website age";
    match answer {
        Ok(age) => match age.age_days {
            Some(days) => {
                let since = age
                    .registered_on
                    .as_deref()
                    .map(|d| format!(" on {d}"))
                    .unwrap_or_default();
                let months = (days / 30).max(1);
                let (value, kind, note) = if days < 30 {
                    (
                        "Under a month old".to_string(),
                        "caution",
                        " Most scam shops are only weeks old.",
                    )
                } else if days < 183 {
                    (
                        format!("{months} months old"),
                        "caution",
                        " A shop this new has no track record yet.",
                    )
                } else if days < 730 {
                    (format!("{months} months old"), "info", "")
                } else {
                    (format!("{} years old", days / 365), "good", "")
                };
                card(
                    label,
                    &value,
                    kind,
                    format!("The domain {domain} was registered{since}.{note}"),
                    "website",
                )
            }
            None => card(
                label,
                "Not checked",
                "info",
                format!("The registration date of {domain} is not public."),
                "website",
            ),
        },
        Err(LookupError::NotFound) => card(
            label,
            "Not found",
            "caution",
            format!("No registration record was found for {domain}."),
            "website",
        ),
        Err(LookupError::Failed) => card(
            label,
            "Not checked",
            "info",
            format!("The registration date of {domain} could not be checked right now."),
            "website",
        ),
    }
}

/// "Company email": an email on the shop's own domain is a good sign
/// (and the domain must be able to receive email); only a free address
/// (Gmail, Hotmail) is a note, never proof of a scam.
pub fn email_card(domain: &str, emails: &[String], receives_email: Option<bool>) -> Signal {
    let label = "Company email";
    let own: Vec<&String> = emails
        .iter()
        .filter(|e| pages::is_own_domain_email(e, domain))
        .collect();
    if let Some(first) = own.first() {
        if receives_email == Some(false) {
            return card(
                label,
                "Can't receive email",
                "caution",
                format!(
                    "The shop shows {first}, but {domain} is not set up to receive email, so messages to it may never arrive."
                ),
                "identity",
            );
        }
        return card(
            label,
            "On the shop's domain",
            "good",
            format!("The shop gives an email on its own domain: {first}."),
            "identity",
        );
    }
    if let Some(free) = emails.iter().find(|e| pages::is_free_email(e)) {
        return card(
            label,
            "Free email only",
            "info",
            format!(
                "The only email is {free}, a free address anyone can open. Many small shops do this; it is a note, not proof of a scam."
            ),
            "identity",
        );
    }
    if let Some(other) = emails.first() {
        return card(
            label,
            "Email on another domain",
            "info",
            format!("The email {other} is not on {domain}. Ask the shop how the two are related."),
            "identity",
        );
    }
    card(
        label,
        "No email shown",
        "info",
        "The pages read show no email address.".to_string(),
        "identity",
    )
}

/// "Shop policies": shipping, returns, terms and privacy pages with real text.
pub fn policies_card(found: &[&'static str]) -> Signal {
    let missing: Vec<&str> = pages::POLICIES
        .iter()
        .map(|p| p.name)
        .filter(|n| !found.contains(n))
        .collect();
    let total = pages::POLICIES.len();
    let (value, kind) = match found.len() {
        n if n == total => (format!("All {total} found"), "good"),
        n if n >= 2 => (format!("{n} of {total} found"), "info"),
        n => (format!("{n} of {total} found"), "caution"),
    };
    let sub = if missing.is_empty() {
        "The shop has shipping, return / refund, terms and privacy pages with real text."
            .to_string()
    } else {
        format!(
            "No page with real text was found for: {}. A real shop explains how it ships and how refunds work - ask before paying.",
            missing.join(", ")
        )
    };
    card("Shop policies", &value, kind, sub, "website")
}

/// Can the domain receive email? Asked through Google's public DNS
/// (an MX record). None when the answer could not be read.
async fn receives_email(domain: &str) -> Option<bool> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;
    let answer: serde_json::Value = client
        .get(format!("https://dns.google/resolve?name={domain}&type=MX"))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let records = answer.get("Answer").and_then(|a| a.as_array());
    Some(records.is_some_and(|r| {
        r.iter()
            .any(|x| x.get("type").and_then(|t| t.as_u64()) == Some(15))
    }))
}

/// Removes the cards a standards file switches off.
fn keep_enabled_cards(signals: &mut Vec<Signal>) {
    signals.retain(|s| {
        if s.category == OUTSIDE_RECORDS {
            standards::database_card_enabled(&s.label)
        } else {
            standards::websites().card_enabled(&s.label)
        }
    });
}

/// The website risk score, the same way as a marketplace scan but with
/// the points of standards/websites.json (outside records: databases).
pub fn website_score_from_signals(signals: &[Signal], fraud_count: i64) -> i16 {
    let scoring = &standards::websites().scoring;
    let warnings: i16 = signals
        .iter()
        .filter(|s| s.signal_type == "caution")
        .map(|s| {
            if s.category == OUTSIDE_RECORDS {
                standards::database_warning_points(&s.label, &s.value)
            } else {
                standards::websites().warning_points(&s.label, &s.value)
            }
        })
        .sum();
    let infos = signals
        .iter()
        .filter(|s| s.signal_type == "info" && s.category != OUTSIDE_RECORDS)
        .count() as i16;
    let info_points = (infos * scoring.info_points_each).min(scoring.info_points_max);
    let base = (warnings + info_points).min(100) + report_points(fraud_count);
    let factors = derive_risk_factors(signals);
    let count = |severity: &str| {
        factors
            .iter()
            .filter(|f| f.severity == severity && f.name != NETWORK_REPORTS_FACTOR)
            .count() as i16
    };
    let (serious, compound) = (count("hard"), count("compound"));
    let floor = if serious > 0 {
        67 + (serious - 1) * 10
    } else if compound > 0 {
        (45 + (compound - 1) * 10).min(66)
    } else {
        0
    };
    let mut score = base.max(floor).max(report_floor(fraud_count));
    if not_enough_information_factor(signals).is_some() {
        score = score.max(scoring.not_enough_information_floor);
    }
    score.min(100)
}

/// Everything a website check found.
pub struct WebsiteResult {
    pub signals: Vec<Signal>,
    pub risk_score: i16,
    pub notes: String,
    pub company_name: Option<String>,
    pub country: Option<String>,
    pub phone: Option<String>,
    /// What the social presence search found on each site (Facebook,
    /// LinkedIn, Trustpilot, review sites...). The extension's
    /// Intelligence tab and the dashboard draw the "Online presence"
    /// list from this; empty when SERPER_ENABLED is off.
    pub social_candidates: Vec<PlatformCheckResult>,
}

fn non_empty(text: &str) -> Option<String> {
    let t = text.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// The name the social presence search uses when the AI read no
/// company name off the site: the part of the home page title that is
/// the site's own name ("Palmer Holland | Distributor" -> "Palmer
/// Holland", "Custom Gasket... | XTO Inc." -> "XTO Inc."). When no part
/// is, the domain's name ("gloflux.com" -> "Gloflux"): a title like
/// "Custom Lithium Battery Manufacturer" is a description, and searching
/// it next to scam words finds other companies.
fn website_display_name(pages: &[WebsitePage], domain: &str) -> Option<String> {
    let squash = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let stem = domain.split('.').next().unwrap_or("").to_string();
    let stem_squashed = squash(&stem);
    let title = pages.first().map(|p| p.title.trim()).unwrap_or("");
    let own_part = title
        .split(['|', '-', '–', '—'])
        .map(str::trim)
        .find(|part| {
            let p = squash(part);
            p.len() >= 3
                && stem_squashed.len() >= 3
                && (p.contains(&stem_squashed) || stem_squashed.contains(&p))
        });
    if let Some(part) = own_part {
        return Some(part.to_string());
    }
    let mut chars = stem.chars();
    let first = chars.next()?;
    Some(first.to_uppercase().collect::<String>() + chars.as_str())
}

/// A factory or supplier's own website sells by quote: no prices and no
/// way to pay are shown (the same test as the "Before you pay" steps,
/// pdf_report.rs is_supplier_website).
fn is_supplier_site(signals: &[Signal]) -> bool {
    let not_shown = |label: &str| {
        signals
            .iter()
            .any(|s| s.label == label && s.value == "Not shown")
    };
    not_shown("Payment at checkout") && not_shown("Prices")
}

/// On a supplier's own website, missing shipping and return pages are
/// normal (each order is agreed by quote), so "Shop policies" is a note,
/// not a warning, there. It still says which pages are missing.
fn soften_policies_for_supplier_site(signals: &mut [Signal]) {
    if !is_supplier_site(signals) {
        return;
    }
    if let Some(card) = signals
        .iter_mut()
        .find(|s| s.label == "Shop policies" && s.signal_type == "caution")
    {
        card.signal_type = "info".to_string();
        card.sub.push_str(
            " This looks like a supplier's website that sells by quote, where shipping and return pages are often missing - ask for the terms in writing before you pay.",
        );
    }
}

/// Runs every website check on the pages the buyer's browser sent.
pub async fn build_website_analysis(
    pool: &Pool<Postgres>,
    domain: &str,
    pages: &[WebsitePage],
    fraud_count: i64,
    seller_id: Uuid,
) -> Result<WebsiteResult, AnalyzeError> {
    let emails = pages::emails(pages);
    let phones = pages::phone_links(pages);
    let numbers = pages::company_numbers(pages);
    let policies = pages::policies_found(pages);
    // Product photos found on the pages read, for "Image
    // authenticity" - up to 3, the same cap a marketplace scan uses.
    let image_urls = collect_image_urls(pages);

    let known_facts = ai::KnownFacts {
        domain,
        emails: &emails,
        phones: &phones,
        company_numbers: &numbers,
        policies: &policies,
    };
    let prompt = ai::website_prompt(pages, &known_facts, &image_urls);
    let has_own_email = emails.iter().any(|e| pages::is_own_domain_email(e, domain));
    let (answer, age, mx, history) = tokio::join!(
        send_with_photo_fallback(prompt, &image_urls, || {
            ai::website_prompt_photos_blocked(pages, &known_facts, &image_urls)
        }),
        lookup(domain),
        async {
            if has_own_email {
                receives_email(domain).await
            } else {
                None
            }
        },
        build_network_memory_signal(pool, seller_id),
    );
    let answer = answer.map_err(|e| AnalyzeError::ClaudeAnalysisFailed(format!("{e:?}")))?;
    let analysis: ai::WebsiteClaudeAnalysis = serde_json::from_str(&answer)
        .map_err(|e| AnalyzeError::ClaudeAnalysisFailed(e.to_string()))?;

    let mut signals: Vec<Signal> = Vec::new();
    signals.extend(history);
    signals.push(domain_card(domain));
    signals.push(age_card(domain, &age));
    signals.push(email_card(domain, &emails, mx));
    signals.push(policies_card(&policies));
    signals.extend(ai::ai_cards(&analysis));
    soften_policies_for_supplier_site(&mut signals);

    let company_name = non_empty(&analysis.company_name);
    let country = non_empty(&analysis.country);
    let phone = non_empty(&analysis.phone).or_else(|| phones.first().cloned());

    // Social presence search (Serper, costs credits): off unless
    // SERPER_ENABLED=true, the same switch a marketplace scan uses. A
    // website gives no contact name and often no phone, so when the AI
    // read no company name, the home page title is searched instead.
    let social_candidates = if serper_enabled() {
        let search_name = company_name
            .clone()
            .or_else(|| website_display_name(pages, domain));
        let (social_signal, candidates) = build_social_presence_matrix(
            search_name.as_deref(),
            None,
            country.as_deref(),
            phone.as_deref(),
            PLATFORM,
        )
        .await
        .map_err(AnalyzeError::ClaudeAnalysisFailed)?;
        signals.push(social_signal);
        candidates
    } else {
        Vec::new()
    };

    let facts = SellerFacts {
        company_name: company_name.clone(),
        country: country.clone(),
        website: Some(domain.to_string()),
        company_numbers: numbers,
    };
    signals.extend(check_outside_records(&facts).await);
    keep_enabled_cards(&mut signals);

    let risk_score = website_score_from_signals(&signals, fraud_count);
    Ok(WebsiteResult {
        signals,
        risk_score,
        notes: analysis.overall_risk_notes.clone(),
        company_name,
        country,
        phone,
        social_candidates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::domain_age::{DomainAge, Verdict};

    fn age(days: Option<i64>) -> Result<DomainAge, LookupError> {
        Ok(DomainAge {
            domain: "shop.com".into(),
            registered_on: Some("2026-10-01".into()),
            expires_on: None,
            updated_on: None,
            registrar: None,
            age_days: days,
            verdict: Verdict::New,
            expires_soon: false,
        })
    }

    fn page(url: &str) -> WebsitePage {
        WebsitePage {
            url: url.into(),
            title: String::new(),
            text: String::new(),
            links: vec![],
            images: vec![],
        }
    }

    #[test]
    fn only_the_shops_own_pages_are_read() {
        let pages = own_pages(
            &[
                page("https://www.shop.com/"),
                page("https://help.shop.com/refunds"),
                page("https://evil.com/"),
                page("https://notshop.com/"),
            ],
            "shop.com",
        );
        assert_eq!(pages.len(), 2);
    }

    #[test]
    fn image_urls_are_collected_in_order_deduped_and_capped_at_3() {
        let mut home = page("https://shop.com/");
        home.images = vec![
            "https://shop.com/a.jpg".into(),
            "https://shop.com/b.jpg".into(),
        ];
        let mut about = page("https://shop.com/about");
        about.images = vec![
            "https://shop.com/b.jpg".into(), // already seen on the home page
            "https://shop.com/c.jpg".into(),
            "https://shop.com/d.jpg".into(),
        ];
        let urls = collect_image_urls(&[home, about]);
        assert_eq!(
            urls,
            vec![
                "https://shop.com/a.jpg".to_string(),
                "https://shop.com/b.jpg".to_string(),
                "https://shop.com/c.jpg".to_string(),
            ]
        );
    }

    #[test]
    fn display_name_is_the_sites_own_name() {
        let mut home = page("https://shop.com/");
        home.title = "".into();
        assert_eq!(
            website_display_name(&[home.clone()], "gloflux.com"),
            Some("Gloflux".to_string())
        );
        assert_eq!(
            website_display_name(&[], "shop.com"),
            Some("Shop".to_string())
        );
        home.title = "Palmer Holland | Specialty Chemical Distributor".into();
        assert_eq!(
            website_display_name(&[home.clone()], "palmerholland.com"),
            Some("Palmer Holland".to_string())
        );
        home.title = "SC Fastening Systems - Home".into();
        assert_eq!(
            website_display_name(&[home.clone()], "scfastening.com"),
            Some("SC Fastening Systems".to_string())
        );
        // Real titles (Nov 3).
        home.title = "Custom Gasket, Die Cutting and Quality Fabricating | XTO Inc.".into();
        assert_eq!(
            website_display_name(&[home.clone()], "xtoinc.com"),
            Some("XTO Inc.".to_string())
        );
        home.title = "Custom Lithium Battery Manufacturer | OEM Battery Packs".into();
        assert_eq!(
            website_display_name(&[home], "gloflux.com"),
            Some("Gloflux".to_string())
        );
    }

    #[test]
    fn icons_and_trackers_are_not_photos() {
        // Real addresses (xtoinc.com, Nov 3).
        let mut home = page("https://www.xtoinc.com/");
        home.images = vec![
            "https://www.xtoinc.com/wp-content/themes/xto-default/img/ico-exit.svg".into(),
            "https://www.xtoinc.com/wp-content/uploads/xto_management_285.jpg".into(),
            "https://www.webtraxs.com/webtraxs.php?id=wt-28d89fd5&st=img".into(),
        ];
        assert_eq!(
            collect_image_urls(&[home]),
            vec!["https://www.xtoinc.com/wp-content/uploads/xto_management_285.jpg".to_string()]
        );
    }

    #[test]
    fn a_supplier_site_without_shop_policies_gets_a_note_not_a_warning() {
        let sig = |label: &str, value: &str, kind: &str| Signal {
            label: label.into(),
            sub: "No page with real text was found for: shipping policy.".into(),
            value: value.into(),
            signal_type: kind.into(),
            category: "website".into(),
            check_type: String::new(),
        };
        let mut factory = vec![
            sig("Shop policies", "1 of 4 found", "caution"),
            sig("Payment at checkout", "Not shown", "info"),
            sig("Prices", "Not shown", "info"),
        ];
        soften_policies_for_supplier_site(&mut factory);
        assert_eq!(factory[0].signal_type, "info");
        assert!(factory[0].sub.contains("sells by quote"));
        // A shop with prices keeps the warning.
        let mut shop = vec![
            sig("Shop policies", "1 of 4 found", "caution"),
            sig("Payment at checkout", "Normal", "good"),
            sig("Prices", "normal", "good"),
        ];
        soften_policies_for_supplier_site(&mut shop);
        assert_eq!(shop[0].signal_type, "caution");
    }

    #[test]
    fn domain_age_cards() {
        let c = age_card("shop.com", &age(Some(12)));
        assert_eq!(
            (c.value.as_str(), c.signal_type.as_str()),
            ("Under a month old", "caution")
        );
        assert!(c.sub.contains("registered on 2026-10-01"));
        assert_eq!(age_card("shop.com", &age(Some(100))).value, "3 months old");
        assert_eq!(age_card("shop.com", &age(Some(400))).signal_type, "info");
        assert_eq!(age_card("shop.com", &age(Some(4000))).value, "10 years old");
        assert_eq!(
            age_card("shop.com", &Err(LookupError::NotFound)).value,
            "Not found"
        );
        assert_eq!(
            age_card("shop.com", &Err(LookupError::Failed)).signal_type,
            "info"
        );
    }

    #[test]
    fn email_cards() {
        let own = vec!["sales@shop.com".to_string()];
        assert_eq!(email_card("shop.com", &own, Some(true)).signal_type, "good");
        assert_eq!(email_card("shop.com", &own, None).signal_type, "good");
        assert_eq!(
            email_card("shop.com", &own, Some(false)).value,
            "Can't receive email"
        );
        let free = vec!["shop2026@gmail.com".to_string()];
        let c = email_card("shop.com", &free, None);
        assert_eq!(
            (c.value.as_str(), c.signal_type.as_str()),
            ("Free email only", "info")
        );
        assert_eq!(email_card("shop.com", &[], None).value, "No email shown");
    }

    #[test]
    fn policy_cards() {
        let all = [
            "shipping policy",
            "return / refund policy",
            "terms",
            "privacy policy",
        ];
        assert_eq!(policies_card(&all).signal_type, "good");
        let c = policies_card(&["privacy policy", "terms"]);
        assert_eq!(
            (c.value.as_str(), c.signal_type.as_str()),
            ("2 of 4 found", "info")
        );
        assert!(c.sub.contains("shipping policy, return / refund policy"));
        assert_eq!(policies_card(&[]).signal_type, "caution");
    }

    #[test]
    fn a_copied_brand_is_a_warning() {
        let c = domain_card("amaz0n-deals.shop");
        assert_eq!(
            (c.value.as_str(), c.signal_type.as_str()),
            ("Looks like Amazon", "caution")
        );
        assert_eq!(domain_card("bellacasa.com.br").signal_type, "good");
    }

    #[test]
    fn the_score_uses_the_website_points() {
        let sig = |label: &str, value: &str, kind: &str| Signal {
            label: label.into(),
            sub: String::new(),
            value: value.into(),
            signal_type: kind.into(),
            category: "website".into(),
            check_type: String::new(),
        };
        // 25 (new domain) + 15 (policies) = 40.
        let signals = vec![
            sig("Website age", "Under a month old", "caution"),
            sig("Shop policies", "0 of 4 found", "caution"),
            sig("Domain check", "No lookalike found", "good"),
            sig("Contact info", "Given", "good"),
            sig("Business details", "Shown", "good"),
            sig("Prices", "normal", "good"),
            sig("Overall legitimacy check", "Confirmed", "good"),
            sig("Urgency language", "None found", "good"),
        ];
        assert_eq!(website_score_from_signals(&signals, 0), 40);
        // A copied brand is a "Serious" problem: High risk.
        let mut lookalike = signals.clone();
        lookalike.push(sig("Domain check", "Looks like Amazon", "caution"));
        lookalike.retain(|s| !(s.label == "Domain check" && s.signal_type == "good"));
        assert!(website_score_from_signals(&lookalike, 0) >= 67);
    }
}
