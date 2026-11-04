//! GLEIF: the official world register of LEI numbers (Legal Entity
//! Identifier). A company gets an LEI when it trades in finance or
//! internationally, so mostly bigger companies are in it. Free, no key:
//! https://api.gleif.org/api/v1/lei-records
//!
//! Safely searches the company's name in every name GLEIF keeps (the
//! legal name, other names and their Latin spelling) and accepts only an
//! EXACT match of the whole name, after legal forms ("Co., Ltd.") and
//! punctuation are removed (the same rule as the sanctions check).
//!
//! - Registered, in the seller's country: a good sign.
//! - Registered but closed: a warning.
//! - The same name in another country: information only.
//! - Not in the register: information only - most small companies are
//!   not in it, so it is never a warning.
//!
//! Off unless GLEIF_CHECK_ENABLED is "true" in .env, so tests and local
//! runs never call GLEIF. When GLEIF does not answer, no card is shown.

use crate::{
    models::analysis::Signal,
    services::{
        shared::standards::databases_for_country,
        sources::databases::{OUTSIDE_RECORDS, sanctions::compared_name},
    },
};
use serde_json::Value;
use std::time::Duration;

/// The card's name, as in standards/databases/global.json.
pub const LABEL: &str = "Company register (GLEIF)";
pub const REGISTERED: &str = "Registered";
pub const CLOSED: &str = "Closed in the register";
pub const OTHER_COUNTRY: &str = "Same name, other country";
pub const NOT_IN_REGISTER: &str = "Not in the register";

const SEARCH_URL: &str = "https://api.gleif.org/api/v1/lei-records";

/// Most records read for one name.
const PAGE_SIZE: &str = "10";

/// One company from the GLEIF register.
#[derive(Debug, Clone, PartialEq)]
pub struct GleifCompany {
    pub lei: String,
    pub legal_name: String,
    /// Every name GLEIF keeps for it (legal, other, Latin spelling).
    pub names: Vec<String>,
    /// Two letters: "CN", "HK", "US"...
    pub country: String,
    pub city: String,
    /// "ACTIVE" or "INACTIVE" (closed).
    pub entity_status: String,
    /// "ISSUED", "LAPSED" (not renewed - common), "RETIRED"...
    pub registration_status: String,
    /// "2014-02-28" - when it first got its LEI.
    pub registered_on: String,
}

fn text(value: &Value, path: &str) -> String {
    value
        .pointer(path)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

/// The companies in a GLEIF answer.
pub fn companies_from(answer: &Value) -> Vec<GleifCompany> {
    let Some(records) = answer.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    records
        .iter()
        .filter_map(|record| {
            let a = record.get("attributes")?;
            let legal_name = text(a, "/entity/legalName/name");
            let lei = text(a, "/lei");
            if legal_name.is_empty() || lei.is_empty() {
                return None;
            }
            let mut names = vec![legal_name.clone()];
            for list in ["/entity/otherNames", "/entity/transliteratedOtherNames"] {
                for other in a
                    .pointer(list)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let name = text(other, "/name");
                    if !name.is_empty() && !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
            Some(GleifCompany {
                lei,
                legal_name,
                names,
                country: text(a, "/entity/legalAddress/country").to_uppercase(),
                city: text(a, "/entity/legalAddress/city"),
                entity_status: text(a, "/entity/status").to_uppercase(),
                registration_status: text(a, "/registration/status").to_uppercase(),
                registered_on: text(a, "/registration/initialRegistrationDate")
                    .chars()
                    .take(10)
                    .collect(),
            })
        })
        .collect()
}

/// The companies whose name is exactly the seller's name (legal forms
/// and punctuation left out).
pub fn exact_matches<'a>(company_name: &str, found: &'a [GleifCompany]) -> Vec<&'a GleifCompany> {
    let Some(wanted) = compared_name(company_name) else {
        return Vec::new();
    };
    found
        .iter()
        .filter(|c| {
            c.names
                .iter()
                .any(|n| compared_name(n).as_deref() == Some(wanted.as_str()))
        })
        .collect()
}

/// true when the GLEIF country code ("CN") is the seller's country as
/// the page writes it ("Guangdong, China"). None when Safely has no
/// file for the seller's country, so it cannot tell.
fn same_country(seller_country: Option<&str>, code: &str) -> Option<bool> {
    let file = databases_for_country(seller_country?)?;
    Some(file.names.iter().any(|n| n.eq_ignore_ascii_case(code)))
}

