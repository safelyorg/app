//! USA: SEC EDGAR, the US stock market regulator's list of companies
//! listed on the US stock market. Free, no key, but the SEC asks every
//! program to send a contact email with each request
//! (SEC_CONTACT_EMAIL in .env).
//!
//! Safely downloads the SEC's list of listed companies once a day
//! (https://www.sec.gov/files/company_tickers.json, about 10,000
//! companies) and keeps it in memory. A company matches only when its
//! WHOLE name is the seller's name, after legal forms ("Inc.", "Co.,
//! Ltd.") and punctuation are removed (the same rule as the sanctions
//! and GLEIF checks).
//!
//! - Listed: a good sign, with a reminder that scammers sometimes use
//!   the name of a big company, so the website and email should be the
//!   company's own.
//! - Not listed: information only, and shown only for US sellers - most
//!   companies are not on the stock market.
//!
//! Off unless REGISTERS_CHECK_ENABLED is "true" and SEC_CONTACT_EMAIL is
//! set in .env. When the SEC does not answer, no card is shown.

use crate::{
    models::analysis::Signal,
    services::{
        shared::standards::databases_for_country,
        sources::databases::{
            OUTSIDE_RECORDS, numbers::registers_enabled, sanctions::compared_name,
        },
    },
};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// The card's name, as in standards/databases/global.json.
pub const LABEL: &str = "US stock market (SEC)";
pub const LISTED: &str = "Listed company";
pub const NOT_LISTED: &str = "Not on the stock market";

const TICKERS_URL: &str = "https://www.sec.gov/files/company_tickers.json";

/// How old the list may get before it is downloaded again.
const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// After a download (even a failed one), the next waits at least this long.
const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);

/// One company on the SEC's list.
#[derive(Debug, Clone, PartialEq)]
pub struct ListedCompany {
    pub name: String,
    /// The SEC's number for the company.
    pub cik: u64,
    /// Its stock market codes ("AAPL").
    pub tickers: Vec<String>,
}

/// The list, by compared name.
pub type ListedIndex = HashMap<String, Vec<ListedCompany>>;

/// Reads company_tickers.json:
/// {"0": {"cik_str": 320193, "ticker": "AAPL", "title": "Apple Inc."}, ...}.
/// A company with several codes is one company.
pub fn index_from(answer: &Value) -> ListedIndex {
    let mut index: ListedIndex = HashMap::new();
    let Some(rows) = answer.as_object() else {
        return index;
    };
    for row in rows.values() {
        let name = row
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let Some(cik) = row.get("cik_str").and_then(Value::as_u64) else {
            continue;
        };
        let ticker = row
            .get("ticker")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let Some(key) = compared_name(name) else {
            continue;
        };
        let companies = index.entry(key).or_default();
        match companies.iter_mut().find(|c| c.cik == cik) {
            Some(c) => {
                if !ticker.is_empty() && !c.tickers.iter().any(|t| t == ticker) {
                    c.tickers.push(ticker.to_string());
                }
            }
            None => companies.push(ListedCompany {
                name: name.to_string(),
                cik,
                tickers: if ticker.is_empty() {
                    Vec::new()
                } else {
                    vec![ticker.to_string()]
                },
            }),
        }
    }
    index
}

/// The companies whose whole name is the seller's name.
pub fn find<'a>(index: &'a ListedIndex, company_name: &str) -> &'a [ListedCompany] {
    compared_name(company_name)
        .and_then(|key| index.get(&key))
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn is_us_seller(country: Option<&str>) -> bool {
    country
        .and_then(databases_for_country)
        .is_some_and(|c| c.country == "usa")
}

/// The card, or None when the company is not listed and the seller is
/// not in the USA (nothing worth showing).
pub fn sec_card(
    company_name: &str,
    country: Option<&str>,
    found: &[ListedCompany],
) -> Option<Signal> {
    let (value, signal_type, sub) = match found.first() {
        Some(c) => {
            let codes = if c.tickers.is_empty() {
                String::new()
            } else {
                format!(", stock code {}", c.tickers.join(", "))
            };
            (
                LISTED,
                "good",
                format!(
                    "\"{}\" (SEC number {}{codes}) is a company listed on the US stock market (SEC EDGAR: https://www.sec.gov/edgar/browse/?CIK={}). Scammers sometimes use a big company's name, so check that the supplier's website and email are the company's own.",
                    c.name, c.cik, c.cik
                ),
            )
        }
        None if is_us_seller(country) => (
            NOT_LISTED,
            "info",
            format!(
                "\"{company_name}\" is not a company listed on the US stock market (SEC EDGAR). Most companies are not listed, so this is not a warning."
            ),
        ),
        None => return None,
    };
    Some(Signal {
        label: LABEL.to_string(),
        sub,
        value: value.to_string(),
        signal_type: signal_type.to_string(),
        category: OUTSIDE_RECORDS.to_string(),
        check_type: "existence".to_string(),
    })
}

/// The contact email the SEC asks for, or None (then no check).
fn contact_email() -> Option<String> {
    std::env::var("SEC_CONTACT_EMAIL")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| v.contains('@'))
}

