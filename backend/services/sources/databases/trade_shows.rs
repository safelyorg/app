//! Trade shows: did the seller really have a booth at a fair? A booth
//! is a good sign (the fair checks its exhibitors and the company paid
//! for a stand). Not being an exhibitor says nothing, so then no card -
//! UNLESS the seller says it exhibits there: a claim the fair's own list
//! does not confirm is a warning (a common lie).
//!
//! Built so far: the Canton Fair (China Import and Export Fair, the
//! biggest fair for Chinese suppliers). Its exhibitor search, the one
//! 365.cantonfair.org.cn uses (free, no key):
//! POST https://www.cantonfair.org.cn/appbuyerapi/exhibition/queryshop
//! {"keyword": "...", "pageIndex": 1, "pageSize": 20, ...}
//! It lists the exhibitors of the current session (140th = autumn 2026).
//! Asked for China and Hong Kong sellers, and for any seller that says
//! "Canton Fair" about itself.
//!
//! An exhibitor counts only when its WHOLE name is the seller's name
//! (legal forms and punctuation left out).
//!
//! Off unless TRADE_SHOWS_CHECK_ENABLED is "true" in .env.

use crate::{
    models::analysis::Signal,
    services::{
        shared::standards::databases_for_country,
        sources::databases::{OUTSIDE_RECORDS, sanctions::compared_name},
    },
};
use serde_json::{Value, json};
use std::time::Duration;

/// The card's name, as in standards/databases/global.json.
pub const CANTON_LABEL: &str = "Canton Fair";
pub const EXHIBITOR: &str = "Canton Fair exhibitor";
pub const CLAIM_NOT_FOUND: &str = "Claim not confirmed";

const CANTON_SEARCH: &str = "https://www.cantonfair.org.cn/appbuyerapi/exhibition/queryshop";
const CANTON_SHOP: &str = "https://365.cantonfair.org.cn/en-US/shops?id=";

/// One booth of an exhibitor.
#[derive(Debug, Clone, PartialEq)]
pub struct Booth {
    /// "140" - the fair's session.
    pub session: String,
    /// "1", "2" or "3".
    pub phase: String,
    /// "Household Electrical Appliances".
    pub section: String,
    /// "3.2A09-54".
    pub booth: String,
}

/// One Canton Fair exhibitor.
#[derive(Debug, Clone, PartialEq)]
pub struct CantonExhibitor {
    pub shop_id: String,
    pub name: String,
    /// "CF Award-winning Enterprise", "Multiple-session Exhibitor"...
    pub badges: Vec<String>,
    pub booths: Vec<Booth>,
}

fn text(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// The exhibitors in a "queryshop" answer, or None when the answer is
/// not a success (errCode other than 0).
pub fn canton_exhibitors(answer: &Value) -> Option<Vec<CantonExhibitor>> {
    if answer.get("errCode").and_then(Value::as_i64) != Some(0) {
        return None;
    }
    let items = answer
        .pointer("/result/itemList")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Some(
        items
            .iter()
            .filter_map(|item| {
                let name = text(item, "name");
                let shop_id = text(item, "shopNo");
                if name.is_empty() {
                    return None;
                }
                let badges = item
                    .get("tagDescList")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|b| b.trim().to_string())
                    .filter(|b| !b.is_empty())
                    .collect();
                let mut booths = Vec::new();
                for shop in item
                    .get("offlineShops")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    for area in shop
                        .get("areas")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        let Some(b) = area.get("booth") else {
                            continue;
                        };
                        let booth = Booth {
                            session: text(b, "session"),
                            phase: text(b, "period"),
                            section: [text(b, "areaName"), text(area, "areaName")]
                                .into_iter()
                                .find(|s| !s.is_empty())
                                .unwrap_or_default(),
                            booth: text(b, "booth"),
                        };
                        if !booth.booth.is_empty() && !booths.contains(&booth) {
                            booths.push(booth);
                        }
                    }
                }
                Some(CantonExhibitor {
                    shop_id: if shop_id.is_empty() {
                        text(item, "id")
                    } else {
                        shop_id
                    },
                    name,
                    badges,
                    booths,
                })
            })
            .collect(),
    )
}

