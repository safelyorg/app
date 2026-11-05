//! BBB (Better Business Bureau): the rating (A+ to F) and accreditation
//! of US businesses. BBB's search page carries its results as data in
//! the page itself (window.__PRELOADED_STATE__ = {...}; seen in the
//! pages Bilal saved, Nov 5):
//!   https://www.bbb.org/search?find_text={name}&find_country=USA
//!   searchResult.results[] = { businessName ("<em>Zappos</em> Development, Inc."),
//!     rating ("A+", "" when not rated), bbbMember (accredited), city, state,
//!     reportUrl ("/us/nv/las-vegas/profile/..."), outOfBusinessStatus }
//!
//! BBB blocks many visitors with a robot check (Bilal could not get past
//! it even with a VPN until he came from Google), so the page is read
//! through ScraperAPI from a US address (scraper_client: platform "bbb").
//!
//! A business counts only when its WHOLE name is the seller's name.
//! - F: a warning. D: a smaller warning.
//! - A+ to A-: a good sign (more so when accredited).
//! - B and C, or not rated: information only.
//! - Not on BBB: "Not found", information only (asked for US sellers only).
//!
//! Asked for US sellers. Each answer is kept 7 days. Off unless
//! BBB_CHECK_ENABLED is "true" in .env.

use crate::{
    models::analysis::Signal,
    services::{
        shared::{
            scraper_client::{
                build_scraper_client, scraperapi_key_is_set, wrap_scraper_url_for_platform,
            },
            standards::databases_for_country,
        },
        sources::databases::{
            OUTSIDE_RECORDS, not_found_card, numbers::without_accents, sanctions::compared_name,
        },
    },
};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

/// The card's name, as in standards/databases/global.json.
pub const LABEL: &str = "BBB rating";
pub const RATED_A: &str = "A rating";
pub const RATED_B: &str = "B rating";
pub const RATED_C: &str = "C rating";
pub const RATED_D: &str = "D rating";
pub const RATED_F: &str = "F rating";
pub const NOT_RATED: &str = "Not rated";

const PLATFORM: &str = "bbb";

/// One business in BBB's search results.
#[derive(Debug, Clone, PartialEq)]
pub struct BbbBusiness {
    pub name: String,
    /// "A+", "B-", "F", or "" when BBB does not rate it.
    pub rating: String,
    pub accredited: bool,
    pub place: String,
    pub out_of_business: bool,
    /// Full address of its BBB profile.
    pub profile: String,
}

/// The search page's address for a name.
pub fn search_url(company_name: &str) -> String {
    format!(
        "https://www.bbb.org/search?find_text={}&find_country=USA",
        urlencoding::encode(company_name.trim())
    )
}

/// The JSON after "__PRELOADED_STATE__ =" (up to its closing "};").
fn preloaded_state(page: &str) -> Option<Value> {
    let start = page.find("__PRELOADED_STATE__")?;
    let open = start + page[start..].find('{')?;
    let script_end = open + page[open..].find("</script>")?;
    let body = page[open..script_end].trim_end();
    let body = body.strip_suffix(';').unwrap_or(body).trim_end();
    serde_json::from_str(body).ok()
}

/// "<em>Zappos</em> Development, Inc." -> "Zappos Development, Inc."
fn without_tags(text: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&#39;", "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

/// The businesses in a BBB search page, or None when the page has no
/// results data (a robot check, another page).
pub fn read_search(page: &str) -> Option<Vec<BbbBusiness>> {
    let state = preloaded_state(page)?;
    let results = state.pointer("/searchResult/results")?.as_array()?;
    Some(
        results
            .iter()
            .filter_map(|r| {
                let name = without_tags(&text(r, "businessName"));
                if name.is_empty() {
                    return None;
                }
                let place = [text(r, "city"), text(r, "state")]
                    .into_iter()
                    .filter(|p| !p.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                let report = text(r, "reportUrl");
                Some(BbbBusiness {
                    name,
                    rating: text(r, "rating").to_ascii_uppercase(),
                    accredited: r.get("bbbMember").and_then(Value::as_bool).unwrap_or(false),
                    place,
                    out_of_business: match r.get("outOfBusinessStatus") {
                        Some(Value::Bool(b)) => *b,
                        Some(Value::String(s)) => !s.trim().is_empty(),
                        _ => false,
                    },
                    profile: if report.starts_with('/') {
                        format!("https://www.bbb.org{report}")
                    } else {
                        report
                    },
                })
            })
            .collect(),
    )
}

/// The business whose whole name (accents left out) is the seller's.
pub fn find<'a>(company_name: &str, found: &'a [BbbBusiness]) -> Option<&'a BbbBusiness> {
    let wanted = compared_name(&without_accents(company_name))?;
    found
        .iter()
        .find(|b| compared_name(&without_accents(&b.name)).as_deref() == Some(wanted.as_str()))
}

