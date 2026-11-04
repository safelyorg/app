//! USA: SAM.gov, the US government's register of companies that may sell
//! to it. Free, but needs a key from a SAM.gov account
//! (SAM_GOV_API_KEY in .env). A basic key allows only 10 lookups a day
//! and stops working after about 90 days (a new one is made on the
//! SAM.gov profile page).
//!
//! So that the few lookups are not wasted:
//! - only US sellers are looked up;
//! - each answer is kept in memory for 7 days, so the same company is not
//!   asked for again;
//! - at most SAM_GOV_DAILY_LIMIT lookups a day (10 when not set), and
//!   none for the rest of the day once SAM.gov says the limit is reached.
//!
//! A company matches only when its WHOLE legal name or trade name is the
//! seller's name (legal forms and punctuation left out).
//!
//! - Active registration: a good sign.
//! - Excluded by the US government (barred from government work): a warning.
//! - Registration expired: information only (it has to be renewed every
//!   year, and many companies let it lapse).
//! - Not registered: information only - most companies never sell to
//!   the US government.
//!
//! Off unless REGISTERS_CHECK_ENABLED is "true" and SAM_GOV_API_KEY is
//! set in .env. When SAM.gov does not answer, no card is shown.

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
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// The card's name, as in standards/databases/global.json.
pub const LABEL: &str = "US government register (SAM.gov)";
pub const ACTIVE: &str = "Registered with the US government";
pub const EXCLUDED: &str = "Excluded by the US government";
pub const EXPIRED: &str = "Registration expired";
pub const NOT_REGISTERED: &str = "Not registered";

const ENTITIES_URL: &str = "https://api.sam.gov/entity-information/v3/entities";

/// How long one answer is kept.
const KEEP_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// One company from SAM.gov.
#[derive(Debug, Clone, PartialEq)]
pub struct SamEntity {
    pub legal_name: String,
    pub trade_name: String,
    /// The government's number for the company (UEI).
    pub uei: String,
    /// "Active", "Inactive", "ID Assigned".
    pub status: String,
    /// "Y" when the company is excluded from government work.
    pub excluded: bool,
    pub registered_on: String,
    pub expires_on: String,
    pub city: String,
    pub state: String,
}

fn text(value: &Value, path: &str) -> String {
    value
        .pointer(path)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

/// The companies in a SAM.gov answer.
pub fn entities_from(answer: &Value) -> Vec<SamEntity> {
    answer
        .get("entityData")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let legal_name = text(e, "/entityRegistration/legalBusinessName");
            if legal_name.is_empty() {
                return None;
            }
            Some(SamEntity {
                legal_name,
                trade_name: text(e, "/entityRegistration/dbaName"),
                uei: text(e, "/entityRegistration/ueiSAM"),
                status: text(e, "/entityRegistration/registrationStatus"),
                excluded: text(e, "/entityRegistration/exclusionStatusFlag")
                    .eq_ignore_ascii_case("Y"),
                registered_on: text(e, "/entityRegistration/registrationDate"),
                expires_on: text(e, "/entityRegistration/registrationExpirationDate"),
                city: text(e, "/coreData/physicalAddress/city"),
                state: text(e, "/coreData/physicalAddress/stateOrProvinceCode"),
            })
        })
        .collect()
}

/// The companies whose whole legal or trade name is the seller's name,
/// an active one first.
pub fn exact_matches(company_name: &str, found: &[SamEntity]) -> Vec<SamEntity> {
    let Some(wanted) = compared_name(company_name) else {
        return Vec::new();
    };
    let mut matches: Vec<SamEntity> = found
        .iter()
        .filter(|e| {
            [&e.legal_name, &e.trade_name]
                .iter()
                .any(|n| compared_name(n).as_deref() == Some(wanted.as_str()))
        })
        .cloned()
        .collect();
    matches.sort_by_key(|e| !e.status.eq_ignore_ascii_case("Active"));
    matches
}

fn details(e: &SamEntity) -> String {
    let place = [e.city.as_str(), e.state.as_str()]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    let mut parts = Vec::new();
    if !e.uei.is_empty() {
        parts.push(format!("UEI {}", e.uei));
    }
    if !place.is_empty() {
        parts.push(place);
    }
    if !e.registered_on.is_empty() {
        parts.push(format!("registered {}", e.registered_on));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" ({})", parts.join(", "))
    }
}

