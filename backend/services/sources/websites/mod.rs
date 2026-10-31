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
            domain_age::{LookupError, lookup, normalize_domain},
            fraud_reports::{report_floor, report_points},
            network_memory::build_network_memory_signal,
            risk_factors::{
                NETWORK_REPORTS_FACTOR, derive_risk_factors, not_enough_information_factor,
            },
        },
        shared::{claude::ask_claude_text, standards},
        sources::databases::{OUTSIDE_RECORDS, SellerFacts, check_outside_records},
    },
};
use serde::Deserialize;
use sqlx::{Pool, Postgres};
use std::time::Duration;
use uuid::Uuid;

/// The platform name website checks are saved under.
pub const PLATFORM: &str = "website";

/// Most pages read from one shop (standards/websites.json "max_pages").
pub const MAX_PAGES: usize = 12;

#[derive(Debug, Clone, Deserialize)]
pub struct PageLink {
    pub href: String,
    #[serde(default)]
    pub text: String,
}

/// One page of the shop, as the extension read it.
#[derive(Debug, Clone, Deserialize)]
pub struct WebsitePage {
    pub url: String,
    #[serde(default)]
    pub title: String,
    /// The text the page shows.
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub links: Vec<PageLink>,
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
}

fn non_empty(text: &str) -> Option<String> {
    let t = text.trim();
    (!t.is_empty()).then(|| t.to_string())
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

    let prompt = ai::website_prompt(
        pages,
        &ai::KnownFacts {
            domain,
            emails: &emails,
            phones: &phones,
            company_numbers: &numbers,
            policies: &policies,
        },
    );
    let has_own_email = emails.iter().any(|e| pages::is_own_domain_email(e, domain));
    let (answer, age, mx, history) = tokio::join!(
        ask_claude_text(prompt),
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

    let company_name = non_empty(&analysis.company_name);
    let country = non_empty(&analysis.country);
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
        phone: non_empty(&analysis.phone).or_else(|| phones.first().cloned()),
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

    #[test]
    fn only_the_shops_own_pages_are_read() {
        let page = |url: &str| WebsitePage {
            url: url.into(),
            title: String::new(),
            text: String::new(),
            links: vec![],
        };
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