/// The exhibitor whose whole name is the seller's name.
pub fn canton_match<'a>(
    company_name: &str,
    found: &'a [CantonExhibitor],
) -> Option<&'a CantonExhibitor> {
    let wanted = compared_name(company_name)?;
    found
        .iter()
        .find(|e| compared_name(&e.name).as_deref() == Some(wanted.as_str()))
}

/// true when the seller says it exhibits at the Canton Fair.
pub fn claims_canton_fair(about: Option<&str>) -> bool {
    about.is_some_and(|t| {
        let lower = t.to_lowercase();
        lower.contains("canton fair") || t.contains("广交会")
    })
}

/// "140th" - English ordinal for the session number.
fn ordinal(number: &str) -> String {
    let Ok(n) = number.parse::<u32>() else {
        return number.to_string();
    };
    let suffix = match (n % 10, n % 100) {
        (1, x) if x != 11 => "st",
        (2, x) if x != 12 => "nd",
        (3, x) if x != 13 => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// The card: an exhibitor is a good sign; a claim the list does not
/// confirm is a warning; otherwise no card.
pub fn canton_card(
    company_name: &str,
    claims: bool,
    exhibitor: Option<&CantonExhibitor>,
) -> Option<Signal> {
    let (value, signal_type, sub) = match exhibitor {
        Some(e) => {
            // At most 3 booths written out (a big company can have 6+).
            let more = e.booths.len().saturating_sub(3);
            let booths = e
                .booths
                .iter()
                .take(3)
                .map(|b| {
                    let mut parts = Vec::new();
                    if !b.phase.is_empty() {
                        parts.push(format!("Phase {}", b.phase));
                    }
                    if !b.section.is_empty() {
                        parts.push(b.section.clone());
                    }
                    parts.push(format!("booth {}", b.booth));
                    parts.join(", ")
                })
                .collect::<Vec<_>>()
                .join("; ")
                + &if more > 0 {
                    format!("; and {more} more")
                } else {
                    String::new()
                };
            let session = e
                .booths
                .iter()
                .map(|b| b.session.as_str())
                .find(|s| !s.is_empty())
                .map(|s| format!("the {} Canton Fair", ordinal(s)))
                .unwrap_or_else(|| "the Canton Fair".to_string());
            let booths = if booths.is_empty() {
                String::new()
            } else {
                format!(" ({booths})")
            };
            let badges = if e.badges.is_empty() {
                String::new()
            } else {
                format!(" The fair lists it as: {}.", e.badges.join(", "))
            };
            (
                EXHIBITOR,
                "good",
                format!(
                    "\"{}\" is an exhibitor at {session}{booths}.{badges} The fair checks its exhibitors, so this is a good sign - but it does not prove that this listing is the company's own. Exhibitor page: {CANTON_SHOP}{}",
                    e.name, e.shop_id
                ),
            )
        }
        None if claims => (
            CLAIM_NOT_FOUND,
            "caution",
            format!(
                "The supplier says it exhibits at the Canton Fair, but \"{company_name}\" is not in the fair's own list of exhibitors for this session. It may have exhibited before, or the claim may be false - ask for its booth number and check it on 365.cantonfair.org.cn before paying."
            ),
        ),
        None => return None,
    };
    Some(Signal {
        label: CANTON_LABEL.to_string(),
        sub,
        value: value.to_string(),
        signal_type: signal_type.to_string(),
        category: OUTSIDE_RECORDS.to_string(),
        check_type: "existence".to_string(),
    })
}

fn enabled() -> bool {
    std::env::var("TRADE_SHOWS_CHECK_ENABLED")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(false)
}

fn is_chinese_seller(country: Option<&str>) -> bool {
    country
        .and_then(databases_for_country)
        .is_some_and(|c| c.country == "china" || c.country == "hong_kong")
}

/// What the search is sent, as the fair's own page sends it.
pub fn canton_request(company_name: &str) -> Value {
    json!({
        "keyword": company_name,
        "pageIndex": 1,
        "pageSize": 20,
        "searchProductShop": "N",
        "searchBooth": "N",
        "companyType": "",
        "isOffline": "N"
    })
}

/// A device id like the one the fair's page makes for a browser (32
/// upper-case letters and digits), made once per server start.
fn device_id() -> &'static str {
    static ID: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        uuid::Uuid::new_v4()
            .simple()
            .to_string()
            .to_ascii_uppercase()
    });
    &ID
}