/// The card from the companies that match.
pub fn sam_card(company_name: &str, matches: &[SamEntity]) -> Signal {
    let (value, signal_type, sub) = match matches.first() {
        Some(e) if matches.iter().any(|m| m.excluded) => {
            let e = matches.iter().find(|m| m.excluded).unwrap_or(e);
            (
                EXCLUDED,
                "caution",
                format!(
                    "\"{}\"{} is EXCLUDED by the US government in SAM.gov: it is barred from government work, often after fraud or a broken contract. It may be another company with the same name - check before paying.",
                    e.legal_name,
                    details(e)
                ),
            )
        }
        Some(e) if e.status.eq_ignore_ascii_case("Active") => (
            ACTIVE,
            "good",
            format!(
                "\"{}\"{} has an active registration with the US government in SAM.gov{}. It shows the company exists, not that this listing is real.",
                e.legal_name,
                details(e),
                if e.expires_on.is_empty() {
                    String::new()
                } else {
                    format!(", valid until {}", e.expires_on)
                }
            ),
        ),
        Some(e) => (
            EXPIRED,
            "info",
            format!(
                "\"{}\"{} was registered with the US government in SAM.gov, but the registration is not active now. It has to be renewed every year and many companies let it lapse, so this is not a warning.",
                e.legal_name,
                details(e)
            ),
        ),
        None => (
            NOT_REGISTERED,
            "info",
            format!(
                "\"{company_name}\" is not registered with the US government in SAM.gov. Most companies never sell to the government, so this is not a warning."
            ),
        ),
    };
    Signal {
        label: LABEL.to_string(),
        sub,
        value: value.to_string(),
        signal_type: signal_type.to_string(),
        category: OUTSIDE_RECORDS.to_string(),
        check_type: "existence".to_string(),
    }
}

fn api_key() -> Option<String> {
    std::env::var("SAM_GOV_API_KEY")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn daily_limit() -> u32 {
    std::env::var("SAM_GOV_DAILY_LIMIT")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(10)
}

/// The search address. "&" and brackets confuse SAM.gov's name search,
/// so they are left out (the exact match is checked by Safely anyway).
pub fn search_url(company_name: &str, key: &str) -> String {
    let name: String = company_name
        .chars()
        .map(|c| if "&()[]{}\"".contains(c) { ' ' } else { c })
        .collect();
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    format!(
        "{ENTITIES_URL}?api_key={}&legalBusinessName={}&includeSections=entityRegistration,coreData",
        urlencoding::encode(key),
        urlencoding::encode(&name)
    )
}

/// What is remembered between scans.
#[derive(Default)]
struct Memory {
    /// Compared name -> (when asked, the matching companies).
    answers: HashMap<String, (Instant, Vec<SamEntity>)>,
    /// The day (days since 1970) the count below is for.
    day: u64,
    asked_today: u32,
}

static MEMORY: Mutex<Option<Memory>> = Mutex::new(None);

fn today() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

/// A kept answer for `key`, if not too old.
fn remembered(key: &str) -> Option<Vec<SamEntity>> {
    let memory = MEMORY.lock().ok()?;
    let (when, matches) = memory.as_ref()?.answers.get(key)?;
    (when.elapsed() < KEEP_FOR).then(|| matches.clone())
}

/// Counts one lookup; false when today's lookups are used up.
fn may_ask_now(limit: u32) -> bool {
    let Ok(mut memory) = MEMORY.lock() else {
        return false;
    };
    let memory = memory.get_or_insert_with(Memory::default);
    let day = today();
    if memory.day != day {
        memory.day = day;
        memory.asked_today = 0;
    }
    if memory.asked_today >= limit {
        return false;
    }
    memory.asked_today += 1;
    true
}

/// SAM.gov said the limit is reached: no more lookups today.
fn stop_for_today() {
    if let Ok(mut memory) = MEMORY.lock() {
        let memory = memory.get_or_insert_with(Memory::default);
        memory.day = today();
        memory.asked_today = u32::MAX;
    }
}

fn remember(key: String, matches: Vec<SamEntity>) {
    if let Ok(mut memory) = MEMORY.lock() {
        let memory = memory.get_or_insert_with(Memory::default);
        memory
            .answers
            .retain(|_, (when, _)| when.elapsed() < KEEP_FOR);
        memory.answers.insert(key, (Instant::now(), matches));
    }
}

async fn ask_sam(company_name: &str, key: &str) -> Option<Vec<SamEntity>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("Safely company check (safely.sh)")
        .build()
        .ok()?;
    // The key is in the address, so errors are logged without it.
    match client.get(search_url(company_name, key)).send().await {
        Ok(r) if r.status().is_success() => match r.json::<Value>().await {
            Ok(answer) => Some(exact_matches(company_name, &entities_from(&answer))),
            Err(e) => {
                eprintln!(
                    "Safely: SAM.gov answer could not be read: {}",
                    e.without_url()
                );
                None
            }
        },
        Ok(r) => {
            let status = r.status();
            if status.as_u16() == 429 {
                stop_for_today();
                eprintln!("Safely: SAM.gov daily limit reached, no more lookups today");
            } else if status.as_u16() == 401 || status.as_u16() == 403 {
                eprintln!(
                    "Safely: SAM.gov refused the key ({status}) - it may have expired; make a new one on your SAM.gov profile page"
                );
            } else {
                eprintln!("Safely: SAM.gov answered {status}");
            }
            None
        }
        Err(e) => {
            eprintln!("Safely: SAM.gov could not be reached: {}", e.without_url());
            None
        }
    }
}

