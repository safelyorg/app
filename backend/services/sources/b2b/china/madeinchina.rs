//! Made-in-China.com: Chinese suppliers, used by many US buyers.
//!
//! Two kinds of page are scanned:
//! - a product page: https://{shop}.en.made-in-china.com/product/{id}/....html
//!   or https://www.made-in-china.com/product/....html
//! - the supplier's own page: https://{shop}.en.made-in-china.com/
//!
//! Everything is in the page's plain HTML (no hidden app data):
//! - the product: the page's schema.org "Product" data (name, photos,
//!   summary), the price box and the product details table;
//! - the supplier: label / value pairs ("Address", "Terms of Payment",
//!   "Year of Establishment"...), the member level ("Diamond Member",
//!   "Since 2025", "2 yrs"), the audit note ("audited by SGS ... with ID
//!   QIP-ASI269732"), the business licence note and the contact person.
//!
//! Made-in-China has no "Verified" badge. Its own checks are an on-site
//! audit by an outside company (TÜV Rheinland, SGS...) and a check of
//! the business licence. "Diamond Member" / "Gold Member" are paid
//! membership levels, never a check on the company.

use crate::services::sources::b2b::{
    B2bListingProfile, B2bScraper, B2bSupplierProfile, SupplierRecord,
};
use chrono::{Datelike, Utc};
use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};
use serde_json::Value;
use std::sync::LazyLock;

pub struct MadeinchinaScraper;

const PLATFORM: &str = "madeinchina";
const PLATFORM_NAME: &str = "Made-in-China";

/// Every supplier shop is "{shop}.en.made-in-china.com".
const SHOP_HOST_END: &str = ".made-in-china.com";

/// Longest company profile text kept (the rest is cut). Some suppliers
/// paste several pages of text.
const MAX_PROFILE_CHARS: usize = 3000;

/// Most products listed from a supplier's own page.
const MAX_SHOP_PRODUCTS: usize = 8;

/// Most product details ("Condition: Used") added to the description.
const MAX_PRODUCT_DETAILS: usize = 15;

/// Most other company names quoted from the company profile.
const MAX_OTHER_NAMES: usize = 3;

/// The line Made-in-China shows when it checked the business licence.
const LICENCE_LINE: &str = "Suppliers with verified business licenses";

/// The start of the badge text when the supplier was audited on site.
pub const AUDITED_PREFIX: &str = "Audited";

/// The badge text when only the business licence was checked.
pub const LICENCE_CHECKED: &str = "Business licence checked";

/// Label / value pairs on Made-in-China pages, as (label element, class
/// of the value element right after it):
/// - product page, "About Our Factory & Business Background";
/// - supplier page, the header box and the "Authentication" box;
/// - product page, the product details table (two layouts).
const LABEL_PAIRS: &[(&str, &str)] = &[
    ("dt.sr-comProfile-label", "sr-comProfile-fields"),
    ("div.info-label", "info-fields"),
    ("span.item-l", "item-v"),
    ("dt.product-attrs-label", "product-attrs-value"),
    (".bac-item-label", "bac-item-value"),
];

/// Only the product details table (both layouts).
const PRODUCT_DETAIL_PAIRS: &[(&str, &str)] = &[
    ("dt.product-attrs-label", "product-attrs-value"),
    (".bac-item-label", "bac-item-value"),
];

/// Company facts put in the description, as (label on the page, name
/// used in the description), in this order.
const COMPANY_FACTS: &[(&str, &str)] = &[
    ("Business Type", "Business type"),
    ("Year of Establishment", "Year of establishment"),
    ("Registered Capital", "Registered capital"),
    ("Number of Employees", "Employees"),
    ("Plant Area", "Plant area"),
    ("Number of Foreign Trading Staff", "Foreign trade staff"),
    ("Export Year", "Export year"),
    ("Main Markets", "Main markets"),
    ("Repeat Buyers Choice", "Repeat buyers"),
    ("Main Products", "Main products"),
];

/// Start of the description lines this scraper writes.
const ADDRESS_PREFIX: &str = "Address: ";
const REGION_PREFIX: &str = "Region shown by Made-in-China: ";
const PLATFORM_PREFIX: &str = "On Made-in-China: ";
const OTHER_NAMES_PREFIX: &str = "The company profile also names other companies: ";
const SHOP_PRODUCTS_PREFIX: &str = "Products on its Made-in-China page: ";
const PRODUCT_DETAILS_PREFIX: &str = "Product details: ";
const FAQ_PREFIX: &str = "Supplier's FAQ: ";
const OWN_TEXT_PREFIX: &str = "Supplier's own product text: ";
const PAYMENT_WORDS_PREFIX: &str = "Payment, in the supplier's own words: ";

/// Longest part of the FAQ and of the supplier's own product text kept
/// in the description (the rest is cut).
const MAX_FAQ_CHARS: usize = 2000;
const MAX_OWN_TEXT_CHARS: usize = 2500;

/// Most payment sentences taken from the FAQ and the product text.
const MAX_PAYMENT_NOTES: usize = 3;