/// The card for the seller's BBB entry.
pub fn bbb_card(b: &BbbBusiness) -> Signal {
    let letter = b.rating.chars().next().unwrap_or(' ');
    let (value, signal_type) = match letter {
        'A' => (RATED_A, "good"),
        'B' => (RATED_B, "info"),
        'C' => (RATED_C, "info"),
        'D' => (RATED_D, "caution"),
        'F' => (RATED_F, "caution"),
        _ => (NOT_RATED, "info"),
    };
    let rating = if b.rating.is_empty() {
        "no BBB rating".to_string()
    } else {
        format!("the BBB rating {}", b.rating)
    };
    let place = if b.place.is_empty() {
        String::new()
    } else {
        format!(" ({})", b.place)
    };
    let accredited = if b.accredited {
        " It is BBB Accredited (it agreed to BBB's standards)."
    } else {
        ""
    };
    let closed = if b.out_of_business {
        " BBB lists it as out of business."
    } else {
        ""
    };
    let advice = match signal_type {
        "caution" => {
            " A low rating usually means unanswered or unsolved complaints - read them before paying."
        }
        _ => "",
    };
    Signal {
        label: LABEL.to_string(),
        sub: format!(
            "\"{}\"{place} has {rating} from the Better Business Bureau.{accredited}{closed}{advice} Profile: {}",
            b.name, b.profile
        ),
        value: value.to_string(),
        signal_type: signal_type.to_string(),
        category: OUTSIDE_RECORDS.to_string(),
        check_type: "existence".to_string(),
    }
}

fn enabled() -> bool {
    std::env::var("BBB_CHECK_ENABLED")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(false)
}

fn is_us_seller(country: Option<&str>) -> bool {
    country
        .and_then(databases_for_country)
        .is_some_and(|c| c.country == "usa")
}

const KEEP_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// One kept answer: when, and the card (None: not found).
type Kept = (Instant, Option<Signal>);
static KEPT: Mutex<Option<HashMap<String, Kept>>> = Mutex::new(None);

/// Searches BBB. Some(None): searched, not there. None: no answer.
async fn look_up(name: &str) -> Option<Option<Signal>> {
    let url = search_url(name);
    let response = build_scraper_client()
        .get(wrap_scraper_url_for_platform(&url, PLATFORM))
        .timeout(Duration::from_secs(60))
        .send()
        .await;
    let page = match response {
        Ok(r) if r.status().is_success() => r.text().await.ok()?,
        Ok(r) => {
            eprintln!("Safely: BBB search answered {}: {url}", r.status());
            return None;
        }
        Err(e) => {
            eprintln!("Safely: BBB search could not be read: {}", e.without_url());
            return None;
        }
    };
    let Some(found) = read_search(&page) else {
        eprintln!("Safely: BBB page had no results data (robot check?): {url}");
        return None;
    };
    Some(Some(find(name, &found).map(bbb_card).unwrap_or_else(|| {
        not_found_card(
            LABEL,
            format!(
                "\"{name}\" has no Better Business Bureau profile. Many real companies are not listed there, so this is not a warning."
            ),
        )
    })))
}

