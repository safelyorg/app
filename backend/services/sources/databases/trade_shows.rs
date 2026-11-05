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
//! Also the HKTDC fairs (Hong Kong Trade Development Council). Each
//! fair's exhibitor search is an ordinary page whose address holds the
//! search (free, no key):
//! https://www.hktdc.com/event/{fair code}/en/exhibitor-search/{name}
//! and the page carries its results as data (the "__NEXT_DATA__"
//! script): name, booth, and the HKTDC fairs the company exhibited at
//! in recent years. Asked for China and Hong Kong sellers, one request
//! per fair in HKTDC_FAIRS. Only a good sign: HKTDC runs many fairs and
//! Safely reads only some, so a claim that is not found is not a warning.
//!
//! And US fairs on Map Your Show (the exhibitor system of CES and
//! hundreds of US fairs). Each show's exhibitor search, the one its own
//! page uses (free, no key):
//! GET {show}/ajax/remote-proxy.cfm?action=search&search={name}&searchtype=keyword&show=all
//! Asked for every seller (CES has many Chinese, Korean and US
//! exhibitors). Only a good sign; not found shows no card.
//!
//! And fairs on Swapcard, the event platform Informa uses (Hospitalar
//! and Fispal Tecnologia / Food Service in Brazil...). Each fair's exhibitor list is read the way its own
//! page reads it (free, no key):
//! POST {host}/api/graphql  [{"operationName": "EventExhibitorListViewConnectionQuery",
//!   "variables": {"withEvent": true, "viewId": ..., "eventId": ..., "endCursor": ...},
//!   "extensions": {"persistedQuery": {"version": 1, "sha256Hash": ...}}}]
//! 50 exhibitors a page. The whole list (Hospitalar 2027: 568) is read
//! once a day and kept in memory, so a scan adds no request. Asked for
//! every seller. Only a good sign; not found shows no card.
//!
//! And fairs searched on Algolia (RX's Feicon in Brazil, Comexposium's
//! SIAL Paris and Maison&Objet in France...). Their exhibitor lists are searched
//! on Algolia, with the public search key each fair's page gives every
//! visitor's browser (it can only read the public exhibitor list):
//! POST https://{app}-dsn.algolia.net/1/indexes/{index}/query
//!   {"params": "query={name}&filters=recordType:exhibitor AND locale:pt-br AND eventEditionId:{edition}&typoTolerance=false"}
//! One request per fair per scan. Asked for every seller. Only a good
//! sign; not found shows no card.
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

/// The browser the fairs' own pages are made for.
const BROWSER: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36";

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
    /// Certified by China Customs as an AEO (Advanced Certified
    /// Enterprise): "isAeo": "Y", or an "AEO" badge.
    pub aeo: bool,
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
                let badges: Vec<String> = item
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
                let aeo = matches!(item.get("isAeo"), Some(Value::Bool(true)))
                    || matches!(
                        text(item, "isAeo").to_ascii_uppercase().as_str(),
                        "Y" | "YES" | "1" | "TRUE"
                    )
                    || badges.iter().any(|b: &String| {
                        b.split(|c: char| !c.is_ascii_alphanumeric())
                            .any(|w| w == "AEO")
                    });
                Some(CantonExhibitor {
                    shop_id: if shop_id.is_empty() {
                        text(item, "id")
                    } else {
                        shop_id
                    },
                    name,
                    badges,
                    booths,
                    aeo,
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
        .user_agent(BROWSER)
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
/// The card's name, as in standards/databases/global.json.
pub const AEO_LABEL: &str = "China Customs AEO";
pub const AEO_CERTIFIED: &str = "AEO certified";

/// The "China Customs AEO" card for a Canton Fair exhibitor China
/// Customs certified as an AEO; None otherwise. China Customs' own
/// lookup (credit.customs.gov.cn) refuses visitors outside mainland
/// China (Bilal, Nov 5), so the fair's own AEO mark is used.
pub fn aeo_card(e: &CantonExhibitor) -> Option<Signal> {
    if !e.aeo {
        return None;
    }
    Some(Signal {
        label: AEO_LABEL.to_string(),
        sub: format!(
            "\"{}\" is certified by China Customs as an AEO (Advanced Certified Enterprise), its strictest check of exporters, as the Canton Fair's own exhibitor list shows. This is a strong good sign - but it does not prove that this listing is the company's own. Exhibitor page: {CANTON_SHOP}{}",
            e.name, e.shop_id
        ),
        value: AEO_CERTIFIED.to_string(),
        signal_type: "good".to_string(),
        category: OUTSIDE_RECORDS.to_string(),
        check_type: "existence".to_string(),
    })
}

/// The "Canton Fair" card (and the "China Customs AEO" card when the
/// exhibitor is AEO certified). Empty when the check is off, the seller
/// is neither Chinese nor claims the fair, it is not an exhibitor
/// without a claim, or the fair did not answer.
pub async fn canton_signal(
    company_name: Option<&str>,
    country: Option<&str>,
    about: Option<&str>,
) -> Vec<Signal> {
    let Some(name) = company_name.map(str::trim).filter(|n| !n.is_empty()) else {
        return Vec::new();
    };
    let claims = claims_canton_fair(about);
    if !enabled() || (!claims && !is_chinese_seller(country)) || compared_name(name).is_none() {
        return Vec::new();
    }
    let Some(found) = search_canton(name).await else {
        return Vec::new();
    };
    let exhibitor = canton_match(name, &found);
    canton_card(name, claims, exhibitor)
        .into_iter()
        .chain(exhibitor.and_then(aeo_card))
        .collect()
}

// ---------------------------------------------------------------------
// HKTDC fairs
// ---------------------------------------------------------------------

/// The card's name, as in standards/databases/global.json.
pub const HKTDC_LABEL: &str = "HKTDC fair";
pub const HKTDC_EXHIBITOR: &str = "HKTDC exhibitor";

/// The HKTDC fairs searched, by the code in their address. Add a fair
/// by adding its code (the part after /event/ in its exhibitor list's
/// address).
pub const HKTDC_FAIRS: &[&str] = &["hkelectronicsfairse"];

/// Most fairs written out from the "exhibited in recent years" list.
const MAX_PAST_FAIRS: usize = 4;

/// One HKTDC exhibitor.
#[derive(Debug, Clone, PartialEq)]
pub struct HktdcExhibitor {
    pub name: String,
    /// "1S005V6OU" - its page is /exhibitor/{urn}.
    pub urn: String,
    pub fair_code: String,
    /// "HKTDC Hong Kong Electronics Fair (Spring Edition)".
    pub fair_name: String,
    /// "1D-C01".
    pub booths: String,
    /// "Chinese Mainland".
    pub country: String,
    /// "HKTDC Hong Kong Electronics Fair 2026 (Autumn Edition)", ...
    pub past_fairs: Vec<String>,
}

impl HktdcExhibitor {
    pub fn page(&self) -> String {
        format!(
            "https://www.hktdc.com/event/{0}/en/exhibitor/{1}?fairCode={0}",
            self.fair_code, self.urn
        )
    }
}

/// The search page's address for a company name. Only letters, digits
/// and spaces are kept ("/" or "?" would break the address).
pub fn hktdc_search_url(fair_code: &str, company_name: &str) -> String {
    let words: String = company_name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    format!(
        "https://www.hktdc.com/event/{fair_code}/en/exhibitor-search/{}",
        urlencoding::encode(&words)
    )
}

/// The exhibitors in a search page (its "__NEXT_DATA__" script), or
/// None when the page has no such data (another page, or HKTDC changed it).
pub fn hktdc_exhibitors(page: &str) -> Option<Vec<HktdcExhibitor>> {
    let start = page.find("id=\"__NEXT_DATA__\"")?;
    let open = start + page[start..].find('>')? + 1;
    let close = open + page[open..].find("</script>")?;
    let data: Value = serde_json::from_str(page[open..close].trim()).ok()?;
    let props = data.pointer("/props/pageProps")?;
    let list = props.pointer("/exhibitorListData/data")?.as_array()?;
    let fair_name = props
        .pointer("/fairOption/fairName")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Some(
        list.iter()
            .filter_map(|item| {
                let name = text(item, "exhibitorName");
                if name.is_empty() {
                    return None;
                }
                let mut past_fairs: Vec<String> = Vec::new();
                for detail in item
                    .get("supplierSriDetail")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let fair = text(detail, "supplierRefValue");
                    if text(detail, "supplierRefKey").starts_with("Exhibited in HKTDC")
                        && !fair.is_empty()
                        && !past_fairs.contains(&fair)
                    {
                        past_fairs.push(fair);
                    }
                }
                Some(HktdcExhibitor {
                    name,
                    urn: text(item, "exhibitorUrn"),
                    fair_code: text(item, "fairSymbol"),
                    fair_name: fair_name.clone(),
                    booths: text(item, "boothNumbers"),
                    country: text(item, "countryDesc"),
                    past_fairs,
                })
            })
            .collect(),
    )
}

/// The exhibitor whose whole name is the seller's name.
pub fn hktdc_match<'a>(
    company_name: &str,
    found: &'a [HktdcExhibitor],
) -> Option<&'a HktdcExhibitor> {
    let wanted = compared_name(company_name)?;
    found
        .iter()
        .find(|e| compared_name(&e.name).as_deref() == Some(wanted.as_str()))
}