static YEARS_ON_PLATFORM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{1,2}) yrs?$").unwrap());
static MEMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([A-Z][a-z]+) Member$").unwrap());
static MEMBER_SINCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^Since (\d{4})$").unwrap());
static REVIEWS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\((\d+) Reviews?\)$").unwrap());
static RATING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d(?:\.\d)?)(?:/5)?$").unwrap());
static REGION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Z][A-Za-z ]+, China$").unwrap());
static REPORT_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"with ID "\s*([A-Za-z0-9-]+)"#).unwrap());
static YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(19\d\d|20\d\d)\b").unwrap());
static PRICE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"US ?\$ ?[\d,.]+(?:\s*-\s*[\d,.]+)?").unwrap());
static FIRST_QUANTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d[\d,]*)[^A-Za-z]*([A-Za-z]+)?").unwrap());
/// A company name with a legal ending: "Wafangdian Jinrui Bearing Co., Ltd.".
/// Every word before the ending starts with a capital letter.
static COMPANY_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b((?:[A-Z][A-Za-z&'\-]*\.? ){1,8}(?:Co\.?,? ?Ltd\.?|Company Limited|Co\. Limited|Limited|Inc\.|Corporation|Corp\.|LLC))",
    )
    .unwrap()
});

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("a valid CSS selector")
}

/// The text with every run of spaces and line breaks made one space.
fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn text_of(el: &ElementRef) -> String {
    squash(&el.text().collect::<Vec<_>>().join(" "))
}

/// A value as shown, or None when it is empty. "Bearing , Roller" (a
/// list with a space before each comma) becomes "Bearing, Roller".
fn clean_value(text: &str) -> Option<String> {
    let text = squash(text).replace(" ,", ",");
    let text = text.trim().trim_start_matches(':').trim();
    (!text.is_empty() && text != "-").then(|| text.to_string())
}

/// "//image.made-in-china.com/x.jpg" -> "https://image.made-in-china.com/x.jpg".
fn absolute_url(url: &str) -> String {
    let url = url.trim();
    if url.starts_with("//") {
        format!("https:{url}")
    } else {
        url.to_string()
    }
}

/// "https://yongjie-bearing.en.made-in-china.com/product/..." ->
/// "yongjie-bearing.en.made-in-china.com". Only supplier shops count;
/// the main site (www.made-in-china.com) is not a company.
fn shop_host(url: &str) -> Option<String> {
    let url = absolute_url(url);
    let after = url.split("://").nth(1)?;
    let host = after.split(['/', '?', '#']).next()?.to_lowercase();
    let is_shop = host.ends_with(SHOP_HOST_END) && !host.starts_with("www.");
    is_shop.then_some(host)
}

/// The text the page shows, one piece per line, without scripts and
/// styles.
fn visible_lines(document: &Html) -> Vec<String> {
    let mut lines = Vec::new();
    for node in document.root_element().descendants() {
        let Node::Text(text) = node.value() else {
            continue;
        };
        let hidden = node.ancestors().any(|a| {
            matches!(a.value(), Node::Element(e)
                if matches!(e.name(), "script" | "style" | "noscript" | "template" | "head"))
        });
        if hidden {
            continue;
        }
        let line = squash(text);
        if !line.is_empty() {
            lines.push(line);
        }
    }
    lines
}

/// Every label / value pair from `pairs`, in page order. When a label
/// shows up twice, the first value is kept.
fn labeled_values(document: &Html, pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    for (label_css, value_class) in pairs {
        for label in document.select(&selector(label_css)) {
            let Some(value) = label.next_siblings().find_map(ElementRef::wrap) else {
                continue;
            };
            if !value.value().classes().any(|c| c == *value_class) {
                continue;
            }
            let name = text_of(&label).trim_end_matches(':').trim().to_string();
            if name.is_empty() || found.iter().any(|(n, _)| *n == name) {
                continue;
            }
            if let Some(v) = clean_value(&text_of(&value)) {
                found.push((name, v));
            }
        }
    }
    found
}

/// The page's schema.org data blocks (Product, Organization...). A block
/// that is not valid JSON is skipped (one product page has an empty
/// one).
fn page_data(document: &Html) -> Vec<Value> {
    let mut items = Vec::new();
    for script in document.select(&selector(r#"script[type="application/ld+json"]"#)) {
        let raw = script.text().collect::<String>();
        let Ok(value) = serde_json::from_str::<Value>(raw.trim()) else {
            continue;
        };
        match value {
            Value::Array(list) => items.extend(list),
            other => items.push(other),
        }
    }
    items
}

fn data_of_type<'a>(items: &'a [Value], kind: &str) -> Option<&'a Value> {
    items
        .iter()
        .find(|v| v.get("@type").and_then(Value::as_str) == Some(kind))
}

fn data_text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(decode_entities)
        .and_then(|t| clean_value(&t))
}

/// The page data sometimes keeps HTML codes inside its text
/// ("world&apos;s"); they are turned back into the characters.
fn decode_entities(text: &str) -> String {
    text.replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

fn first_text(document: &Html, css: &str) -> Option<String> {
    document
        .select(&selector(css))
        .map(|el| text_of(&el))
        .find(|t| !t.is_empty())
}

fn first_attr(document: &Html, css: &str, attr: &str) -> Option<String> {
    document
        .select(&selector(css))
        .find_map(|el| el.value().attr(attr).map(str::trim).map(str::to_string))
        .filter(|v| !v.is_empty())
}

/// Cuts `text` to at most `max` characters, at a character boundary.
fn cut(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}...", text[..end].trim_end()),
        None => text.to_string(),
    }
}

