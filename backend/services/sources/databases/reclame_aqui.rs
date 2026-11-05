//! Reclame Aqui, Brazil's biggest complaint site. Each company page
//! gives its reputation ("Não recomendada", "Ótimo"...), its complaints
//! and how many were answered and solved - in the page's own <meta>
//! tags:
//!   <meta name="reclameaqui:company-name" content="Americanas - Loja Física">
//!   <meta name="reclameaqui:company-cnpj" content="33014556000196">
//!   <meta name="reclameaqui:reputation-status" content="NOT_RECOMMENDED">
//!   <meta name="reclameaqui:reputation-score" content="4.1"> ...
//!
//! Reclame Aqui's own search refuses many visitors, and the site refuses
//! many non-Brazilian addresses (Bilal could only open it through a VPN,
//! Nov 5). So:
//! 1. one Google search (Serper) finds the company's page
//!    (reclameaqui.com.br/empresa/{name}/);
//! 2. the page is read through ScraperAPI from a Brazilian address
//!    (scraper_client: platform "reclameaqui").
//!
//! The page counts only when the seller's CNPJ is the page's CNPJ, or
//! (no CNPJ given) its WHOLE name is the page's name.
//!
//! - "Não recomendada": a warning. "Ruim": a smaller warning.
//! - "Regular", or too few reviews to have a grade: information only.
//! - "Bom", "Ótimo", "RA1000": a good sign.
//! - Not on Reclame Aqui: "Not found", information only (many real
//!   companies are not there). Only when every page found could be read.
//!
//! Asked for Brazil sellers and for any seller that gives a CNPJ. Each
//! answer is kept 7 days. Off unless RECLAME_AQUI_CHECK_ENABLED is
//! "true" in .env.

use crate::{
    models::analysis::Signal,
    services::{
        features::osint::run_serper_search,
        shared::{
            scraper_client::{
                build_scraper_client, scraperapi_key_is_set, wrap_scraper_url_for_platform,
            },
            standards::databases_for_country,
        },
        sources::databases::{
            OUTSIDE_RECORDS, brazil, not_found_card,
            numbers::{normalize, without_accents},
            sanctions::compared_name,
        },
    },
};
use regex::Regex;
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

/// The card's name, as in standards/databases/global.json.
pub const LABEL: &str = "Reclame Aqui";
pub const NOT_RECOMMENDED: &str = "Not recommended";
pub const BAD: &str = "Bad reputation";
pub const REGULAR: &str = "Regular reputation";
pub const GOOD: &str = "Good reputation";
pub const GREAT: &str = "Great reputation";
pub const RA1000: &str = "RA1000 reputation";
pub const NO_GRADE: &str = "Not enough reviews";

const PLATFORM: &str = "reclameaqui";

/// What a Reclame Aqui company page says.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RaCompany {
    pub url: String,
    pub name: String,
    /// 14 digits, or empty.
    pub cnpj: String,
    /// "NOT_RECOMMENDED", "BAD", "REGULAR", "GOOD", "GREAT", "RA1000"...
    pub status: String,
    /// "4.1" (out of 10).
    pub score: String,
    pub complaints: String,
    pub answered: String,
    pub solved: String,
    pub deal_again: String,
}

static META: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<meta\s+name="reclameaqui:([a-z-]+)"\s+content="([^"]*)""#).unwrap()
});