fn is_us_seller(country: Option<&str>) -> bool {
    country
        .and_then(databases_for_country)
        .is_some_and(|c| c.country == "usa")
}

/// The "US government register (SAM.gov)" card, or None when the seller
/// is not in the USA, the check is off, today's lookups are used up, or
/// SAM.gov did not answer.
pub async fn sam_signal(company_name: Option<&str>, country: Option<&str>) -> Option<Signal> {
    let name = company_name.map(str::trim).filter(|n| !n.is_empty())?;
    if !is_us_seller(country) || !registers_enabled() {
        return None;
    }
    let key = api_key()?;
    let wanted = compared_name(name)?;
    if let Some(matches) = remembered(&wanted) {
        return Some(sam_card(name, &matches));
    }
    if !may_ask_now(daily_limit()) {
        return None;
    }
    let matches = ask_sam(name, &key).await?;
    remember(wanted, matches.clone());
    Some(sam_card(name, &matches))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn answer() -> Value {
        json!({ "totalRecords": 3, "entityData": [
            { "entityRegistration": { "ueiSAM": "ABC123DEF456", "legalBusinessName": "ACME TOOLS LLC",
                "dbaName": "", "registrationStatus": "Inactive", "exclusionStatusFlag": "N",
                "registrationDate": "2015-02-01", "registrationExpirationDate": "2019-02-01" },
              "coreData": { "physicalAddress": { "city": "AUSTIN", "stateOrProvinceCode": "TX" } } },
            { "entityRegistration": { "ueiSAM": "XYZ987", "legalBusinessName": "ACME TOOLS, INC.",
                "dbaName": "ACME", "registrationStatus": "Active", "exclusionStatusFlag": "N",
                "registrationDate": "2020-05-03", "registrationExpirationDate": "2027-05-03" },
              "coreData": { "physicalAddress": { "city": "DALLAS", "stateOrProvinceCode": "TX" } } },
            { "entityRegistration": { "ueiSAM": "OTHER1", "legalBusinessName": "ACME TOOLS AND DIES CORP",
                "registrationStatus": "Active", "exclusionStatusFlag": "Y" } }
        ]})
    }

    #[test]
    fn whole_names_only_and_an_active_one_first() {
        let found = entities_from(&answer());
        assert_eq!(found.len(), 3);
        let matches = exact_matches("Acme Tools", &found);
        // "ACME TOOLS AND DIES CORP" is another company.
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].uei, "XYZ987");
        assert_eq!(matches[0].city, "DALLAS");
        // The trade name counts too.
        assert_eq!(exact_matches("Acme Inc", &found).len(), 1);
    }

    #[test]
    fn sam_cards() {
        let found = entities_from(&answer());
        let card = sam_card("Acme Tools", &exact_matches("Acme Tools", &found));
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (ACTIVE, "good")
        );
        assert_eq!(card.category, OUTSIDE_RECORDS);
        assert!(
            card.sub
                .contains("(UEI XYZ987, DALLAS, TX, registered 2020-05-03)")
        );
        assert!(card.sub.contains("valid until 2027-05-03"));

        let inactive: Vec<SamEntity> = found.iter().take(1).cloned().collect();
        let card = sam_card("Acme Tools", &inactive);
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (EXPIRED, "info")
        );

        let excluded = exact_matches("Acme Tools and Dies", &found);
        let card = sam_card("Acme Tools and Dies", &excluded);
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (EXCLUDED, "caution")
        );

        let card = sam_card("Golden Steel Mills", &[]);
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (NOT_REGISTERED, "info")
        );
    }

    #[test]
    fn the_search_leaves_out_characters_sam_gov_cannot_take() {
        let url = search_url("Smith & Sons (USA) LLC", "KEY");
        assert!(url.contains("legalBusinessName=Smith%20Sons%20USA%20LLC"));
        assert!(url.contains("api_key=KEY"));
        assert!(url.contains("includeSections=entityRegistration,coreData"));
    }

    #[test]
    fn lookups_stop_at_the_daily_limit() {
        // A limit of 0 never asks.
        assert!(!may_ask_now(0));
    }

    #[tokio::test]
    async fn only_us_sellers_and_only_when_switched_on() {
        assert!(
            sam_signal(Some("Acme Tools"), Some("Guangdong, China"))
                .await
                .is_none()
        );
        if registers_enabled() && api_key().is_some() {
            return;
        }
        assert!(
            sam_signal(Some("Acme Tools"), Some("Texas, USA"))
                .await
                .is_none()
        );
    }
}