/// The "BBB rating" card, or None when the check is off, the seller is
/// not in the USA, or it is not on BBB.
pub async fn bbb_signal(company_name: Option<&str>, country: Option<&str>) -> Option<Signal> {
    let name = company_name.map(str::trim).filter(|n| !n.is_empty())?;
    if !enabled() || !scraperapi_key_is_set() || !is_us_seller(country) {
        return None;
    }
    let key = compared_name(&without_accents(name))?;
    if let Some((when, card)) = KEPT
        .lock()
        .ok()
        .and_then(|m| m.as_ref().and_then(|m| m.get(&key).cloned()))
        && when.elapsed() < KEEP_FOR
    {
        return card;
    }
    let card = look_up(name).await?;
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

    /// The data part of the real search page Bilal saved ("zappos", Nov 5).
    const SEARCH: &str = r#"<html><head></head><body><script>window.__PRELOADED_STATE__ = {"user":{},"page":{},"searchResult":{"page":1,"totalResults":2,"results":[
{"id":"1086_67964_14370","businessName":"<em>Zappos</em> Development, Inc.","address":"400 Stewart Ave","city":"Las Vegas","state":"NV","postalcode":"89101-2913","tobText":"Online Shopping","rating":"A+","bbbMember":true,"reportUrl":"/us/nv/las-vegas/profile/online-shopping/zapposcom-1086-67964","businessId":"67964","ratingScore":100,"bbbId":"1086","outOfBusinessStatus":null},
{"id":"1126_1000080397_411441","businessName":"<em>Zappo</em> Electric","city":"Glendale","state":"AZ","rating":"","bbbMember":false,"reportUrl":"/us/az/glendale/profile/electrical-contractors/zappo-electric-1126-1000080397","ratingScore":0,"outOfBusinessStatus":null}
],"accredited":0}};</script></body></html>"#;

    #[test]
    fn the_search_page_data_is_read() {
        let found = read_search(SEARCH).unwrap();
        assert_eq!(found.len(), 2);
        let z = &found[0];
        assert_eq!(z.name, "Zappos Development, Inc.");
        assert_eq!((z.rating.as_str(), z.accredited), ("A+", true));
        assert_eq!(z.place, "Las Vegas, NV");
        assert_eq!(
            z.profile,
            "https://www.bbb.org/us/nv/las-vegas/profile/online-shopping/zapposcom-1086-67964"
        );
        assert!(!z.out_of_business);
        assert_eq!(found[1].rating, "");
        // A robot-check page has no data.
        assert!(read_search("<html><title>Just a moment...</title></html>").is_none());
        assert_eq!(
            search_url("Zappos Development, Inc."),
            "https://www.bbb.org/search?find_text=Zappos%20Development%2C%20Inc.&find_country=USA"
        );
    }

    #[test]
    fn whole_names_only() {
        let found = read_search(SEARCH).unwrap();
        assert_eq!(find("Zappos Development Inc", &found).unwrap().rating, "A+");
        assert_eq!(find("Zappo Electric", &found).unwrap().rating, "");
        assert!(find("Zappos", &found).is_none());
    }

    #[test]
    fn ratings_become_cards() {
        let found = read_search(SEARCH).unwrap();
        let card = bbb_card(&found[0]);
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (RATED_A, "good")
        );
        assert_eq!(card.category, OUTSIDE_RECORDS);
        assert!(card.sub.contains("(Las Vegas, NV) has the BBB rating A+"));
        assert!(card.sub.contains("BBB Accredited"));
        let card = bbb_card(&found[1]);
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (NOT_RATED, "info")
        );
        let mut b = found[0].clone();
        for (rating, value, kind) in [
            ("B-", RATED_B, "info"),
            ("C", RATED_C, "info"),
            ("D+", RATED_D, "caution"),
            ("F", RATED_F, "caution"),
        ] {
            b.rating = rating.to_string();
            let card = bbb_card(&b);
            assert_eq!(
                (card.value.as_str(), card.signal_type.as_str()),
                (value, kind),
                "{rating}"
            );
        }
    }

    #[tokio::test]
    async fn only_us_sellers_and_only_when_switched_on() {
        assert!(
            bbb_signal(Some("Acme"), Some("Guangdong, China"))
                .await
                .is_none()
        );
        if enabled() && scraperapi_key_is_set() {
            return;
        }
        assert!(
            bbb_signal(Some("Zappos Development"), Some("Nevada, USA"))
                .await
                .is_none()
        );
    }
}