fn unescape(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

/// The company in a Reclame Aqui page, or None when the page has no
/// company tags (another page, a robot check...).
pub fn read_page(url: &str, page: &str) -> Option<RaCompany> {
    let tags: HashMap<String, String> = META
        .captures_iter(page)
        .map(|c| (c[1].to_string(), unescape(c[2].trim())))
        .collect();
    let name = tags
        .get("company-name")
        .cloned()
        .filter(|n| !n.is_empty())?;
    let get = |key: &str| tags.get(key).cloned().unwrap_or_default();
    Some(RaCompany {
        url: url.to_string(),
        name,
        cnpj: normalize(&get("company-cnpj")),
        status: get("reputation-status").to_ascii_uppercase(),
        score: get("reputation-score"),
        complaints: get("total-complaints"),
        answered: get("response-rate"),
        solved: get("solved-rate"),
        deal_again: get("deal-again-rate"),
    })
}

/// true for a Reclame Aqui company page address
/// (https://www.reclameaqui.com.br/empresa/{name}/, nothing after it).
pub fn is_company_page(link: &str) -> bool {
    let Some(rest) = link
        .strip_prefix("https://www.reclameaqui.com.br/empresa/")
        .or_else(|| link.strip_prefix("https://reclameaqui.com.br/empresa/"))
    else {
        return false;
    };
    let slug = rest.trim_end_matches('/');
    !slug.is_empty() && !slug.contains('/') && !slug.contains('?')
}

/// The seller's valid CNPJs (14 digits).
fn seller_cnpjs(numbers: &[String]) -> Vec<String> {
    brazil::check_numbers(numbers)
        .into_iter()
        .filter(|c| c.valid)
        .map(|c| normalize(&c.number))
        .collect()
}

/// true when the page is the seller's: same CNPJ, or (no CNPJ to
/// compare) the same whole name.
pub fn is_the_seller(company_name: &str, cnpjs: &[String], page: &RaCompany) -> bool {
    if !cnpjs.is_empty() && !page.cnpj.is_empty() {
        return cnpjs.contains(&page.cnpj);
    }
    // Accents left out too: "Física" and "Fisica" are the same name.
    match (
        compared_name(&without_accents(company_name)),
        compared_name(&without_accents(&page.name)),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn details(c: &RaCompany) -> String {
    let mut parts = Vec::new();
    if !c.score.is_empty() {
        parts.push(format!("score {}/10", c.score));
    }
    if !c.complaints.is_empty() {
        parts.push(format!("{} complaints", c.complaints));
    }
    if !c.answered.is_empty() {
        parts.push(format!("{}% answered", c.answered));
    }
    if !c.solved.is_empty() {
        parts.push(format!("{}% solved", c.solved));
    }
    if !c.deal_again.is_empty() {
        parts.push(format!("{}% would buy again", c.deal_again));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" ({})", parts.join(", "))
    }
}

/// The card for the seller's Reclame Aqui page.
pub fn ra_card(c: &RaCompany) -> Signal {
    let (value, signal_type, grade) = match c.status.as_str() {
        "NOT_RECOMMENDED" => (
            NOT_RECOMMENDED,
            "caution",
            "\"Não recomendada\" (not recommended)",
        ),
        "BAD" => (BAD, "caution", "\"Ruim\" (bad)"),
        "REGULAR" => (REGULAR, "info", "\"Regular\""),
        "GOOD" => (GOOD, "good", "\"Bom\" (good)"),
        "GREAT" => (GREAT, "good", "\"Ótimo\" (great)"),
        "RA1000" => (RA1000, "good", "\"RA1000\" (the best grade)"),
        _ => (NO_GRADE, "info", "no grade yet (too few reviews)"),
    };
    let advice = match signal_type {
        "caution" => {
            " Read the latest complaints before paying, and prefer a payment method that protects you."
        }
        "good" => " Buyers who complained mostly got an answer.",
        _ => "",
    };
    Signal {
        label: LABEL.to_string(),
        sub: format!(
            "\"{}\" has the reputation {grade} on Reclame Aqui, Brazil's biggest complaint site{}.{advice} Page: {}",
            c.name,
            details(c),
            c.url
        ),
        value: value.to_string(),
        signal_type: signal_type.to_string(),
        category: OUTSIDE_RECORDS.to_string(),
        check_type: "existence".to_string(),
    }
}

fn enabled() -> bool {
    std::env::var("RECLAME_AQUI_CHECK_ENABLED")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(false)
}

fn is_brazil_seller(country: Option<&str>) -> bool {
    country
        .and_then(databases_for_country)
        .is_some_and(|c| c.country == "brazil")
}

/// Answers kept 7 days: (when, the card or None when not found).
const KEEP_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// One kept answer: when, and the card (None: not found).
type Kept = (Instant, Option<Signal>);
static KEPT: Mutex<Option<HashMap<String, Kept>>> = Mutex::new(None);

async fn open_page(url: &str) -> Option<String> {
    let response = build_scraper_client()
        .get(wrap_scraper_url_for_platform(url, PLATFORM))
        .timeout(Duration::from_secs(40))
        .send()
        .await;
    match response {
        Ok(r) if r.status().is_success() => r.text().await.ok(),
        Ok(r) => {
            eprintln!("Safely: Reclame Aqui page answered {}: {url}", r.status());
            None
        }
        Err(e) => {
            eprintln!(
                "Safely: Reclame Aqui page could not be read: {}",
                e.without_url()
            );
            None
        }
    }
}

/// Finds and reads the seller's page. Some(None): searched, not there.
/// None: the search did not answer (nothing kept).
async fn look_up(name: &str, cnpjs: &[String]) -> Option<Option<Signal>> {
    let query = match cnpjs.first() {
        Some(cnpj) => format!("site:reclameaqui.com.br/empresa \"{name}\" OR \"{cnpj}\""),
        None => format!(
            "site:reclameaqui.com.br/empresa \"{}\"",
            name.replace('"', " ")
        ),
    };
    let found = run_serper_search(&query, Some("br")).await?;
    let links: Vec<String> = found
        .organic
        .iter()
        .map(|r| r.link.clone())
        .filter(|l| is_company_page(l))
        .take(3)
        .collect();
    let mut all_read = true;
    for link in links {
        let Some(page) = open_page(&link).await else {
            all_read = false;
            continue;
        };
        match read_page(&link, &page) {
            Some(company) if is_the_seller(name, cnpjs, &company) => {
                return Some(Some(ra_card(&company)));
            }
            Some(_) => {}
            None => {
                all_read = false;
                eprintln!("Safely: Reclame Aqui page had no company data: {link}");
            }
        }
    }
    // A page that could not be read might have been the seller's: no
    // card, and not kept, so the next scan tries again.
    if !all_read {
        return None;
    }
    Some(Some(not_found_card(LABEL, not_found_text(name))))
}

/// "Not found" on Reclame Aqui.
pub fn not_found_text(company_name: &str) -> String {
    format!(
        "\"{company_name}\" has no company page on Reclame Aqui. Many companies, mostly those that do not sell to shoppers, are not there, so this is not a warning."
    )
}

/// The "Reclame Aqui" card, or None when the check is off, the seller is
/// neither Brazilian nor gives a CNPJ, or it is not on Reclame Aqui.
pub async fn ra_signal(
    company_name: Option<&str>,
    country: Option<&str>,
    numbers: &[String],
) -> Option<Signal> {
    let name = company_name.map(str::trim).filter(|n| !n.is_empty())?;
    if !enabled() || !scraperapi_key_is_set() {
        return None;
    }
    let cnpjs = seller_cnpjs(numbers);
    if cnpjs.is_empty() && !is_brazil_seller(country) {
        return None;
    }
    let key = format!("{}|{}", compared_name(name)?, cnpjs.join(","));
    if let Some((when, card)) = KEPT
        .lock()
        .ok()
        .and_then(|m| m.as_ref().and_then(|m| m.get(&key).cloned()))
        && when.elapsed() < KEEP_FOR
    {
        return card;
    }
    let card = look_up(name, &cnpjs).await?;
    if let Ok(mut m) = KEPT.lock() {
        let m = m.get_or_insert_with(HashMap::new);
        m.retain(|_, (when, _)| when.elapsed() < KEEP_FOR);
        m.insert(key, (Instant::now(), card.clone()));
    }
    card
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The <meta> tags of the real page Bilal saved (Nov 5).
    const AMERICANAS: &str = r#"<html><head>
<meta name="reclameaqui:company-name" content="Americanas - Loja Física">
<meta name="reclameaqui:company-shortname" content="lojas-americanas-loja-fisica">
<meta name="reclameaqui:company-id" content="31900">
<meta name="reclameaqui:company-cnpj" content="33014556000196">
<meta name="reclameaqui:company-status" content="ACTIVE">
<meta name="reclameaqui:reputation-score" content="4.1">
<meta name="reclameaqui:total-complaints" content="2331">
<meta name="reclameaqui:total-answers" content="2045">
<meta name="reclameaqui:total-ratings" content="763">
<meta name="reclameaqui:response-rate" content="87.7">
<meta name="reclameaqui:solved-rate" content="33.9">
<meta name="reclameaqui:deal-again-rate" content="27.8">
<meta name="reclameaqui:reputation-status" content="NOT_RECOMMENDED">
</head><body></body></html>"#;

    const URL: &str = "https://www.reclameaqui.com.br/empresa/lojas-americanas-loja-fisica/";

    #[test]
    fn the_page_tags_are_read() {
        let c = read_page(URL, AMERICANAS).unwrap();
        assert_eq!(c.name, "Americanas - Loja Física");
        assert_eq!(c.cnpj, "33014556000196");
        assert_eq!(c.status, "NOT_RECOMMENDED");
        assert_eq!((c.score.as_str(), c.complaints.as_str()), ("4.1", "2331"));
        assert_eq!(
            (
                c.answered.as_str(),
                c.solved.as_str(),
                c.deal_again.as_str()
            ),
            ("87.7", "33.9", "27.8")
        );
        // A robot-check page has no company tags.
        assert!(
            read_page(
                URL,
                "<html><head><title>Just a moment...</title></head></html>"
            )
            .is_none()
        );
    }

    #[test]
    fn only_company_pages_count() {
        assert!(is_company_page(URL));
        assert!(is_company_page(
            "https://www.reclameaqui.com.br/empresa/americanas-com"
        ));
        assert!(!is_company_page(
            "https://www.reclameaqui.com.br/empresa/americanas-com/lista-reclamacoes/"
        ));
        assert!(!is_company_page(
            "https://www.reclameaqui.com.br/americanas/produto-quebrado_abc/"
        ));
        assert!(!is_company_page("https://fake.com/empresa/americanas/"));
    }

    #[test]
    fn the_cnpj_decides_when_there_is_one() {
        let c = read_page(URL, AMERICANAS).unwrap();
        // Same CNPJ, even with another name.
        assert!(is_the_seller(
            "Lojas Americanas S.A.",
            &["33014556000196".into()],
            &c
        ));
        // Another CNPJ: not the seller, even with the same name.
        assert!(!is_the_seller(
            "Americanas - Loja Física",
            &["11222333000181".into()],
            &c
        ));
        // No CNPJ: the whole name decides.
        assert!(is_the_seller("Americanas Loja Fisica", &[], &c));
        assert!(!is_the_seller("Americanas", &[], &c));
    }

    #[test]
    fn grades_become_cards() {
        let mut c = read_page(URL, AMERICANAS).unwrap();
        let card = ra_card(&c);
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (NOT_RECOMMENDED, "caution")
        );
        assert_eq!(card.category, OUTSIDE_RECORDS);
        assert!(card.sub.contains(
            "score 4.1/10, 2331 complaints, 87.7% answered, 33.9% solved, 27.8% would buy again"
        ));
        assert!(card.sub.contains(URL));
        for (status, value, kind) in [
            ("BAD", BAD, "caution"),
            ("REGULAR", REGULAR, "info"),
            ("GOOD", GOOD, "good"),
            ("GREAT", GREAT, "good"),
            ("RA1000", RA1000, "good"),
            ("NO_INDEX", NO_GRADE, "info"),
        ] {
            c.status = status.to_string();
            let card = ra_card(&c);
            assert_eq!(
                (card.value.as_str(), card.signal_type.as_str()),
                (value, kind),
                "{status}"
            );
        }
    }

    #[test]
    fn not_found_is_a_note() {
        let t = not_found_text("Baseativa");
        assert!(t.starts_with("\"Baseativa\" has no company page on Reclame Aqui."));
        assert!(t.contains("not a warning"));
    }

    #[tokio::test]
    async fn only_brazil_sellers_or_a_cnpj_and_only_when_switched_on() {
        assert!(
            ra_signal(Some("Acme"), Some("Guangdong, China"), &[])
                .await
                .is_none()
        );
        if enabled() && scraperapi_key_is_set() {
            return;
        }
        assert!(
            ra_signal(Some("Americanas"), Some("São Paulo, Brazil"), &[])
                .await
                .is_none()
        );
    }
}