async fn search_canton(company_name: &str) -> Option<Vec<CantonExhibitor>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36")
        .build()
        .ok()?;
    let response = client
        .post(CANTON_SEARCH)
        .header("Accept", "application/json, text/plain, */*")
        .header("Accept-Language", "en-US")
        // The fair's own page sends these three (seen in Bilal's browser
        // on Nov 4); "locale" makes the answer English.
        .header("locale", "en-US")
        .header("cus-os-type", "WEB")
        .header("deviceid", device_id())
        .header("Origin", "https://365.cantonfair.org.cn")
        .header("Referer", "https://365.cantonfair.org.cn/")
        .json(&canton_request(company_name))
        .send()
        .await;
    match response {
        Ok(r) if r.status().is_success() => match r.json::<Value>().await {
            Ok(answer) => {
                let found = canton_exhibitors(&answer);
                if found.is_none() {
                    eprintln!(
                        "Safely: Canton Fair search said: {}",
                        text(&answer, "errMsg")
                    );
                }
                found
            }
            Err(e) => {
                eprintln!("Safely: Canton Fair answer could not be read: {}", e);
                None
            }
        },
        Ok(r) => {
            eprintln!("Safely: Canton Fair search answered {}", r.status());
            None
        }
        Err(e) => {
            eprintln!("Safely: Canton Fair search could not be reached: {}", e);
            None
        }
    }
}