/// The card for an HKTDC exhibitor.
pub fn hktdc_card(e: &HktdcExhibitor) -> Signal {
    let fair = if e.fair_name.is_empty() {
        "an HKTDC fair".to_string()
    } else {
        e.fair_name.clone()
    };
    let booth = if e.booths.is_empty() {
        String::new()
    } else {
        format!(" (booth {})", e.booths)
    };
    let past = if e.past_fairs.is_empty() {
        String::new()
    } else {
        let more = e.past_fairs.len().saturating_sub(MAX_PAST_FAIRS);
        let shown = e
            .past_fairs
            .iter()
            .take(MAX_PAST_FAIRS)
            .cloned()
            .collect::<Vec<_>>()
            .join("; ");
        let more = if more > 0 {
            format!("; and {more} more")
        } else {
            String::new()
        };
        format!(" HKTDC lists it at these fairs in recent years: {shown}{more}.")
    };
    Signal {
        label: HKTDC_LABEL.to_string(),
        sub: format!(
            "\"{}\" is an exhibitor at {fair}{booth}.{past} HKTDC checks its exhibitors, so this is a good sign - but it does not prove that this listing is the company's own. Exhibitor page: {}",
            e.name,
            e.page()
        ),
        value: HKTDC_EXHIBITOR.to_string(),
        signal_type: "good".to_string(),
        category: OUTSIDE_RECORDS.to_string(),
        check_type: "existence".to_string(),
    }
}

async fn search_hktdc(fair_code: &str, company_name: &str) -> Option<Vec<HktdcExhibitor>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent(BROWSER)
        .build()
        .ok()?;
    let url = hktdc_search_url(fair_code, company_name);
    match client
        .get(&url)
        .header("Accept", "text/html,application/xhtml+xml")
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => {
            let page = r.text().await.ok()?;
            let found = hktdc_exhibitors(&page);
            if found.is_none() {
                eprintln!("Safely: HKTDC page had no exhibitor data: {url}");
            }
            found
        }
        Ok(r) => {
            eprintln!("Safely: HKTDC answered {}: {url}", r.status());
            None
        }
        Err(e) => {
            eprintln!("Safely: HKTDC could not be reached: {}", e);
            None
        }
    }
}

/// The "HKTDC fair" card, or None when the check is off, the seller is
/// not Chinese, or it is not an exhibitor at the fairs Safely reads.
pub async fn hktdc_signal(company_name: Option<&str>, country: Option<&str>) -> Option<Signal> {
    let name = company_name.map(str::trim).filter(|n| !n.is_empty())?;
    if !enabled() || !is_chinese_seller(country) {
        return None;
    }
    compared_name(name)?;
    // One fair after another, stopping at the first match (few fairs).
    for fair in HKTDC_FAIRS {
        if let Some(found) = search_hktdc(fair, name).await
            && let Some(e) = hktdc_match(name, &found)
        {
            return Some(hktdc_card(e));
        }
    }
    None
}

// ---------------------------------------------------------------------
// Map Your Show fairs (CES...)
// ---------------------------------------------------------------------

/// The card's name, as in standards/databases/global.json.
pub const MYS_LABEL: &str = "US trade show";
/// Already translated for Global Sources' badge of the same words.
pub const MYS_EXHIBITOR: &str = "Trade show exhibitor";

/// The Map Your Show fairs searched: (name, address of its "8_0" site).
/// Add a fair by adding its line.
pub const MYS_SHOWS: &[(&str, &str)] = &[("CES", "https://exhibitors.ces.tech/8_0")];

/// One Map Your Show exhibitor.
#[derive(Debug, Clone, PartialEq)]
pub struct MysExhibitor {
    pub name: String,
    /// "0013000001KaMXYAA3".
    pub id: String,
    /// "CES 2027" - the show and year, from the record's id ("...-CES27").
    pub show: String,
    /// "N110", "Alsace"...
    pub booths: Vec<String>,
    pub page: String,
}

/// The search address for a company name.
pub fn mys_search_url(base: &str, company_name: &str) -> String {
    format!(
        "{base}/ajax/remote-proxy.cfm?action=search&search={}&searchtype=keyword&show=all",
        urlencoding::encode(company_name.trim())
    )
}

/// "CES" + "...-Exhibitor-CES27" -> "CES 2027".
fn show_and_year(show: &str, record_id: &str) -> String {
    let code = record_id.rsplit('-').next().unwrap_or("");
    let digits: String = code
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect();
    let digits: String = digits.chars().rev().collect();
    match digits.len() {
        2 => format!("{show} 20{digits}"),
        4 => format!("{show} {digits}"),
        _ => show.to_string(),
    }
}

