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