/// Everything this scraper reads from one Made-in-China page (product
/// page or supplier page).
#[derive(Debug, Default)]
struct PageFacts {
    is_product_page: bool,
    company_name: Option<String>,
    /// The supplier's shop, "https://{shop}.en.made-in-china.com".
    company_url: Option<String>,
    logo_url: Option<String>,
    /// "Liaoning, China": the region Made-in-China shows for the company.
    region: Option<String>,
    labels: Vec<(String, String)>,
    years_on_platform: Option<u32>,
    member_label: Option<String>,
    member_since: Option<i32>,
    licence_checked: bool,
    audited: bool,
    auditor: Option<String>,
    audit_report: Option<String>,
    rating: Option<f64>,
    review_count: Option<u64>,
    contact_name: Option<String>,
    profile_text: Option<String>,
    /// Products on the supplier's own page, with their price:
    /// "Spherical Roller Bearing (US$0.01-10.00)".
    shop_products: Vec<String>,
    /// The page offers Made-in-China's Secured Trading Service (escrow).
    secured_trading: bool,
}

impl PageFacts {
    fn label(&self, name: &str) -> Option<&str> {
        self.labels
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn read_page(html: &str) -> PageFacts {
    let document = Html::parse_document(html);
    let data = page_data(&document);
    let product = data_of_type(&data, "Product");
    let organization = data_of_type(&data, "Organization");
    let canonical = first_attr(&document, r#"link[rel="canonical"]"#, "href");
    let is_product_page = product.is_some()
        || canonical
            .as_deref()
            .is_some_and(|c| c.contains("/product/"));

    let lines = visible_lines(&document);
    let has_line = |text: &str| lines.iter().any(|l| l == text);

    // The company. The schema.org data first, then the visible name.
    let company_name = organization
        .and_then(|o| data_text(o, "name"))
        .or_else(|| first_text(&document, "h1.shop-sign__header-title--text"))
        .or_else(|| first_text(&document, "a.sr-comInfo-title"))
        .or_else(|| {
            // Supplier page title: "Bearing Manufacturer, ... - Changzhou Nanyi Bearing Co., Ltd."
            if is_product_page {
                return None;
            }
            let title = first_text(&document, "title")?;
            title.rsplit(" - ").next().and_then(clean_value)
        });
    let company_url = organization
        .and_then(|o| data_text(o, "url"))
        .or_else(|| first_attr(&document, "a.sr-comInfo-title", "href"))
        .or(canonical)
        .and_then(|u| shop_host(&u))
        .map(|host| format!("https://{host}"));
    // Older product pages have no company data: the name is then the
    // title of the link to the company's shop.
    let company_name = company_name.or_else(|| {
        let shop = company_url.as_deref().and_then(shop_host)?;
        document
            .select(&selector("a[title][href]"))
            .filter(|a| {
                let href = a.value().attr("href").unwrap_or("");
                shop_host(href).as_deref() == Some(shop.as_str()) && !href.contains("/product")
            })
            .find_map(|a| a.value().attr("title").and_then(clean_value))
    });
    let logo_url = organization
        .and_then(|o| data_text(o, "logo"))
        .map(|u| absolute_url(&u));

    let region = first_text(&document, ".detail-address")
        .or_else(|| lines.iter().find(|l| REGION.is_match(l)).cloned());

    // The member box: "2 yrs", "Diamond Member", "Since 2025", the
    // licence and the audit.
    let years_on_platform = lines.iter().find_map(|l| {
        YEARS_ON_PLATFORM
            .captures(l)
            .and_then(|c| c[1].parse::<u32>().ok())
    });
    let member_label = lines.iter().find(|l| MEMBER.is_match(l)).cloned();
    let member_since = lines.iter().find_map(|l| {
        MEMBER_SINCE
            .captures(l)
            .and_then(|c| c[1].parse::<i32>().ok())
    });
    // A supplier's own page shows "Since 2025" but not "2 yrs". On every
    // product page checked, "N yrs" counts the years from that year,
    // this year included (Since 2024 = 3 yrs in 2026).
    let years_on_platform = years_on_platform.or_else(|| {
        let this_year = Utc::now().year();
        member_since
            .filter(|y| *y <= this_year)
            .map(|y| (this_year - y + 1) as u32)
    });
    let licence_checked = has_line(LICENCE_LINE);
    let audit_note = lines
        .iter()
        .find_map(|l| l.strip_prefix("This supplier has been audited by "));
    let auditor = audit_note
        .map(|n| n.split(". Verify").next().unwrap_or(n))
        .map(|n| n.trim().trim_end_matches('.').to_string())
        .filter(|n| !n.is_empty());
    let audited = audit_note.is_some()
        || has_line("Audited Supplier")
        || lines
            .iter()
            .any(|l| l.starts_with("Audited by an independent third-party"));
    let audit_report = REPORT_ID
        .captures(&lines.join(" "))
        .map(|c| c[1].to_string());

    // Rating: "5.0" then "(2 Reviews)" on a product page, "Rating:" then
    // "4.8/5" on a supplier page, "Rating" then "4.7" on older pages.
    let mut rating = None;
    let mut review_count = None;
    for (i, line) in lines.iter().enumerate() {
        if let Some(c) = REVIEWS.captures(line) {
            review_count = c[1].parse::<u64>().ok();
            rating = i
                .checked_sub(1)
                .and_then(|p| RATING.captures(&lines[p]))
                .and_then(|c| c[1].parse::<f64>().ok());
            break;
        }
    }
    if rating.is_none() {
        rating = lines
            .iter()
            .position(|l| l == "Rating:" || l == "Rating")
            .and_then(|i| lines.get(i + 1))
            .and_then(|l| RATING.captures(l))
            .and_then(|c| c[1].parse::<f64>().ok());
    }
    let rating = rating.filter(|r| *r > 0.0);

    let contact_name = first_text(&document, ".sr-sendMsg-name");
    let profile_text = organization
        .and_then(|o| data_text(o, "description"))
        .or_else(|| first_text(&document, ".company-desc .desc"))
        .or_else(|| first_text(&document, ".sr-comProfile-intro"))
        .or_else(|| first_text(&document, ".intro-cnt"))
        .or_else(|| first_text(&document, ".video-info .text-cnt"))
        .map(|t| cut(&t, MAX_PROFILE_CHARS));

    let shop_products = if is_product_page {
        Vec::new()
    } else {
        shop_products(&document, company_url.as_deref())
    };

    PageFacts {
        is_product_page,
        company_name,
        company_url,
        logo_url,
        region,
        labels: labeled_values(&document, LABEL_PAIRS),
        years_on_platform,
        member_label,
        member_since,
        licence_checked,
        audited,
        auditor,
        audit_report,
        rating,
        review_count,
        contact_name,
        profile_text,
        shop_products,
        secured_trading: has_line("Secured Trading Service"),
    }
}

/// The products on a supplier's own page, each with its price when one
/// is shown: "Low Price ... Spherical Roller Bearing (US$0.01-10.00)".
/// Only the supplier's own products (links on its own shop) are read.
fn shop_products(document: &Html, company_url: Option<&str>) -> Vec<String> {
    let shop = company_url.and_then(shop_host);
    let own_product = |href: &str| {
        href.contains("/product/")
            && (href.starts_with('/') && !href.starts_with("//")
                || shop.as_deref().is_some_and(|s| href.contains(s)))
    };
    let mut products: Vec<String> = Vec::new();
    let mut titles: Vec<String> = Vec::new();
    for link in document.select(&selector("a[href]")) {
        let href = link.value().attr("href").unwrap_or("");
        if !own_product(href) {
            continue;
        }
        let title = text_of(&link);
        if title.is_empty() || titles.contains(&title) {
            continue;
        }
        titles.push(title.clone());
        // The price is in the product's own box: go up from the link,
        // but stop before a box that holds more than one product.
        let mut price = None;
        let mut node = link.parent().and_then(ElementRef::wrap);
        for _ in 0..3 {
            let Some(el) = node else { break };
            let links: Vec<&str> = el
                .select(&selector("a[href*='/product/']"))
                .filter_map(|a| a.value().attr("href"))
                .collect();
            let first = links.first().copied().unwrap_or("");
            if links.iter().any(|h| *h != first) {
                break;
            }
            if let Some(m) = PRICE.find(&text_of(&el)) {
                price = Some(m.as_str().replace(' ', ""));
                break;
            }
            node = el.parent().and_then(ElementRef::wrap);
        }
        products.push(match price {
            Some(p) => format!("{} ({})", cut(&title, 120), p),
            None => cut(&title, 120),
        });
        if products.len() >= MAX_SHOP_PRODUCTS {
            break;
        }
    }
    products
}

/// The founding year from "2026-04-15" or "2005".
fn year_in(text: &str) -> Option<String> {
    YEAR.captures(text).map(|c| c[1].to_string())
}

/// Lowercase letters and digits only, without legal endings, so
/// "Changzhou Nanyi Bearing Co., Ltd." and "CHANGZHOU NANYI BEARING"
/// compare equal.
fn name_key(name: &str) -> String {
    const ENDINGS: &[&str] = &[
        "co",
        "ltd",
        "company",
        "limited",
        "inc",
        "corporation",
        "corp",
        "llc",
    ];
    name.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .filter(|w| !ENDINGS.contains(&w.as_str()))
        .collect::<Vec<_>>()
        .join("")
}

/// Other companies named in the company's own profile text (a factory,
/// a parent company - or a profile copied from another company). The
/// company's own name is left out.
fn other_company_names(profile: &str, own_name: Option<&str>) -> Vec<String> {
    let own = own_name.map(name_key).unwrap_or_default();
    let mut found: Vec<String> = Vec::new();
    let mut keys: Vec<String> = Vec::new();
    for m in COMPANY_NAME.captures_iter(profile) {
        let name = m[1].trim().to_string();
        let key = name_key(&name);
        if key.is_empty()
            || key == own
            || (!own.is_empty() && (own.contains(&key) || key.contains(&own)))
            || keys.contains(&key)
        {
            continue;
        }
        keys.push(key);
        found.push(name);
        if found.len() >= MAX_OTHER_NAMES {
            break;
        }
    }
    found
}

/// "Diamond Member since 2025; 2 years on Made-in-China; rating 5.0 out
/// of 5 from 2 reviews; audited on site by TÜV Rheinland (report
/// MIC-ASI254819); business licence checked by Made-in-China."
fn platform_line(f: &PageFacts) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    match (&f.member_label, f.member_since) {
        (Some(m), Some(y)) => parts.push(format!("{m} (a paid membership) since {y}")),
        (Some(m), None) => parts.push(format!("{m} (a paid membership)")),
        _ => {}
    }
    if let Some(y) = f.years_on_platform {
        parts.push(format!(
            "{y} {} on Made-in-China",
            if y == 1 { "year" } else { "years" }
        ));
    }
    if let Some(r) = f.rating {
        parts.push(match f.review_count {
            Some(n) => format!(
                "rating {r:.1} out of 5 from {n} {}",
                if n == 1 { "review" } else { "reviews" }
            ),
            None => format!("rating {r:.1} out of 5"),
        });
    }
    if f.audited {
        let by = f
            .auditor
            .as_deref()
            .map(|a| format!(" by {a}"))
            .unwrap_or_default();
        let report = f
            .audit_report
            .as_deref()
            .map(|r| format!(" (report {r})"))
            .unwrap_or_default();
        parts.push(format!("audited on site{by}{report}"));
    }
    if f.licence_checked {
        parts.push("business licence checked by Made-in-China".to_string());
    }
    (!parts.is_empty()).then(|| format!("{PLATFORM_PREFIX}{}.", parts.join("; ")))
}

/// The description lines for the company, in order: address, region,
/// company facts, Made-in-China's own record, other company names, the
/// supplier's products and its own profile text.
fn company_lines(f: &PageFacts) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(address) = f.label("Address") {
        lines.push(format!(
            "{ADDRESS_PREFIX}{}.",
            address.trim_end_matches('.')
        ));
    }
    if let Some(region) = &f.region {
        lines.push(format!("{REGION_PREFIX}{region}."));
    }
    for (label, name) in COMPANY_FACTS {
        if let Some(value) = f.label(label) {
            lines.push(format!("{name}: {}.", value.trim_end_matches('.')));
        }
    }
    lines.extend(platform_line(f));
    if let Some(profile) = &f.profile_text {
        let others = other_company_names(profile, f.company_name.as_deref());
        if !others.is_empty() {
            let mut names = others.join("; ");
            if !names.ends_with('.') {
                names.push('.');
            }
            lines.push(format!(
                "{OTHER_NAMES_PREFIX}{names} Ask how they are related."
            ));
        }
    }
    if !f.shop_products.is_empty() {
        lines.push(format!(
            "{SHOP_PRODUCTS_PREFIX}{}.",
            f.shop_products.join("; ")
        ));
    }
    lines.extend(f.profile_text.clone());
    lines
}

/// What the badge text says Made-in-China checked (read by
/// signals.rs): "Audited on site by SGS (report QIP-ASI269732)",
/// "Business licence checked", or None.
fn badge_text(f: &PageFacts) -> Option<String> {
    if f.audited {
        let by = f
            .auditor
            .as_deref()
            .map(|a| format!(" by {a}"))
            .unwrap_or_default();
        let report = f
            .audit_report
            .as_deref()
            .map(|r| format!(" (report {r})"))
            .unwrap_or_default();
        Some(format!("{AUDITED_PREFIX} on site{by}{report}"))
    } else if f.licence_checked {
        Some(LICENCE_CHECKED.to_string())
    } else {
        None
    }
}

fn supplier_from(f: &PageFacts, profile_url: &str) -> B2bSupplierProfile {
    let lines = company_lines(f);
    B2bSupplierProfile {
        company_name: f.company_name.clone(),
        logo_url: f.logo_url.clone(),
        year_established: f.label("Year of Establishment").and_then(year_in),
        // Every supplier on Made-in-China is in China; the region is
        // more exact when the page shows it.
        country: f.region.clone().or_else(|| Some("China".to_string())),
        platform_verified_badge: f.audited,
        employee_count: f.label("Number of Employees").map(str::to_string),
        sales_revenue: None,
        export_percentage: None,
        profile_url: profile_url.to_string(),
        source_platform: PLATFORM.to_string(),
        contact_name: f.contact_name.clone(),
        contact_phone: None,
        badge_honorific: badge_text(f),
        company_description: (!lines.is_empty()).then(|| lines.join("\n")),
        website_url: None,
    }
}

fn record_from(f: &PageFacts) -> Option<SupplierRecord> {
    let mut products: Vec<String> = f
        .label("Main Products")
        .map(|p| {
            p.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    products.extend(f.shop_products.iter().cloned());
    let record = SupplierRecord {
        platform: PLATFORM_NAME.to_string(),
        checked_by_platform: f.audited,
        member_label: f.member_label.clone(),
        years_on_platform: f.years_on_platform,
        rating: f.rating,
        review_count: f.review_count,
        products_offered: products,
        business_types: f
            .label("Business Type")
            .map(|t| vec![t.to_string()])
            .unwrap_or_default(),
        ..Default::default()
    };
    let has_anything = record.checked_by_platform
        || record.member_label.is_some()
        || record.years_on_platform.is_some()
        || record.rating.is_some()
        || !record.products_offered.is_empty();
    has_anything.then_some(record)
}

/// The supplier's FAQ, as (question, answer): from the page's schema.org
/// "FAQPage" data. Older pages have none (their FAQ is inside the
/// product text, see own_text_lines).
fn faq_pairs(data: &[Value]) -> Vec<(String, String)> {
    let Some(faq) = data_of_type(data, "FAQPage") else {
        return Vec::new();
    };
    faq.get("mainEntity")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|q| {
                    let question = data_text(q, "name")?;
                    let answer = q.get("acceptedAnswer").and_then(|a| data_text(a, "text"))?;
                    Some((question, answer))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The supplier's own product text (the "Product Description" box),
/// one piece per line. On older pages the FAQ is written in here
/// ("1: What kind terms of payment can be accepted?" "A: ...").
fn own_text_lines(document: &Html) -> Vec<String> {
    let Some(text_box) = document.select(&selector(".rich-text")).next() else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for node in text_box.descendants() {
        let Node::Text(text) = node.value() else {
            continue;
        };
        let hidden = node.ancestors().any(|a| {
            matches!(a.value(), Node::Element(e)
                if matches!(e.name(), "script" | "style" | "noscript" | "template"))
        });
        let line = squash(text);
        if !hidden && !line.is_empty() {
            lines.push(line);
        }
    }
    lines
}

/// What the supplier itself wrote about payment in its FAQ or product
/// text, e.g. "A: For terms of payment, L/C, T/T, D/A, D/P, Western
/// Union (can be) could accepted." Questions are skipped, answers kept.
fn payment_notes(faq: &[(String, String)], own_text: &[String]) -> Vec<String> {
    let mut notes: Vec<String> = Vec::new();
    let mut add = |note: &str| {
        let note = note.trim();
        if !note.is_empty() && note.chars().count() <= 300 && !notes.iter().any(|n| n == note) {
            notes.push(note.to_string());
        }
    };
    for (question, answer) in faq {
        if question.to_lowercase().contains("payment") {
            add(answer);
        }
    }
    for line in own_text {
        let lower = line.to_lowercase();
        if lower.contains("payment") && !line.trim_end().ends_with('?') {
            add(line);
        }
    }
    notes.truncate(MAX_PAYMENT_NOTES);
    notes
}

/// The price box. Tiered prices become "US$901.00 (1-9 Pieces); US$896.00
/// (10-49 Pieces)" with the first tier as the minimum order; a single
/// price or range comes with its "100 Pieces(MOQ)" line.
fn price_and_moq(document: &Html) -> (Option<String>, Option<String>) {
    let mut tiers: Vec<(String, String)> = Vec::new();
    for tier in document.select(&selector("li.swiper-slide-div")) {
        let money = tier
            .select(&selector(".swiper-money-container"))
            .next()
            .map(|e| text_of(&e))
            .unwrap_or_default();
        let quantity = tier
            .select(&selector(".swiper-unit-container"))
            .next()
            .map(|e| text_of(&e))
            .unwrap_or_default();
        if money.is_empty() || tiers.iter().any(|(m, q)| *m == money && *q == quantity) {
            continue;
        }
        tiers.push((money, quantity));
    }
    if !tiers.is_empty() {
        let price = tiers
            .iter()
            .map(|(m, q)| {
                if q.is_empty() {
                    m.clone()
                } else {
                    format!("{m} ({q})")
                }
            })
            .collect::<Vec<_>>()
            .join("; ");
        let moq = FIRST_QUANTITY
            .captures(&tiers[0].1)
            .map(|c| match c.get(2) {
                Some(unit) => format!("{} {}", &c[1], unit.as_str()),
                None => c[1].to_string(),
            });
        return (Some(price), moq);
    }

    let Some(price_el) = document
        .select(&selector(".only-one-priceNum-td-left"))
        .next()
    else {
        return (None, None);
    };
    let price = clean_value(&text_of(&price_el));
    // The MOQ is next to the price: "100 Pieces(MOQ)", or in the next
    // cell on older pages: "1 Unit" "(MOQ)".
    let mut moq = None;
    let mut node = price_el.parent().and_then(ElementRef::wrap);
    for _ in 0..3 {
        let Some(el) = node else { break };
        let text = text_of(&el);
        if text.contains("MOQ") {
            let rest = match &price {
                Some(p) => text.replacen(p.as_str(), "", 1),
                None => text,
            };
            moq = clean_value(&rest.replace("(MOQ)", ""));
            break;
        }
        node = el.parent().and_then(ElementRef::wrap);
    }
    (price, moq)
}

impl B2bScraper for MadeinchinaScraper {
    fn matches_platform(&self, platform: &str) -> bool {
        platform == PLATFORM
    }

    fn parse_supplier(&self, html: &str, profile_url: &str) -> B2bSupplierProfile {
        supplier_from(&read_page(html), profile_url)
    }

    fn parse_listing(&self, html: &str, listing_url: &str) -> B2bListingProfile {
        let facts = read_page(html);
        let document = Html::parse_document(html);
        let payment_type = facts.label("Terms of Payment").map(|terms| {
            if facts.secured_trading {
                format!("{terms}; Made-in-China Secured Trading Service (escrow)")
            } else {
                terms.to_string()
            }
        });
        let mut listing = B2bListingProfile {
            payment_type,
            incoterms: facts
                .label("International Commercial Terms(Incoterms)")
                .map(str::to_string),
            delivery_timeframe: facts.label("Average Lead Time").map(str::to_string),
            preferred_port: facts.label("Nearest Port").map(str::to_string),
            listing_url: listing_url.to_string(),
            source_platform: PLATFORM.to_string(),
            ..Default::default()
        };
        // A supplier's own page lists many products: no single product
        // is read from it (no title), only the company's order terms.
        if !facts.is_product_page {
            return listing;
        }

        let data = page_data(&document);
        let product = data_of_type(&data, "Product");
        listing.title = product
            .and_then(|p| data_text(p, "name"))
            .or_else(|| first_text(&document, "h1"));

        let details = labeled_values(&document, PRODUCT_DETAIL_PAIRS);
        let detail = |name: &str| {
            details
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.clone())
        };
        let detail_line = {
            let shown: Vec<String> = details
                .iter()
                .filter(|(_, v)| v.chars().count() <= 100)
                .take(MAX_PRODUCT_DETAILS)
                .map(|(n, v)| format!("{n}: {v}"))
                .collect();
            (!shown.is_empty()).then(|| format!("{PRODUCT_DETAILS_PREFIX}{}.", shown.join("; ")))
        };
        let summary = product.and_then(|p| data_text(p, "description"));
        let faq = faq_pairs(&data);
        let faq_line = (!faq.is_empty()).then(|| {
            let joined = faq
                .iter()
                .map(|(q, a)| format!("{q} {a}"))
                .collect::<Vec<_>>()
                .join(" | ");
            format!("{FAQ_PREFIX}{}", cut(&joined, MAX_FAQ_CHARS))
        });
        let own_text = own_text_lines(&document);
        let own_text_line = (!own_text.is_empty()).then(|| {
            format!(
                "{OWN_TEXT_PREFIX}{}",
                cut(&own_text.join(" "), MAX_OWN_TEXT_CHARS)
            )
        });
        let parts: Vec<String> = [summary, detail_line, faq_line, own_text_line]
            .into_iter()
            .flatten()
            .collect();
        listing.description = (!parts.is_empty()).then(|| parts.join("\n"));

        // Payment terms the supplier wrote only in its FAQ or product
        // text (older pages have no "Terms of Payment" box) are added, so
        // Western Union or MoneyGram written there is still caught.
        let notes = payment_notes(&faq, &own_text);
        if !notes.is_empty() {
            let words = format!("{PAYMENT_WORDS_PREFIX}{}", notes.join(" "));
            listing.payment_type = Some(match listing.payment_type.take() {
                Some(terms) => format!("{terms}; {words}"),
                None if facts.secured_trading => {
                    format!("{words}; Made-in-China Secured Trading Service (escrow)")
                }
                None => words,
            });
        }

        listing.image_urls = match product.and_then(|p| p.get("image")) {
            Some(Value::Array(list)) => list
                .iter()
                .filter_map(Value::as_str)
                .map(absolute_url)
                .collect(),
            Some(Value::String(one)) => vec![absolute_url(one)],
            _ => Vec::new(),
        };

        let (price, moq) = price_and_moq(&document);
        listing.unit_price = price.or_else(|| {
            product
                .and_then(|p| p.get("offers"))
                .and_then(|o| o.get("price"))
                .and_then(Value::as_str)
                .map(|p| format!("US${p}"))
        });
        listing.minimum_order_quantity =
            moq.or_else(|| facts.label("Minimum Order Quantity").map(str::to_string));
        listing.production_capacity = detail("Production Capacity");
        listing.packaging_details = detail("Transport Package");
        listing.reference = detail("Model NO.");
        listing
    }

    /// The supplier's shop on a product page; None on the supplier's
    /// own page (it already is the company page).
    fn extract_company_profile_url(&self, listing_html: &str) -> Option<String> {
        let facts = read_page(listing_html);
        if facts.is_product_page {
            facts.company_url
        } else {
            None
        }
    }

    /// The supplier's shop, e.g. "yongjie-bearing.en.made-in-china.com":
    /// the same on its own page and on every product it sells, so its
    /// Safely history and reports belong to the company.
    fn company_key(&self, listing_html: &str) -> Option<String> {
        read_page(listing_html)
            .company_url
            .as_deref()
            .and_then(shop_host)
    }

    fn supplier_record(&self, listing_html: &str) -> Option<SupplierRecord> {
        record_from(&read_page(listing_html))
    }

    /// Adds what only the supplier's own page shows: the region, the
    /// contact person, the founding year and staff on some pages, and
    /// the supplier's other products. What the product page already gave
    /// is kept.
    fn enrich_from_company_profile(
        &self,
        mut supplier: B2bSupplierProfile,
        profile_html: &str,
    ) -> B2bSupplierProfile {
        let f = read_page(profile_html);
        if supplier.company_name.is_none() {
            supplier.company_name = f.company_name.clone();
        }
        if supplier.year_established.is_none() {
            supplier.year_established = f.label("Year of Establishment").and_then(year_in);
        }
        if supplier.employee_count.is_none() {
            supplier.employee_count = f.label("Number of Employees").map(str::to_string);
        }
        if let Some(region) = &f.region {
            supplier.country = Some(region.clone());
        }
        if supplier.contact_name.is_none() {
            supplier.contact_name = f.contact_name.clone();
        }
        if supplier.logo_url.is_none() {
            supplier.logo_url = f.logo_url.clone();
        }
        supplier.platform_verified_badge |= f.audited;
        let page_badge = badge_text(&f);
        let better_badge = match (&supplier.badge_honorific, &page_badge) {
            (None, Some(_)) => true,
            (Some(old), Some(new)) => {
                !old.starts_with(AUDITED_PREFIX) && new.starts_with(AUDITED_PREFIX)
            }
            _ => false,
        };
        if better_badge {
            supplier.badge_honorific = page_badge;
        }

        // Only lines the product page did not already give (by their
        // start, e.g. "Address:"). The profile text is added only when
        // there was none.
        let mut description = supplier.company_description.take().unwrap_or_default();
        let has_profile = description.lines().any(|l| l.chars().count() > 200);
        for line in company_lines(&f) {
            let key = line.split(':').next().unwrap_or("");
            let is_profile = f.profile_text.as_deref() == Some(line.as_str());
            let already = if is_profile {
                has_profile
            } else {
                description
                    .lines()
                    .any(|l| l.split(':').next() == Some(key))
            };
            if !already {
                if !description.is_empty() {
                    description.push('\n');
                }
                description.push_str(&line);
            }
        }
        supplier.company_description = (!description.is_empty()).then_some(description);
        supplier
    }

    fn enrich_record_from_company_profile(
        &self,
        record: Option<SupplierRecord>,
        profile_html: &str,
    ) -> Option<SupplierRecord> {
        let page = record_from(&read_page(profile_html));
        match (record, page) {
            (None, page) => page,
            (Some(r), None) => Some(r),
            (Some(mut r), Some(p)) => {
                r.checked_by_platform |= p.checked_by_platform;
                r.member_label = r.member_label.or(p.member_label);
                r.years_on_platform = r.years_on_platform.or(p.years_on_platform);
                r.rating = r.rating.or(p.rating);
                r.review_count = r.review_count.or(p.review_count);
                if r.business_types.is_empty() {
                    r.business_types = p.business_types;
                }
                for product in p.products_offered {
                    if !r.products_offered.contains(&product) {
                        r.products_offered.push(product);
                    }
                }
                Some(r)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shop_host_is_only_a_supplier_shop() {
        assert_eq!(
            shop_host("https://yongjie-bearing.en.made-in-china.com/product/x.html").as_deref(),
            Some("yongjie-bearing.en.made-in-china.com")
        );
        assert_eq!(
            shop_host("//Sysmartlink.en.made-in-china.com").as_deref(),
            Some("sysmartlink.en.made-in-china.com")
        );
        assert_eq!(
            shop_host("https://www.made-in-china.com/product/x.html"),
            None
        );
        assert_eq!(shop_host("https://example.com/"), None);
    }

    #[test]
    fn other_company_names_leave_out_the_companys_own_name() {
        let profile = "Wafangdian Jinrui Bearing Manufacturing Co., Ltd. is located in Dalian. \
            According to the plan, Changzhou Nanyi Bearing Co., Ltd. Will be established in 2021. \
            Wafangdian Jinrui Bearing Co., Ltd. is based in Dalian.";
        assert_eq!(
            other_company_names(profile, Some("Changzhou Nanyi Bearing Co., Ltd.")),
            vec![
                "Wafangdian Jinrui Bearing Manufacturing Co., Ltd.",
                "Wafangdian Jinrui Bearing Co., Ltd."
            ]
        );
        assert!(
            other_company_names(
                "Welcome to Shenzhen Sy Smartlink Technology Co., Ltd. today.",
                Some("Shenzhen Sy Smartlink Technology Co., Ltd.")
            )
            .is_empty()
        );
    }

    #[test]
    fn founding_year_comes_from_a_full_date() {
        assert_eq!(year_in("2026-04-15").as_deref(), Some("2026"));
        assert_eq!(year_in("2005").as_deref(), Some("2005"));
        assert_eq!(year_in("N/A"), None);
    }

    #[test]
    fn long_profiles_are_cut() {
        assert_eq!(cut("abcdef", 3), "abc...");
        assert_eq!(cut("abc", 3), "abc");
        assert_eq!(cut("TÜV", 2), "TÜ...");
    }
}