/// The exhibitors in a search answer, or None when it is not a success.
pub fn mys_exhibitors(show: &str, base: &str, answer: &Value) -> Option<Vec<MysExhibitor>> {
    if answer.get("SUCCESS").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let hits = answer
        .pointer("/DATA/results/exhibitor/hit")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Some(
        hits.iter()
            .filter_map(|hit| {
                let f = hit.get("fields")?;
                let name = text(f, "exhname_t");
                let id = text(f, "exhid_l");
                if name.is_empty() || id.is_empty() {
                    return None;
                }
                let mut booths: Vec<String> = Vec::new();
                for b in f
                    .get("boothsdisplay_la")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    // Map Your Show adds "randomstring" to every booth.
                    let b = b.trim_end_matches("randomstring").trim().to_string();
                    if !b.is_empty() && !booths.contains(&b) {
                        booths.push(b);
                    }
                }
                Some(MysExhibitor {
                    page: format!("{base}/exhibitor/exhibitor-details.cfm?exhid={id}"),
                    show: show_and_year(show, &text(hit, "id")),
                    name,
                    id,
                    booths,
                })
            })
            .collect(),
    )
}

/// The exhibitor whose whole name is the seller's name.
pub fn mys_match<'a>(company_name: &str, found: &'a [MysExhibitor]) -> Option<&'a MysExhibitor> {
    let wanted = compared_name(company_name)?;
    found
        .iter()
        .find(|e| compared_name(&e.name).as_deref() == Some(wanted.as_str()))
}

/// The card for a Map Your Show exhibitor.
pub fn mys_card(e: &MysExhibitor) -> Signal {
    let booths = if e.booths.is_empty() {
        String::new()
    } else {
        let more = e.booths.len().saturating_sub(3);
        let shown = e
            .booths
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let label = if e.booths.len() == 1 {
            "booth"
        } else {
            "booths"
        };
        if more > 0 {
            format!(" ({label} {shown} and {more} more)")
        } else {
            format!(" ({label} {shown})")
        }
    };
    Signal {
        label: MYS_LABEL.to_string(),
        sub: format!(
            "\"{}\" is an exhibitor at {}{booths}. The fair checks its exhibitors, so this is a good sign - but it does not prove that this listing is the company's own. Exhibitor page: {}",
            e.name, e.show, e.page
        ),
        value: MYS_EXHIBITOR.to_string(),
        signal_type: "good".to_string(),
        category: OUTSIDE_RECORDS.to_string(),
        check_type: "existence".to_string(),
    }
}

async fn search_mys(show: &str, base: &str, company_name: &str) -> Option<Vec<MysExhibitor>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(BROWSER)
        .build()
        .ok()?;
    let response = client
        .get(mys_search_url(base, company_name))
        .header("Accept", "application/json, text/javascript, */*; q=0.01")
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Referer", format!("{base}/"))
        .send()
        .await;
    match response {
        Ok(r) if r.status().is_success() => match r.json::<Value>().await {
            Ok(answer) => {
                let found = mys_exhibitors(show, base, &answer);
                if found.is_none() {
                    eprintln!(
                        "Safely: {show} search said: {}",
                        text(&answer, "ERRORMESSAGE")
                    );
                }
                found
            }
            Err(e) => {
                eprintln!("Safely: {show} answer could not be read: {}", e);
                None
            }
        },
        Ok(r) => {
            eprintln!("Safely: {show} search answered {}", r.status());
            None
        }
        Err(e) => {
            eprintln!("Safely: {show} search could not be reached: {}", e);
            None
        }
    }
}

/// The "US trade show" card, or None when the check is off or the
/// seller is not an exhibitor at the Map Your Show fairs Safely reads.
pub async fn mys_signal(company_name: Option<&str>) -> Option<Signal> {
    let name = company_name.map(str::trim).filter(|n| !n.is_empty())?;
    if !enabled() {
        return None;
    }
    compared_name(name)?;
    for (show, base) in MYS_SHOWS {
        if let Some(found) = search_mys(show, base, name).await
            && let Some(e) = mys_match(name, &found)
        {
            return Some(mys_card(e));
        }
    }
    None
}

// ---------------------------------------------------------------------
// Swapcard fairs (Informa: Hospitalar...)
// ---------------------------------------------------------------------

/// The card's name, as in standards/databases/global.json.
pub const BRAZIL_LABEL: &str = "Brazil trade show";

/// One fair on Swapcard, as its exhibitor list page asks for it (seen
/// in Bilal's browser, Nov 5).
#[derive(Debug, Clone, Copy)]
pub struct SwapcardFair {
    /// "Hospitalar 2027".
    pub name: &'static str,
    /// The card it gives.
    pub label: &'static str,
    /// "https://app.informamarkets.com.br".
    pub host: &'static str,
    pub view_id: &'static str,
    pub event_id: &'static str,
    /// The fair's own exhibitor list page, linked from the card.
    pub list_page: &'static str,
}

/// The Swapcard fairs read. Add a fair by adding its line (its viewId
/// and eventId are in the payload of its list's "graphql" request).
pub const SWAPCARD_FAIRS: &[SwapcardFair] = &[
    SwapcardFair {
        name: "Hospitalar 2027",
        label: BRAZIL_LABEL,
        host: "https://app.informamarkets.com.br",
        view_id: "RXZlbnRWaWV3XzE4OTYxMTQ=",
        event_id: "RXZlbnRfNDUzNDA5Mw==",
        list_page: "https://www.hospitalar.com/en/i-want-to-visit/exhibitor-list/",
    },
    SwapcardFair {
        name: "Fispal Tecnologia 2026",
        label: BRAZIL_LABEL,
        host: "https://app.informamarkets.com.br",
        view_id: "RXZlbnRWaWV3XzEyMzc0MjI=",
        event_id: "RXZlbnRfMzg5NTY2Mw==",
        list_page: "https://www.fispaltecnologia.com.br/expor/lista-de-expositores/",
    },
    SwapcardFair {
        name: "Fispal Food Service 2026",
        label: BRAZIL_LABEL,
        host: "https://app.informamarkets.com.br",
        // Written without the "=" at the end, as the fair's own page sends it.
        view_id: "RXZlbnRWaWV3XzEyNDY3NDA",
        event_id: "RXZlbnRfNDMxNzQyOQ==",
        list_page: "https://www.fispalfoodservice.com.br/quero-visitar/lista-de-expositores/",
    },
];

/// The fingerprint of the list question Swapcard's server already knows.
const SWAPCARD_LIST_QUERY: &str =
    "589c0fbaa483a657bad0a0af1e5a088746c200f0b1c7dd8520c869aeaba28867";

/// Most pages read for one fair (50 exhibitors each).
const SWAPCARD_MAX_PAGES: usize = 40;

/// One Swapcard exhibitor.
#[derive(Debug, Clone, PartialEq)]
pub struct SwapcardExhibitor {
    pub name: String,
    pub booths: Vec<String>,
}

/// What is sent for one page of the list.
pub fn swapcard_request(fair: &SwapcardFair, end_cursor: Option<&str>) -> Value {
    let mut variables = json!({
        "withEvent": true,
        "viewId": fair.view_id,
        "eventId": fair.event_id
    });
    if let Some(cursor) = end_cursor {
        variables["endCursor"] = json!(cursor);
    }
    json!([{
        "operationName": "EventExhibitorListViewConnectionQuery",
        "variables": variables,
        "extensions": { "persistedQuery": { "version": 1, "sha256Hash": SWAPCARD_LIST_QUERY } }
    }])
}