/// The "Canton Fair" card, or None when the check is off, the seller is
/// neither Chinese nor claims the fair, it is not an exhibitor without a
/// claim, or the fair did not answer.
pub async fn canton_signal(
    company_name: Option<&str>,
    country: Option<&str>,
    about: Option<&str>,
) -> Option<Signal> {
    let name = company_name.map(str::trim).filter(|n| !n.is_empty())?;
    if !enabled() {
        return None;
    }
    let claims = claims_canton_fair(about);
    if !claims && !is_chinese_seller(country) {
        return None;
    }
    compared_name(name)?;
    let found = search_canton(name).await?;
    canton_card(name, claims, canton_match(name, &found))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Part of the real answer for "Midea" Bilal saved on Nov 4.
    fn answer() -> Value {
        json!({
          "errCode": 0, "errMsg": "success",
          "result": { "totalCount": 50, "itemList": [
            { "id": "0797904938", "shopNo": "0797904938", "name": "MIDEA GROUP CO.,LTD.",
              "tagDescList": ["CF Award-winning Enterprise", "Multiple-session Exhibitor", "Brand Enterprise"],
              "offlineShops": [ { "period": "1", "areas": [ { "areaCode": "EH04",
                  "areaName": "Household Electrical Appliances",
                  "booth": { "session": "140", "period": "1", "booth": "3.2A09-54",
                             "areaName": "Household Electrical Appliances" } } ] } ],
              "session": "140" },
            { "id": "0763715755", "shopNo": "0763715755", "name": "Hefei Midea Laundry Appliances,Co.,Ltd",
              "tagDescList": [],
              "offlineShops": [ { "period": "1", "areas": [ { "areaName": "Household Electrical Appliances",
                  "booth": { "session": "140", "period": "1", "booth": "3.2A55-64" } } ] } ] }
          ]}
        })
    }

    #[test]
    fn the_answer_is_read() {
        let found = canton_exhibitors(&answer()).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].shop_id, "0797904938");
        assert_eq!(found[0].badges.len(), 3);
        assert_eq!(
            found[0].booths,
            vec![Booth {
                session: "140".into(),
                phase: "1".into(),
                section: "Household Electrical Appliances".into(),
                booth: "3.2A09-54".into()
            }]
        );
        // The section comes from the area when the booth has none.
        assert_eq!(
            found[1].booths[0].section,
            "Household Electrical Appliances"
        );
        assert!(canton_exhibitors(&json!({ "errCode": 401, "errMsg": "no" })).is_none());
        assert_eq!(
            canton_exhibitors(&json!({ "errCode": 0, "result": {} })),
            Some(vec![])
        );
    }

    #[test]
    fn whole_names_only() {
        let found = canton_exhibitors(&answer()).unwrap();
        assert_eq!(
            canton_match("Midea Group Co., Ltd.", &found).map(|e| e.shop_id.as_str()),
            Some("0797904938")
        );
        assert_eq!(
            canton_match("Hefei Midea Laundry Appliances Co., Ltd.", &found)
                .map(|e| e.shop_id.as_str()),
            Some("0763715755")
        );
        assert!(canton_match("Midea", &found).is_none());
        assert!(canton_match("Midea Trading Co., Ltd.", &found).is_none());
    }

    /// Bilal's second real answer (Nov 4): searched with the full name,
    /// not logged in (tokens null) - exactly one exhibitor.
    #[test]
    fn a_full_name_search_finds_the_one_company() {
        let answer = json!({
          "errCode": 0, "errMsg": "success",
          "result": { "totalCount": 1, "itemList": [
            { "id": "0796200728", "shopNo": "0796200728",
              "name": "NINGBO HEIGER ELECTRICAL APPLIANCES CO.,LTD.",
              "tagDescList": ["Multiple-session Exhibitor", "Brand Enterprise"],
              "offlineShops": [ { "period": "1", "areas": [ { "areaName": "Household Electrical Appliances",
                  "booth": { "session": "140", "period": "1", "booth": "2.2B08-12，2.2A13-17",
                             "areaName": "Household Electrical Appliances" } } ] } ] }
          ]},
          "cantonToken": null, "qdToken": null, "authToken": null, "statusCode": 200
        });
        let found = canton_exhibitors(&answer).unwrap();
        let name = "Ningbo Heiger Electrical Appliances Co., Ltd";
        let card = canton_card(name, false, canton_match(name, &found)).unwrap();
        assert_eq!(card.value, EXHIBITOR);
        assert!(card.sub.contains("booth 2.2B08-12，2.2A13-17"));
        assert!(card.sub.contains("shops?id=0796200728"));
    }

    #[test]
    fn claims_are_spotted() {
        assert!(claims_canton_fair(Some(
            "We exhibit at the Canton Fair every year."
        )));
        assert!(claims_canton_fair(Some("参加广交会")));
        assert!(!claims_canton_fair(Some("We make fans.")));
        assert!(!claims_canton_fair(None));
    }

    #[test]
    fn canton_cards() {
        let found = canton_exhibitors(&answer()).unwrap();
        let card = canton_card(
            "Midea Group Co., Ltd.",
            false,
            canton_match("Midea Group Co., Ltd.", &found),
        )
        .unwrap();
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (EXHIBITOR, "good")
        );
        assert_eq!(card.category, OUTSIDE_RECORDS);
        assert!(card.sub.contains(
            "the 140th Canton Fair (Phase 1, Household Electrical Appliances, booth 3.2A09-54)"
        ));
        assert!(card.sub.contains("CF Award-winning Enterprise"));
        assert!(card.sub.contains("shops?id=0797904938"));

        let card = canton_card("Golden Fans Co., Ltd.", true, None).unwrap();
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (CLAIM_NOT_FOUND, "caution")
        );
        assert!(canton_card("Golden Fans Co., Ltd.", false, None).is_none());
        assert_eq!(ordinal("141"), "141st");
        assert_eq!(ordinal("112"), "112th");
    }

    #[test]
    fn the_search_is_sent_like_the_fairs_page_sends_it() {
        let r = canton_request("Midea Group");
        assert_eq!(r["keyword"], "Midea Group");
        assert_eq!(r["pageSize"], 20);
        assert_eq!(r["searchProductShop"], "N");
        let id = device_id();
        assert_eq!(id.len(), 32);
        assert!(
            id.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        );
        assert_eq!(id, device_id());
    }

    #[tokio::test]
    async fn only_chinese_sellers_or_a_claim_and_only_when_switched_on() {
        assert!(
            canton_signal(Some("Acme"), Some("Texas, USA"), None)
                .await
                .is_none()
        );
        if enabled() {
            return;
        }
        assert!(
            canton_signal(Some("Midea Group"), Some("Guangdong, China"), None)
                .await
                .is_none()
        );
    }
}