static INDEX: RwLock<Option<(Instant, Arc<ListedIndex>)>> = RwLock::new(None);
static DOWNLOADING: AtomicBool = AtomicBool::new(false);
static LAST_DOWNLOAD: Mutex<Option<Instant>> = Mutex::new(None);

fn may_download_now() -> bool {
    let Ok(mut last) = LAST_DOWNLOAD.lock() else {
        return false;
    };
    if last.is_some_and(|t| t.elapsed() < RETRY_AFTER) {
        return false;
    }
    *last = Some(Instant::now());
    true
}

async fn download(email: &str) -> Option<ListedIndex> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(format!("Safely company check (safely.sh) {email}"))
        .build()
        .ok()?;
    match client.get(TICKERS_URL).send().await {
        Ok(r) if r.status().is_success() => match r.json::<Value>().await {
            Ok(answer) => {
                let index = index_from(&answer);
                eprintln!(
                    "Safely: SEC list of US listed companies read ({} names)",
                    index.len()
                );
                (!index.is_empty()).then_some(index)
            }
            Err(e) => {
                eprintln!("Safely: SEC list could not be read: {}", e);
                None
            }
        },
        Ok(r) => {
            eprintln!("Safely: SEC answered {}", r.status());
            None
        }
        Err(e) => {
            eprintln!("Safely: SEC could not be reached: {}", e);
            None
        }
    }
}

/// The list in memory, downloaded first when there is none or it is a
/// day old. While another scan downloads it, the old list (or none) is used.
async fn current_index(email: &str) -> Option<Arc<ListedIndex>> {
    let held = INDEX.read().ok().and_then(|i| i.clone());
    let fresh = held.as_ref().is_some_and(|(t, _)| t.elapsed() < MAX_AGE);
    if fresh || DOWNLOADING.load(Ordering::SeqCst) || !may_download_now() {
        return held.map(|(_, i)| i);
    }
    DOWNLOADING.store(true, Ordering::SeqCst);
    let downloaded = download(email).await;
    DOWNLOADING.store(false, Ordering::SeqCst);
    match downloaded {
        Some(index) => {
            let index = Arc::new(index);
            if let Ok(mut slot) = INDEX.write() {
                *slot = Some((Instant::now(), index.clone()));
            }
            Some(index)
        }
        None => held.map(|(_, i)| i),
    }
}

/// The "US stock market (SEC)" card, or None when there is no name, the
/// check is off, or the SEC's list could not be read.
pub async fn sec_signal(company_name: Option<&str>, country: Option<&str>) -> Option<Signal> {
    let name = company_name.map(str::trim).filter(|n| !n.is_empty())?;
    if !registers_enabled() {
        return None;
    }
    let email = contact_email()?;
    let index = current_index(&email).await?;
    sec_card(name, country, find(&index, name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn list() -> ListedIndex {
        index_from(&json!({
            "0": { "cik_str": 320193, "ticker": "AAPL", "title": "Apple Inc." },
            "1": { "cik_str": 1577552, "ticker": "BABA", "title": "Alibaba Group Holding Ltd" },
            "2": { "cik_str": 1577552, "ticker": "BABAF", "title": "Alibaba Group Holding Ltd" },
            "3": { "cik_str": 99, "ticker": "", "title": "" }
        }))
    }

    #[test]
    fn the_list_is_read_one_company_with_all_its_codes() {
        let index = list();
        let found = find(&index, "ALIBABA GROUP HOLDING LIMITED");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].cik, 1577552);
        assert_eq!(found[0].tickers, vec!["BABA", "BABAF"]);
        assert_eq!(find(&index, "Apple, Inc").len(), 1);
        // Only the whole name counts.
        assert!(find(&index, "Apple Trading Co., Ltd.").is_empty());
        assert!(find(&index, "Alibaba").is_empty());
    }

    #[test]
    fn listed_is_good_and_not_listed_only_shows_for_us_sellers() {
        let index = list();
        let card = sec_card(
            "Apple Inc",
            Some("Shenzhen, China"),
            find(&index, "Apple Inc"),
        )
        .unwrap();
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (LISTED, "good")
        );
        assert_eq!(card.category, OUTSIDE_RECORDS);
        assert!(card.sub.contains("stock code AAPL"));
        assert!(card.sub.contains("CIK=320193"));
        assert!(card.sub.contains("website and email are the company's own"));

        let card = sec_card("Acme Tools LLC", Some("Texas, USA"), &[]).unwrap();
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (NOT_LISTED, "info")
        );
        assert!(sec_card("Acme Tools LLC", Some("Guangdong, China"), &[]).is_none());
        assert!(sec_card("Acme Tools LLC", None, &[]).is_none());
    }

    #[tokio::test]
    async fn the_check_needs_its_switch_and_a_contact_email() {
        if registers_enabled() && contact_email().is_some() {
            return;
        }
        assert!(sec_signal(Some("Apple Inc"), None).await.is_none());
    }
}