/// One page of the answer: its exhibitors and the cursor of the next
/// page (None on the last page). None when the answer is not a list.
pub fn swapcard_page(answer: &Value) -> Option<(Vec<SwapcardExhibitor>, Option<String>)> {
    let connection = answer
        .get(0)
        .unwrap_or(answer)
        .pointer("/data/view/exhibitors")?;
    let exhibitors = connection
        .get("nodes")?
        .as_array()?
        .iter()
        .filter_map(|node| {
            let name = text(node, "name");
            if name.is_empty() {
                return None;
            }
            let booths = node
                .pointer("/withEvent/booths")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|b| text(b, "name"))
                .filter(|b| !b.is_empty())
                .collect();
            Some(SwapcardExhibitor { name, booths })
        })
        .collect();
    let page_info = connection.get("pageInfo").unwrap_or(&Value::Null);
    let next = page_info
        .get("hasNextPage")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        .then(|| text(page_info, "endCursor"))
        .filter(|c| !c.is_empty());
    Some((exhibitors, next))
}

/// The exhibitor whose whole name is the seller's name.
pub fn swapcard_match<'a>(
    company_name: &str,
    found: &'a [SwapcardExhibitor],
) -> Option<&'a SwapcardExhibitor> {
    let wanted = compared_name(company_name)?;
    found
        .iter()
        .find(|e| compared_name(&e.name).as_deref() == Some(wanted.as_str()))
}

/// The card for a Swapcard exhibitor.
pub fn swapcard_card(fair: &SwapcardFair, e: &SwapcardExhibitor) -> Signal {
    let booths = match e.booths.len() {
        0 => String::new(),
        1 => format!(" (booth {})", e.booths[0]),
        _ => format!(" (booths {})", e.booths.join(", ")),
    };
    Signal {
        label: fair.label.to_string(),
        sub: format!(
            "\"{}\" is an exhibitor at {}{booths}. The fair checks its exhibitors, so this is a good sign - but it does not prove that this listing is the company's own. Exhibitor list: {}",
            e.name, fair.name, fair.list_page
        ),
        value: MYS_EXHIBITOR.to_string(),
        signal_type: "good".to_string(),
        category: OUTSIDE_RECORDS.to_string(),
        check_type: "existence".to_string(),
    }
}

/// Reads every page of a fair's list. None when the first page fails.
async fn download_swapcard(fair: &SwapcardFair) -> Option<Vec<SwapcardExhibitor>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(BROWSER)
        .build()
        .ok()?;
    let mut all: Vec<SwapcardExhibitor> = Vec::new();
    let mut cursor: Option<String> = None;
    for page in 0..SWAPCARD_MAX_PAGES {
        let response = client
            .post(format!("{}/api/graphql", fair.host))
            .header("Accept", "*/*")
            .header("Origin", fair.host)
            .header("Referer", format!("{}/", fair.host))
            .json(&swapcard_request(fair, cursor.as_deref()))
            .send()
            .await;
        let answer = match response {
            Ok(r) if r.status().is_success() => r.json::<Value>().await.ok(),
            Ok(r) => {
                eprintln!("Safely: {} list answered {}", fair.name, r.status());
                None
            }
            Err(e) => {
                eprintln!("Safely: {} list could not be reached: {}", fair.name, e);
                None
            }
        };
        let Some((exhibitors, next)) = answer.as_ref().and_then(swapcard_page) else {
            eprintln!(
                "Safely: {} list page {} could not be read",
                fair.name,
                page + 1
            );
            break;
        };
        let before = all.len();
        for e in exhibitors {
            if !all.contains(&e) {
                all.push(e);
            }
        }
        // No next page, or the same page again (the cursor was not taken).
        if next.is_none() || next == cursor || all.len() == before {
            break;
        }
        cursor = next;
    }
    eprintln!(
        "Safely: {} exhibitor list read ({} companies)",
        fair.name,
        all.len()
    );
    (!all.is_empty()).then_some(all)
}

/// One fair's list and when it was read.
type HeldList = (std::time::Instant, std::sync::Arc<Vec<SwapcardExhibitor>>);

/// The lists read today, by fair name.
static SWAPCARD_LISTS: std::sync::Mutex<Option<std::collections::HashMap<&'static str, HeldList>>> =
    std::sync::Mutex::new(None);

/// A list is read again after a day; a failed read is tried again after an hour.
const SWAPCARD_KEEP: Duration = Duration::from_secs(24 * 60 * 60);
const SWAPCARD_RETRY: Duration = Duration::from_secs(60 * 60);

async fn swapcard_list(fair: &SwapcardFair) -> Option<std::sync::Arc<Vec<SwapcardExhibitor>>> {
    let held = SWAPCARD_LISTS
        .lock()
        .ok()
        .and_then(|m| m.as_ref().and_then(|m| m.get(fair.name).cloned()));
    if let Some((when, list)) = &held {
        let wait = if list.is_empty() {
            SWAPCARD_RETRY
        } else {
            SWAPCARD_KEEP
        };
        if when.elapsed() < wait {
            return (!list.is_empty()).then(|| list.clone());
        }
    }
    // Marked first, so other scans meanwhile use the old list (or none).
    let old = held.map(|(_, l)| l).unwrap_or_default();
    if let Ok(mut m) = SWAPCARD_LISTS.lock() {
        m.get_or_insert_with(Default::default)
            .insert(fair.name, (std::time::Instant::now(), old.clone()));
    }
    let list = match download_swapcard(fair).await {
        Some(list) => std::sync::Arc::new(list),
        None => old,
    };
    if let Ok(mut m) = SWAPCARD_LISTS.lock() {
        m.get_or_insert_with(Default::default)
            .insert(fair.name, (std::time::Instant::now(), list.clone()));
    }
    (!list.is_empty()).then_some(list)
}

/// The Swapcard fairs' card (the first fair the seller exhibits at), or
/// None when the check is off or it exhibits at none of them.
pub async fn swapcard_signal(company_name: Option<&str>) -> Option<Signal> {
    let name = company_name.map(str::trim).filter(|n| !n.is_empty())?;
    if !enabled() {
        return None;
    }
    compared_name(name)?;
    for fair in SWAPCARD_FAIRS {
        if let Some(list) = swapcard_list(fair).await
            && let Some(e) = swapcard_match(name, &list)
        {
            return Some(swapcard_card(fair, e));
        }
    }
    None
}

// ---------------------------------------------------------------------
// Fairs on Algolia (RX: Feicon; Comexposium: SIAL Paris...)
// ---------------------------------------------------------------------

/// One fair searched on Algolia, as its exhibitor list page searches
/// it (seen in Bilal's browser, Nov 5).
#[derive(Debug, Clone, Copy)]
pub struct AlgoliaFair {
    /// "Feicon 2027".
    pub name: &'static str,
    pub label: &'static str,
    /// Algolia's application ("XD0U5M6Y4R").
    pub app_id: &'static str,
    /// The fair's PUBLIC search key, the one its page gives every
    /// visitor's browser. Not a secret and not Safely's own key.
    pub search_key: &'static str,
    pub index: &'static str,
    /// Algolia filters the fair's page sends (only its exhibitors of
    /// this edition); empty when the list holds only this edition.
    pub filters: &'static str,
    pub list_page: &'static str,
    /// The address of one exhibitor's page, "{slug}" replaced by the
    /// record's slug, or "{query}" by the company's name; empty when the
    /// fair has none (the list is linked).
    pub exhibitor_page: &'static str,
    /// Where the names are in a record ("/exhibitor/name"); empty: any
    /// top-level field with "name" in it.
    pub name_fields: &'static [&'static str],
}