fn where_and_when(c: &GleifCompany) -> String {
    let place = match (c.city.is_empty(), c.country.is_empty()) {
        (false, false) => format!(", {}, {}", c.city, c.country),
        (true, false) => format!(", {}", c.country),
        _ => String::new(),
    };
    let since = if c.registered_on.is_empty() {
        String::new()
    } else {
        format!(", in the register since {}", c.registered_on)
    };
    format!("(LEI {}{}{})", c.lei, place, since)
}

/// The card, from the companies GLEIF returned.
pub fn gleif_card(
    company_name: &str,
    seller_country: Option<&str>,
    found: &[GleifCompany],
) -> Signal {
    let matches = exact_matches(company_name, found);
    // A match in the seller's country (or where the country cannot be
    // compared) first, an active one before a closed one.
    let mut here: Vec<&GleifCompany> = matches
        .iter()
        .copied()
        .filter(|c| same_country(seller_country, &c.country) != Some(false))
        .collect();
    here.sort_by_key(|c| c.entity_status != "ACTIVE");

    let (value, signal_type, sub) = match (here.first(), matches.first()) {
        (Some(c), _) if c.entity_status == "INACTIVE" => (
            CLOSED,
            "caution",
            format!(
                "\"{}\" {} is listed as CLOSED in the official GLEIF register. Ask the supplier why it sells under the name of a closed company before paying.",
                c.legal_name,
                where_and_when(c)
            ),
        ),
        (Some(c), _) => (
            REGISTERED,
            "good",
            format!(
                "\"{}\" {} is in the official GLEIF register of companies. GLEIF lists companies that trade in finance or internationally; it shows the company exists, not that this listing is real.{}",
                c.legal_name,
                where_and_when(c),
                if c.registration_status == "LAPSED" {
                    " Its LEI was not renewed (\"lapsed\"), which is common and not a problem on its own."
                } else {
                    ""
                }
            ),
        ),
        (None, Some(c)) => (
            OTHER_COUNTRY,
            "info",
            format!(
                "A company named \"{}\" {} is in the GLEIF register, but in another country than this supplier. It may be a different company with the same name.",
                c.legal_name,
                where_and_when(c)
            ),
        ),
        (None, None) => (
            NOT_IN_REGISTER,
            "info",
            format!(
                "\"{company_name}\" is not in the GLEIF register. Most small companies and factories are not in it, so this is not a warning."
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

fn enabled() -> bool {
    std::env::var("GLEIF_CHECK_ENABLED")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(false)
}

/// The search address: every company with this name in any of its
/// names (filter[fulltext]), at most PAGE_SIZE of them.
fn search_url(company_name: &str) -> String {
    format!(
        "{SEARCH_URL}?filter%5Bfulltext%5D={}&page%5Bsize%5D={PAGE_SIZE}",
        urlencoding::encode(company_name)
    )
}

/// Asks GLEIF for every company with this name in any of its names.
/// None when GLEIF did not answer (the reason is logged).
async fn search(company_name: &str) -> Option<Vec<GleifCompany>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("Safely company check (safely.sh)")
        .build()
        .ok()?;
    let response = client
        .get(search_url(company_name))
        .header("Accept", "application/vnd.api+json")
        .send()
        .await;
    match response {
        Ok(r) if r.status().is_success() => match r.json::<Value>().await {
            Ok(answer) => Some(companies_from(&answer)),
            Err(e) => {
                eprintln!("Safely: GLEIF answer could not be read: {}", e);
                None
            }
        },
        Ok(r) => {
            eprintln!("Safely: GLEIF answered {}", r.status());
            None
        }
        Err(e) => {
            eprintln!("Safely: GLEIF could not be reached: {}", e);
            None
        }
    }
}

/// The "Company register (GLEIF)" card, or None when there is nothing
/// to check (the check is off, or there is no usable company name) or
/// GLEIF did not answer.
pub async fn gleif_signal(
    company_name: Option<&str>,
    seller_country: Option<&str>,
) -> Option<Signal> {
    if !enabled() {
        return None;
    }
    let name = company_name?.trim();
    compared_name(name)?;
    let found = search(name).await?;
    Some(gleif_card(name, seller_country, &found))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn record(lei: &str, legal: &str, other: &str, country: &str, status: &str) -> Value {
        json!({
            "type": "lei-records",
            "id": lei,
            "attributes": {
                "lei": lei,
                "entity": {
                    "legalName": { "name": legal, "language": "zh" },
                    "otherNames": [],
                    "transliteratedOtherNames": [
                        { "name": other, "language": "zh-Latn", "type": "AUTO_ASCII_TRANSLITERATED_LEGAL_NAME" }
                    ],
                    "legalAddress": { "city": "Shenzhen", "country": country },
                    "status": status
                },
                "registration": {
                    "initialRegistrationDate": "2019-03-14T08:00:00Z",
                    "status": "ISSUED"
                }
            }
        })
    }

    fn answer() -> Vec<GleifCompany> {
        companies_from(&json!({ "data": [
            record("300300A1", "深圳市三叶草实业有限公司", "Shenzhen Clover Industrial Co., Ltd.", "CN", "ACTIVE"),
            record("549300B2", "Clover Industrial Holdings Inc.", "", "US", "ACTIVE"),
            record("984500C3", "Acme Trading Limited", "", "HK", "INACTIVE"),
        ]}))
    }

    #[test]
    fn records_are_read_with_every_name() {
        let found = answer();
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].lei, "300300A1");
        assert_eq!(found[0].names[1], "Shenzhen Clover Industrial Co., Ltd.");
        assert_eq!(found[0].country, "CN");
        assert_eq!(found[0].registered_on, "2019-03-14");
        assert!(companies_from(&json!({ "data": [] })).is_empty());
        assert!(companies_from(&json!({ "errors": [] })).is_empty());
    }

    #[test]
    fn only_the_whole_name_matches() {
        let found = answer();
        // The Latin spelling of a Chinese legal name counts.
        assert_eq!(
            exact_matches("SHENZHEN CLOVER INDUSTRIAL CO LTD", &found).len(),
            1
        );
        // A shorter or longer name does not.
        assert!(exact_matches("Clover Industrial Co., Ltd", &found).is_empty());
        assert!(exact_matches("Shenzhen Clover Industrial Trading Co., Ltd.", &found).is_empty());
    }

    #[test]
    fn registered_in_the_sellers_country_is_a_good_sign() {
        let card = gleif_card(
            "Shenzhen Clover Industrial Co., Ltd.",
            Some("Guangdong, China"),
            &answer(),
        );
        assert_eq!(card.label, LABEL);
        assert_eq!(card.value, REGISTERED);
        assert_eq!(card.signal_type, "good");
        assert_eq!(card.category, OUTSIDE_RECORDS);
        assert!(
            card.sub
                .contains("(LEI 300300A1, Shenzhen, CN, in the register since 2019-03-14)")
        );
    }

    #[test]
    fn the_same_name_in_another_country_is_information_only() {
        let card = gleif_card(
            "Shenzhen Clover Industrial Co., Ltd.",
            Some("Paris, France"),
            &answer(),
        );
        assert_eq!(card.value, OTHER_COUNTRY);
        assert_eq!(card.signal_type, "info");
        // A country Safely has no file for cannot be compared: counted as a match.
        let card = gleif_card(
            "Shenzhen Clover Industrial Co., Ltd.",
            Some("Pakistan"),
            &answer(),
        );
        assert_eq!(card.value, REGISTERED);
    }

    #[test]
    fn a_closed_company_is_a_warning() {
        let card = gleif_card("Acme Trading Ltd", Some("Hong Kong"), &answer());
        assert_eq!(card.value, CLOSED);
        assert_eq!(card.signal_type, "caution");
    }

    #[test]
    fn not_in_the_register_is_never_a_warning() {
        let card = gleif_card(
            "Guangdong Boyue Trading Co., Ltd.",
            Some("Guangdong, China"),
            &answer(),
        );
        assert_eq!(card.value, NOT_IN_REGISTER);
        assert_eq!(card.signal_type, "info");
        assert!(card.sub.contains("so this is not a warning"));
    }

    #[test]
    fn the_name_is_sent_safely_in_the_address() {
        assert_eq!(
            search_url("Acme & Sons Co., Ltd."),
            "https://api.gleif.org/api/v1/lei-records?filter%5Bfulltext%5D=Acme%20%26%20Sons%20Co.%2C%20Ltd.&page%5Bsize%5D=10"
        );
    }

    #[tokio::test]
    async fn the_check_is_off_unless_switched_on() {
        if !enabled() {
            assert!(
                gleif_signal(Some("Siemens AG"), Some("Germany"))
                    .await
                    .is_none()
            );
        }
    }
}