/// The card's name for French fairs.
pub const FRANCE_LABEL: &str = "France trade show";

/// The RX fairs searched. Add a fair by adding its line (all the codes
/// are in the Request URL and Payload of its list's Algolia "query").
pub const ALGOLIA_FAIRS: &[AlgoliaFair] = &[
    AlgoliaFair {
        name: "Feicon 2027",
        label: BRAZIL_LABEL,
        app_id: "XD0U5M6Y4R",
        search_key: "d5cd7d4ec26134ff4a34d736a7f9ad47",
        index: "evt-515eaf3f-a833-42fe-bb67-c1cd154e6401-index",
        filters: "recordType:exhibitor AND locale:pt-br AND eventEditionId:eve-72df616a-7258-48e7-909e-51b2b53cf9c4",
        list_page: "https://www.feicon.com.br/pt-br/Expositores.html",
        exhibitor_page: "",
        name_fields: &[],
    },
    AlgoliaFair {
        name: "SIAL Paris 2026",
        label: FRANCE_LABEL,
        app_id: "W1BGHM6UJN",
        search_key: "deb06c31b0495c80bcbdf0077536d647",
        index: "catalog.prod.sial.exhibitors.en",
        filters: "",
        list_page: "https://www.sialparis.com/en/exhibitors-2026",
        exhibitor_page: "https://www.sialparis.com/en/exhibitors-2026/exhibitor/{slug}",
        name_fields: &[],
    },
    AlgoliaFair {
        name: "Maison&Objet Paris",
        label: FRANCE_LABEL,
        app_id: "LPN4STGLQG",
        search_key: "3dfb0b32b6d93bd5a34dceabcc437108",
        index: "InsertionProduct_hall_asc",
        // Exhibitor records only (products are tagged "product").
        filters: "_tags:insertion",
        list_page: "https://www.maison-objet.com/en/paris/exhibitors",
        exhibitor_page: "https://www.maison-objet.com/en/paris/exhibitors?query={query}",
        // The company, and the brand it shows under.
        name_fields: &["/exhibitor/name", "/brand/name"],
    },
];

/// What is sent: the seller's name, only this fair's exhibitors, no
/// typo tolerance (the same choices the fair's own page makes).
pub fn algolia_request(fair: &AlgoliaFair, company_name: &str) -> Value {
    let filters = if fair.filters.is_empty() {
        String::new()
    } else {
        format!("&filters={}", urlencoding::encode(fair.filters))
    };
    json!({
        "params": format!(
            "query={}&hitsPerPage=20{filters}&typoTolerance=false",
            urlencoding::encode(company_name.trim()),
        )
    })
}

/// Every text (or list of texts) in a record whose field name contains
/// one of `words` ("name", "stand"...). The fair's records are read this
/// way so a renamed field does not break the check.
fn texts_where(record: &Value, words: &[&str]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let Some(fields) = record.as_object() else {
        return found;
    };
    for (key, value) in fields {
        let key = key.to_ascii_lowercase();
        if !words.iter().any(|w| key.contains(w)) {
            continue;
        }
        let mut add = |t: &str| {
            let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
            if !t.is_empty() && !found.contains(&t) {
                found.push(t);
            }
        };
        match value {
            Value::String(t) => add(t),
            Value::Array(items) => items.iter().filter_map(Value::as_str).for_each(&mut add),
            _ => {}
        }
    }
    found
}

/// Stands written as objects (SIAL: {"hall": "8", "aisle": "L",
/// "number": "014"} -> "8 L014").
fn stand_objects(record: &Value) -> Vec<String> {
    record
        .get("stands")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|s| s.is_object())
        .map(|s| {
            let place = format!("{}{}", text(s, "aisle"), text(s, "number"));
            [text(s, "hall"), place]
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// One exhibitor found on Algolia.
#[derive(Debug, Clone, PartialEq)]
pub struct AlgoliaExhibitor {
    pub name: String,
    pub booths: Vec<String>,
    /// SIAL: "BAIKA" from {"slug": {"value": "BAIKA"}}.
    pub slug: String,
}

/// The exhibitor in an Algolia answer whose whole name (any field with
/// "name" in it) is the seller's name. None when there is none; the
/// answer not being a search answer is None too.
pub fn algolia_match(company_name: &str, answer: &Value) -> Option<AlgoliaExhibitor> {
    algolia_match_in(company_name, answer, &[])
}

/// The same, reading the names at `name_fields` (JSON pointers) when
/// given.
pub fn algolia_match_in(
    company_name: &str,
    answer: &Value,
    name_fields: &[&str],
) -> Option<AlgoliaExhibitor> {
    let wanted = compared_name(company_name)?;
    let hits = answer.get("hits")?.as_array()?;
    hits.iter().find_map(|hit| {
        let names = if name_fields.is_empty() {
            texts_where(hit, &["name"])
        } else {
            name_fields
                .iter()
                .filter_map(|f| hit.pointer(f).and_then(Value::as_str))
                .map(|n| n.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|n| !n.is_empty())
                .collect()
        };
        let name = names
            .into_iter()
            .find(|n| compared_name(n).as_deref() == Some(wanted.as_str()))?;
        let mut booths = texts_where(hit, &["stand", "booth"]);
        for b in stand_objects(hit) {
            if !booths.contains(&b) {
                booths.push(b);
            }
        }
        let slug = match hit.get("slug") {
            Some(Value::String(s)) => s.trim().to_string(),
            Some(v) => text(v, "value"),
            None => String::new(),
        };
        // Maison&Objet: the hall, when given.
        if booths.is_empty() {
            let hall = text(hit, "hall");
            if !hall.is_empty() {
                booths.push(format!("hall {hall}"));
            }
        }
        Some(AlgoliaExhibitor { name, booths, slug })
    })
}

/// The card for an RX exhibitor.
pub fn algolia_card(fair: &AlgoliaFair, e: &AlgoliaExhibitor) -> Signal {
    let link = if fair.exhibitor_page.contains("{query}") {
        format!(
            "Exhibitor list: {}",
            fair.exhibitor_page
                .replace("{query}", &urlencoding::encode(&e.name))
        )
    } else if fair.exhibitor_page.is_empty() || e.slug.is_empty() {
        format!("Exhibitor list: {}", fair.list_page)
    } else {
        format!(
            "Exhibitor page: {}",
            fair.exhibitor_page
                .replace("{slug}", &urlencoding::encode(&e.slug))
        )
    };
    let booths = match e.booths.len() {
        0 => String::new(),
        1 => format!(" (booth {})", e.booths[0]),
        _ => format!(" (booths {})", e.booths.join(", ")),
    };
    Signal {
        label: fair.label.to_string(),
        sub: format!(
            "\"{}\" is an exhibitor at {}{booths}. The fair checks its exhibitors, so this is a good sign - but it does not prove that this listing is the company's own. {}",
            e.name, fair.name, link
        ),
        value: MYS_EXHIBITOR.to_string(),
        signal_type: "good".to_string(),
        category: OUTSIDE_RECORDS.to_string(),
        check_type: "existence".to_string(),
    }
}

async fn search_algolia(fair: &AlgoliaFair, company_name: &str) -> Option<Value> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent(BROWSER)
        .build()
        .ok()?;
    let url = format!(
        "https://{}-dsn.algolia.net/1/indexes/{}/query",
        fair.app_id.to_ascii_lowercase(),
        fair.index
    );
    match client
        .post(url)
        .header("X-Algolia-Application-Id", fair.app_id)
        .header("X-Algolia-API-Key", fair.search_key)
        .json(&algolia_request(fair, company_name))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r.json::<Value>().await.ok(),
        Ok(r) => {
            eprintln!("Safely: {} search answered {}", fair.name, r.status());
            None
        }
        Err(e) => {
            eprintln!("Safely: {} search could not be reached: {}", fair.name, e);
            None
        }
    }
}

/// The RX fairs' card (the first fair the seller exhibits at), or None
/// when the check is off or it exhibits at none of them.
pub async fn algolia_signal(company_name: Option<&str>) -> Option<Signal> {
    let name = company_name.map(str::trim).filter(|n| !n.is_empty())?;
    if !enabled() {
        return None;
    }
    compared_name(name)?;
    for fair in ALGOLIA_FAIRS {
        let Some(answer) = search_algolia(fair, name).await else {
            continue;
        };
        if let Some(e) = algolia_match_in(name, &answer, fair.name_fields) {
            return Some(algolia_card(fair, &e));
        }
        // Found something but no field with the name: the fair's record
        // has changed shape. Logged with the field names only.
        if let Some(first) = answer.pointer("/hits/0").and_then(Value::as_object)
            && texts_where(&Value::Object(first.clone()), &["name"]).is_empty()
        {
            eprintln!(
                "Safely: {} answer has no name field; fields: {}",
                fair.name,
                first.keys().cloned().collect::<Vec<_>>().join(", ")
            );
        }
    }
    None
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

    /// The parts of the real HKTDC search page Bilal saved on Nov 4
    /// ("Shenzhen" at the Electronics Fair, Spring Edition).
    fn hktdc_page() -> String {
        let data = json!({
          "props": { "pageProps": {
            "fairOption": { "fairName": "HKTDC Hong Kong Electronics Fair (Spring Edition)",
                            "fairCode": "hkelectronicsfairse" },
            "exhibitorListData": { "from": 0, "size": 10, "totalSize": 213, "searchKeyword": "Shenzhen",
              "data": [
                { "exhibitorName": "SHENZHEN TRENDWOO TECH CO LTD", "boothNumbers": "1D-C01",
                  "countryDesc": "Chinese Mainland", "exhibitorUrn": "1S005V6OU",
                  "fairSymbol": "hkelectronicsfairse",
                  "supplierSriDetail": [
                    { "supplierRefKey": "Exhibited in HKTDC Event(s) in recent years",
                      "supplierRefValue": "HKTDC Hong Kong Electronics Fair 2026 (Autumn Edition)" },
                    { "supplierRefKey": "Exhibited in HKTDC Event(s) in recent years",
                      "supplierRefValue": "HKTDC Hong Kong Electronics Fair (Spring Edition) 2026" },
                    { "supplierRefKey": "Exhibited in HKTDC Event(s) in recent years",
                      "supplierRefValue": "HKTDC Hong Kong Electronics Fair 2025 (Autumn Edition)" },
                    { "supplierRefKey": "Exhibited in HKTDC Event(s) in recent years",
                      "supplierRefValue": "HKTDC Hong Kong Electronics Fair (Spring Edition) 2025" },
                    { "supplierRefKey": "Exhibited in HKTDC Event(s) in recent years",
                      "supplierRefValue": "HKTDC Hong Kong Electronics Fair 2024 (Autumn Edition)" }
                  ] },
                { "exhibitorName": "Shenzhen Kingstar Technology Co., Ltd.", "boothNumbers": "1CON-S003, 1E-A01",
                  "exhibitorUrn": "1S005W1N2", "fairSymbol": "hkelectronicsfairse" }
              ] } } },
          "page": "/[fairCode]/[locale]/exhibitor-search/[[...exhibitorSlug]]"
        });
        format!(
            "<!DOCTYPE html><html><head></head><body><div id=\"__next\"></div><script id=\"__NEXT_DATA__\" type=\"application/json\">{data}</script></body></html>"
        )
    }

    #[test]
    fn an_hktdc_search_page_is_read() {
        let found = hktdc_exhibitors(&hktdc_page()).unwrap();
        assert_eq!(found.len(), 2);
        let e = &found[0];
        assert_eq!(
            e.fair_name,
            "HKTDC Hong Kong Electronics Fair (Spring Edition)"
        );
        assert_eq!(e.booths, "1D-C01");
        assert_eq!(e.past_fairs.len(), 5);
        assert_eq!(
            e.page(),
            "https://www.hktdc.com/event/hkelectronicsfairse/en/exhibitor/1S005V6OU?fairCode=hkelectronicsfairse"
        );
        assert!(hktdc_exhibitors("<html><body>No data</body></html>").is_none());
    }

    #[test]
    fn hktdc_whole_names_and_card() {
        let found = hktdc_exhibitors(&hktdc_page()).unwrap();
        let e = hktdc_match("Shenzhen Trendwoo Tech Co., Ltd.", &found).unwrap();
        let card = hktdc_card(e);
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (HKTDC_EXHIBITOR, "good")
        );
        assert!(
            card.sub
                .contains("HKTDC Hong Kong Electronics Fair (Spring Edition) (booth 1D-C01)")
        );
        assert!(card.sub.contains("; and 1 more."));
        assert!(hktdc_match("Shenzhen Trendwoo", &found).is_none());
        assert!(hktdc_match("Trendwoo Trading Co., Ltd.", &found).is_none());
    }

    #[test]
    fn the_hktdc_address_holds_the_name() {
        assert_eq!(
            hktdc_search_url("hkelectronicsfairse", "Shenzhen Trendwoo Tech Co., Ltd./HK"),
            "https://www.hktdc.com/event/hkelectronicsfairse/en/exhibitor-search/Shenzhen%20Trendwoo%20Tech%20Co%20Ltd%20HK"
        );
    }

    /// Part of Bilal's real CES answer for "samsung" (Nov 4).
    fn ces_answer() -> Value {
        json!({
          "SUCCESS": true, "ERRORMESSAGE": "",
          "DATA": { "searchtext": "samsung", "results": { "exhibitor": { "found": 3, "hit": [
            { "id": "0013A00001a0z5XQAQ-Exhibitor-CES27",
              "fields": { "exhname_t": "Samsung SDS", "exhid_l": "0013A00001a0z5XQAQ",
                          "boothsdisplay_la": ["N110randomstring"] } },
            { "id": "0013000001KaMXYAA3-Exhibitor-CES27",
              "fields": { "exhname_t": "Samsung Electronics, Inc.", "exhid_l": "0013000001KaMXYAA3",
                          "boothsdisplay_la": ["Alsacerandomstring", "Bandol 2randomstring",
                            "Castillonrandomstring", "Fleurierandomstring", "Hermitagerandomstring"] } },
            { "id": "001Pp000023KAa3IAG-Exhibitor-CES27",
              "fields": { "exhname_t": "Samsung Electronics America", "exhid_l": "001Pp000023KAa3IAG" } }
          ] } } }
        })
    }

    #[test]
    fn a_ces_answer_is_read() {
        let base = "https://exhibitors.ces.tech/8_0";
        let found = mys_exhibitors("CES", base, &ces_answer()).unwrap();
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].booths, vec!["N110"]);
        assert_eq!(found[0].show, "CES 2027");
        assert_eq!(
            found[1].page,
            "https://exhibitors.ces.tech/8_0/exhibitor/exhibitor-details.cfm?exhid=0013000001KaMXYAA3"
        );
        assert!(found[2].booths.is_empty());
        assert!(mys_exhibitors("CES", base, &json!({ "SUCCESS": false })).is_none());
        assert_eq!(
            mys_search_url(base, "Samsung SDS"),
            "https://exhibitors.ces.tech/8_0/ajax/remote-proxy.cfm?action=search&search=Samsung%20SDS&searchtype=keyword&show=all"
        );
    }

    #[test]
    fn ces_whole_names_and_card() {
        let found =
            mys_exhibitors("CES", "https://exhibitors.ces.tech/8_0", &ces_answer()).unwrap();
        let e = mys_match("Samsung Electronics Inc", &found).unwrap();
        let card = mys_card(e);
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (MYS_EXHIBITOR, "good")
        );
        assert!(
            card.sub
                .contains("CES 2027 (booths Alsace, Bandol 2, Castillon and 2 more)")
        );
        let card = mys_card(mys_match("Samsung SDS", &found).unwrap());
        assert!(card.sub.contains("(booth N110)"));
        assert!(mys_match("Samsung", &found).is_none());
    }

    /// Part of Bilal's real Hospitalar answer (Nov 5): page 1 of 568.
    fn hospitalar_answer() -> Value {
        json!([{ "data": { "view": { "id": "RXZlbnRWaWV3XzE4OTYxMTQ=", "exhibitors": {
          "nodes": [
            { "id": "RXhoaWJpdG9yXzI1MTgzNjY=", "name": "ACCUMED GLICOMED", "type": "Premium",
              "withEvent": { "booths": [ { "name": "E-160", "__typename": "Core_Location" } ] } },
            { "name": "NINGBO DAVID MEDICAL DEVICE CO., LTD.", "withEvent": { "booths": [] } },
            { "name": "", "withEvent": null }
          ],
          "pageInfo": { "hasNextPage": true,
            "endCursor": "WzEuMDQ3NjE5MDQ3NjE5MDQ3NywidGFyZ2V0IDIwMjciLCJtYWQudSIsIlJYaG9hV0pwZEc5eVh6STFNVGd6TVRJPSJd" },
          "totalCount": 568 } } } }])
    }

    #[test]
    fn a_swapcard_page_is_read() {
        let (found, next) = swapcard_page(&hospitalar_answer()).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].booths, vec!["E-160"]);
        assert!(next.unwrap().starts_with("WzEuMDQ3"));
        // The last page has no next cursor.
        let last = json!([{ "data": { "view": { "exhibitors": {
            "nodes": [ { "name": "PHILIPS" } ], "pageInfo": { "hasNextPage": false, "endCursor": "x" } } } } }]);
        assert_eq!(swapcard_page(&last).unwrap().1, None);
        assert!(swapcard_page(&json!([{ "errors": [ { "message": "no" } ] }])).is_none());
    }

    #[test]
    fn the_brazil_swapcard_fairs_are_listed_with_their_own_codes() {
        let names: Vec<&str> = SWAPCARD_FAIRS.iter().map(|f| f.name).collect();
        assert_eq!(
            names,
            vec![
                "Hospitalar 2027",
                "Fispal Tecnologia 2026",
                "Fispal Food Service 2026"
            ]
        );
        // Each fair's codes as Bilal captured them (Nov 5).
        assert_eq!(SWAPCARD_FAIRS[1].event_id, "RXZlbnRfMzg5NTY2Mw==");
        assert_eq!(SWAPCARD_FAIRS[2].view_id, "RXZlbnRWaWV3XzEyNDY3NDA");
        assert_eq!(SWAPCARD_FAIRS[2].event_id, "RXZlbnRfNDMxNzQyOQ==");
    }

    #[test]
    fn the_swapcard_request_is_the_pages_own() {
        let fair = &SWAPCARD_FAIRS[0];
        let first = swapcard_request(fair, None);
        assert_eq!(
            first[0]["operationName"],
            "EventExhibitorListViewConnectionQuery"
        );
        assert_eq!(first[0]["variables"]["viewId"], "RXZlbnRWaWV3XzE4OTYxMTQ=");
        assert_eq!(first[0]["variables"]["eventId"], "RXZlbnRfNDUzNDA5Mw==");
        assert!(first[0]["variables"].get("endCursor").is_none());
        assert_eq!(
            first[0]["extensions"]["persistedQuery"]["sha256Hash"],
            "589c0fbaa483a657bad0a0af1e5a088746c200f0b1c7dd8520c869aeaba28867"
        );
        assert_eq!(
            swapcard_request(fair, Some("abc"))[0]["variables"]["endCursor"],
            "abc"
        );
    }

    #[test]
    fn swapcard_whole_names_and_card() {
        let (found, _) = swapcard_page(&hospitalar_answer()).unwrap();
        let fair = &SWAPCARD_FAIRS[0];
        let card = swapcard_card(fair, swapcard_match("Accumed Glicomed", &found).unwrap());
        assert_eq!(
            (
                card.label.as_str(),
                card.value.as_str(),
                card.signal_type.as_str()
            ),
            (BRAZIL_LABEL, MYS_EXHIBITOR, "good")
        );
        assert!(
            card.sub
                .contains("exhibitor at Hospitalar 2027 (booth E-160)")
        );
        assert!(swapcard_match("Ningbo David Medical Device Co., Ltd", &found).is_some());
        assert!(swapcard_match("Accumed", &found).is_none());
    }

    #[test]
    fn the_algolia_search_is_the_fairs_own() {
        let fair = &ALGOLIA_FAIRS[0];
        let body = algolia_request(fair, "MarGirius Continental");
        let params = body["params"].as_str().unwrap();
        assert!(params.starts_with("query=MarGirius%20Continental&"));
        assert!(params.contains("typoTolerance=false"));
        // The same filter the fair's page sends (Bilal's capture, Nov 5).
        assert!(params.contains(
            "filters=recordType%3Aexhibitor%20AND%20locale%3Apt-br%20AND%20eventEditionId%3Aeve-72df616a-7258-48e7-909e-51b2b53cf9c4"
        ));
    }

    #[test]
    fn an_algolia_answer_is_read_by_its_name_fields() {
        let answer = json!({ "nbHits": 2, "hits": [
            { "objectID": "exh-1_pt-br", "companyName": "MarGirius Continental", "standReference": "G050" },
            { "objectID": "exh-2_pt-br", "name": "BRANCO MOTORES LTDA", "stands": ["E230", "E231"] }
        ]});
        let e = algolia_match("Margirius Continental Ltda", &answer).unwrap();
        assert_eq!(e.booths, vec!["G050"]);
        let card = algolia_card(&ALGOLIA_FAIRS[0], &e);
        assert_eq!(
            (
                card.label.as_str(),
                card.value.as_str(),
                card.signal_type.as_str()
            ),
            (BRAZIL_LABEL, MYS_EXHIBITOR, "good")
        );
        assert!(card.sub.contains("exhibitor at Feicon 2027 (booth G050)"));
        let e = algolia_match("Branco Motores", &answer).unwrap();
        assert_eq!(e.booths, vec!["E230", "E231"]);
        assert!(algolia_match("MarGirius", &answer).is_none());
        // Only an ID came back (what the page itself asks for): no match.
        let ids_only =
            json!({ "hits": [ { "objectID": "exh-ad0b9a02-2f64-48ff-8a32-03128c7e2b7b_pt-br" } ] });
        assert!(algolia_match("MarGirius Continental", &ids_only).is_none());
        assert!(
            algolia_match(
                "MarGirius Continental",
                &json!({ "message": "Invalid API key" })
            )
            .is_none()
        );
    }

    /// Part of Bilal's real SIAL answer for "baika" (Nov 5).
    fn sial_answer() -> Value {
        json!({ "index": "catalog.prod.sial.exhibitors.en", "nbHits": 13, "hits": [
          { "id": "fde9041b-2d79-f111-a8d9-005056ace99d", "name": "BAIKA",
            "slug": { "value": "BAIKA", "search": "baika", "history": [] },
            "address": { "city": "SAN FELIPE", "country": "Chile" },
            "session": { "providerCatalogId": "ASIAL26" },
            "stands": [ { "sector": "Fresh fruit and vegetables, dried fruit and horticulture",
                          "hall": "8", "hall_id": "11576", "aisle": "L", "number": "014", "ref": "4R6A3JKV" } ],
            "standCount": 1 },
          { "name": "Barkat Rice Mills (Pvt)Ltd",
            "slug": { "value": "Barkat-Rice-Mills-PvtLtd" },
            "stands": [ { "hall": "4", "aisle": "L", "number": "195" } ] }
        ]})
    }

    #[test]
    fn a_sial_answer_is_read() {
        let sial = &ALGOLIA_FAIRS[1];
        assert_eq!(sial.name, "SIAL Paris 2026");
        let body = algolia_request(sial, "BAIKA");
        assert_eq!(
            body["params"],
            "query=BAIKA&hitsPerPage=20&typoTolerance=false"
        );
        let e = algolia_match("Baika", &sial_answer()).unwrap();
        assert_eq!(e.booths, vec!["8 L014"]);
        assert_eq!(e.slug, "BAIKA");
        let card = algolia_card(sial, &e);
        assert_eq!(
            (card.label.as_str(), card.value.as_str()),
            (FRANCE_LABEL, MYS_EXHIBITOR)
        );
        assert!(
            card.sub
                .contains("exhibitor at SIAL Paris 2026 (booth 8 L014)")
        );
        assert!(card.sub.contains(
            "Exhibitor page: https://www.sialparis.com/en/exhibitors-2026/exhibitor/BAIKA"
        ));
        let e = algolia_match("Barkat Rice Mills (Pvt) Ltd", &sial_answer()).unwrap();
        assert_eq!(e.booths, vec!["4 L195"]);
        assert!(algolia_match("Baika Foods", &sial_answer()).is_none());
    }

    /// Part of Bilal's real Maison&Objet answer for "hamam suite" (Nov 5).
    fn mo_answer() -> Value {
        json!({ "hits": [
          { "name_fr": null, "name_en": null,
            "brand": { "id": 111303, "name": "HAMAM SUITE", "slug": "hamam-suite" },
            "exhibitor": { "name": "VERITAS TEKSTIL KON. PAZ. SAN. TIC. A.S.", "country_code": "TR", "country_en": "Türkiye" },
            "hall": null, "stands": null, "sector": "DECOR & DESIGN", "_tags": ["insertion"],
            "objectID": "insertion_111303" },
          { "name_en": "Bed Collections", "brand": { "name": "HAMAM", "slug": "hamam" },
            "exhibitor": { "country_code": "TR" }, "_tags": ["product"] }
        ]})
    }

    #[test]
    fn a_maison_objet_answer_is_read_by_company_or_brand() {
        let mo = &ALGOLIA_FAIRS[2];
        assert_eq!(mo.name, "Maison&Objet Paris");
        assert_eq!(
            algolia_request(mo, "Hamam Suite")["params"],
            "query=Hamam%20Suite&hitsPerPage=20&filters=_tags%3Ainsertion&typoTolerance=false"
        );
        let by_company = algolia_match_in(
            "Veritas Tekstil Kon. Paz. San. Tic. A.S.",
            &mo_answer(),
            mo.name_fields,
        )
        .unwrap();
        assert_eq!(by_company.name, "VERITAS TEKSTIL KON. PAZ. SAN. TIC. A.S.");
        let by_brand = algolia_match_in("Hamam Suite", &mo_answer(), mo.name_fields).unwrap();
        assert_eq!(by_brand.name, "HAMAM SUITE");
        let card = algolia_card(mo, &by_brand);
        assert_eq!(card.label, FRANCE_LABEL);
        assert!(card.sub.contains("exhibitor at Maison&Objet Paris."));
        assert!(card.sub.contains("exhibitors?query=HAMAM%20SUITE"));
        // A product's own name is not a company.
        assert!(algolia_match_in("Bed Collections", &mo_answer(), mo.name_fields).is_none());
    }

    #[test]
    fn aeo_exhibitors_get_their_own_card() {
        let answer = json!({ "errCode": 0, "result": { "itemList": [
            { "shopNo": "1", "name": "NINGBO A CO.,LTD.", "isAeo": "Y", "tagDescList": [] },
            { "shopNo": "2", "name": "NINGBO B CO.,LTD.", "isAeo": null, "tagDescList": ["AEO"] },
            { "shopNo": "3", "name": "NINGBO C CO.,LTD.", "isAeo": "N", "tagDescList": ["Brand Enterprise"] },
            { "shopNo": "4", "name": "NINGBO D CO.,LTD.", "isAeo": null, "tagDescList": ["Multiple-session Exhibitor"] }
        ]}});
        let found = canton_exhibitors(&answer).unwrap();
        assert_eq!(
            found.iter().map(|e| e.aeo).collect::<Vec<_>>(),
            vec![true, true, false, false]
        );
        let card = aeo_card(&found[0]).unwrap();
        assert_eq!(
            (
                card.label.as_str(),
                card.value.as_str(),
                card.signal_type.as_str()
            ),
            (AEO_LABEL, AEO_CERTIFIED, "good")
        );
        assert_eq!(card.category, OUTSIDE_RECORDS);
        assert!(card.sub.contains("shops?id=1"));
        assert!(aeo_card(&found[2]).is_none());
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
                .is_empty()
        );
        if enabled() {
            return;
        }
        assert!(
            canton_signal(Some("Midea Group"), Some("Guangdong, China"), None)
                .await
                .is_empty()
        );
    }
}
