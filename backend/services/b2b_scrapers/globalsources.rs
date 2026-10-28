//! Global Sources (globalsources.com): Asian suppliers, many US buyers.
//!
//! Two sites, both read here (the trade-show one in
//! globalsources_show.rs):
//! - the marketplace (below);
//! - the Hong Kong trade-show site: product pages
//!   (/products/HK/{name}_{id}/) are scanned and the supplier's
//!   exhibitor page (/exhibitors/HK/{company}_{id}/) is read with them.
//!
//! Marketplace: only product pages are scanned:
//! https://www.globalsources.com/{category}/{name}-{id}p.htm
//! The supplier's own page (https://{shop}.manufacturer.globalsources.com/
//! homepage_{id}.htm) is read as well, for its founding year, ranking
//! and profile, but is never scanned on its own. The trade-show pages
//! (/products/HK/..., /exhibitors/HK/...) are read by globalsources_show.rs.
//!
//! Everything is in the page's plain HTML:
//! - the product: the schema.org "Product" data (name, price range),
//!   the price tiers, the product details tables (model, FOB port, lead
//!   time, payment method, certificates) and the supplier's own text;
//! - the supplier card on the product page: years with Global Sources,
//!   the "Verified Supplier" / "Verified Manufacturer" badge, country,
//!   business type, trade shows attended;
//! - the supplier page: the schema.org "Organization" data (founding
//!   year, description) and its facts box (ranking, assessment, export
//!   markets).
//!
//! What the badges mean (globalsources.com/STM/site/quality/):
//! - "Verified Supplier": the business registration was checked by an
//!   outside company (D&B, Experian, TÜV SÜD...);
//! - "Verified Manufacturer": the same, and the registration allows
//!   manufacturing;
//! - "O2O Supported": exhibits at Global Sources trade shows;
//! - the ranking ("Super Supplier", "Premier Supplier"...): the
//!   supplier's level with Global Sources, not a check on the company.

// The trade-show reader, in globalsources_show.rs next to this file.
#[path = "globalsources_show.rs"]
mod show;

use super::{B2bListingProfile, B2bScraper, B2bSupplierProfile, SupplierRecord};
use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};
use serde_json::Value;
use std::sync::LazyLock;

pub struct GlobalsourcesScraper;

const PLATFORM: &str = "globalsources";
const PLATFORM_NAME: &str = "Global Sources";

/// Every supplier site is "{shop}.{type}.globalsources.com", e.g.
/// "yongfang.manufacturer.globalsources.com".
const SITE_END: &str = ".globalsources.com";

/// The start of the badge text when Global Sources had the business
/// registration checked (read by signals.rs).
pub const VERIFIED_PREFIX: &str = "Verified";

/// The badge text for a supplier only met at Global Sources trade shows.
pub const TRADE_SHOW_ONLY: &str = "Trade show exhibitor";

const MAX_PROFILE_CHARS: usize = 3000;
const MAX_OWN_TEXT_CHARS: usize = 2500;
const MAX_PRODUCT_DETAILS: usize = 15;
const MAX_IMAGES: usize = 6;
const MAX_OTHER_NAMES: usize = 3;
const MAX_PAYMENT_NOTES: usize = 3;

/// Label / value pairs on Global Sources pages, as (label element,
/// class of the value element right after it):
/// - product page: the details tables (model, shipping, payment,
///   certificates);
/// - product page: the key attributes ("Feature", "Type");
/// - supplier page: the facts box ("Business Type:", "Export Markets:").
const LABEL_PAIRS: &[(&str, &str)] = &[
    (
        "th.ant-descriptions-item-label",
        "ant-descriptions-item-content",
    ),
    (
        ".productAttributes-item-left",
        "productAttributes-item-right",
    ),
    (".item-d-title", "item-content"),
];

/// Only the key attributes ("Feature", "Bluetooth Version").
const ATTRIBUTE_PAIRS: &[(&str, &str)] = &[(
    ".productAttributes-item-left",
    "productAttributes-item-right",
)];

/// Company facts put in the description, as (label on the page, name
/// used in the description), in this order.
const COMPANY_FACTS: &[(&str, &str)] = &[
    ("Country / Region", "Country"),
    ("Business Type", "Business type"),
    ("Export Markets", "Export markets"),
    ("Response time", "Response time"),
    ("Response Rate", "Response rate"),
];

const PLATFORM_PREFIX: &str = "On Global Sources: ";
const OTHER_NAMES_PREFIX: &str = "The company profile also names other companies: ";
const PRODUCT_DETAILS_PREFIX: &str = "Product details: ";
const OWN_TEXT_PREFIX: &str = "Supplier's own product text: ";
const CERTIFICATE_PREFIX: &str = "Certificate shown: ";
const PAYMENT_WORDS_PREFIX: &str = "Payment, in the supplier's own words: ";

static YEARS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d{1,2})").unwrap());
static TRADE_SHOWS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"We exhibited at (\d+) Global Sources Trade Shows").unwrap());
static YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(19\d\d|20\d\d)\b").unwrap());
static FIRST_QUANTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[^\d]*(\d[\d,]*)[^A-Za-z]*([A-Za-z]+)?").unwrap());
static COMPANY_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b((?:[A-Z][A-Za-z&'\-]*\.? ){1,8}(?:Co\.?,? ?Ltd\.?|Company Limited|Co\. Limited|Limited|Inc\.|Corporation|Corp\.|LLC))",
    )
    .unwrap()
});

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("a valid CSS selector")
}

fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn text_of(el: &ElementRef) -> String {
    squash(&el.text().collect::<Vec<_>>().join(" "))
}

/// A value as shown, or None when it is empty. "Asia , Australasia"
/// becomes "Asia, Australasia".
fn clean_value(text: &str) -> Option<String> {
    let text = squash(text).replace(" ,", ",");
    let text = text.trim().trim_start_matches(':').trim();
    (!text.is_empty() && text != "-").then(|| text.to_string())
}

/// The page data keeps some HTML codes in its text ("Audio &amp;").
fn decode_entities(text: &str) -> String {
    text.replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

/// "//p.globalsources.com/x.jpg" -> "https://p.globalsources.com/x.jpg".
fn absolute_url(url: &str) -> String {
    let url = url.trim();
    if url.starts_with("//") {
        format!("https:{url}")
    } else {
        url.to_string()
    }
}

/// The supplier's site from a link to its page:
/// "//yongfang.manufacturer.globalsources.com/homepage_6008825952206.htm"
/// -> "yongfang.manufacturer.globalsources.com". The main site (www) is
/// not a supplier.
fn supplier_host(url: &str) -> Option<String> {
    let url = absolute_url(url);
    let after = url.split("://").nth(1)?;
    let host = after.split(['/', '?', '#']).next()?.to_lowercase();
    let is_supplier =
        host.ends_with(SITE_END) && !host.starts_with("www.") && host.split('.').count() >= 4;
    is_supplier.then_some(host)
}

/// The text the page shows, one piece per line, without scripts and
/// styles.
fn visible_lines(root: ElementRef) -> Vec<String> {
    let mut lines = Vec::new();
    for node in root.descendants() {
        let Node::Text(text) = node.value() else {
            continue;
        };
        let hidden = node.ancestors().any(|a| {
            matches!(a.value(), Node::Element(e)
                if matches!(e.name(), "script" | "style" | "noscript" | "template" | "head"))
        });
        let line = squash(text);
        if !hidden && !line.is_empty() {
            lines.push(line);
        }
    }
    lines
}

/// Every label / value pair from `pairs`, in page order. When a label
/// shows up twice, the first value is kept. A value that is only a
/// picture (a badge) gives the picture's alt text.
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
            let text = clean_value(&text_of(&value)).or_else(|| {
                let alts: Vec<String> = value
                    .select(&selector("img[alt]"))
                    .filter_map(|i| i.value().attr("alt").and_then(clean_value))
                    .collect();
                (!alts.is_empty()).then(|| alts.join(", "))
            });
            if let Some(v) = text {
                found.push((name, v));
            }
        }
    }
    found
}

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
    match value.get(key)? {
        Value::String(s) => clean_value(&decode_entities(s)),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn first_text(root: ElementRef, css: &str) -> Option<String> {
    root.select(&selector(css))
        .map(|el| text_of(&el))
        .find(|t| !t.is_empty())
}

fn cut(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}...", text[..end].trim_end()),
        None => text.to_string(),
    }
}

fn year_in(text: &str) -> Option<String> {
    YEAR.captures(text).map(|c| c[1].to_string())
}

/// Everything this scraper reads from one Global Sources page (product
/// page or supplier page).
#[derive(Debug, Default)]
struct PageFacts {
    is_product_page: bool,
    /// The supplier's contact and registration details from the page data.
    contact: SupplierContact,
    company_name: Option<String>,
    /// The supplier's page: "https://{shop}.manufacturer.globalsources.com/homepage_{id}.htm".
    company_url: Option<String>,
    logo_url: Option<String>,
    founding_year: Option<String>,
    labels: Vec<(String, String)>,
    years_on_platform: Option<u32>,
    /// Every badge picture's text: "Verified Maufacturer" (sic),
    /// "Verified Supplier", "O2O Supported", "Super Supplier"...
    badges: Vec<String>,
    country: Option<String>,
    business_type: Option<String>,
    trade_shows: Option<u32>,
    /// The supplier's own headline claims: "5,000sqm factory | 50
    /// skilled workers | OEM & ODM services".
    highlights: Option<String>,
    profile_text: Option<String>,
    slogan: Option<String>,
}

impl PageFacts {
    fn label(&self, name: &str) -> Option<&str> {
        self.labels
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// "Verified Supplier" or "Verified Manufacturer" (the page spells
    /// it "Maufacturer").
    fn verified(&self) -> Option<&'static str> {
        let has = |word: &str| {
            self.badges.iter().any(|b| {
                b.to_lowercase().starts_with("verified") && b.to_lowercase().contains(word)
            })
        };
        if has("facturer") {
            Some("Verified Manufacturer")
        } else if has("supplier") {
            Some("Verified Supplier")
        } else {
            None
        }
    }

    fn trade_show_exhibitor(&self) -> bool {
        self.badges
            .iter()
            .any(|b| b.eq_ignore_ascii_case("O2O Supported"))
            || self.trade_shows.is_some_and(|n| n > 0)
    }

    /// The supplier's level with Global Sources ("Super Supplier").
    fn ranking(&self) -> Option<String> {
        self.label("Ranking").map(str::to_string).or_else(|| {
            self.badges
                .iter()
                .find(|b| {
                    let b = b.to_lowercase();
                    b.ends_with(" supplier") && !b.starts_with("verified")
                })
                .cloned()
        })
    }
}

/// The supplier's details that Global Sources sends in its page data
/// ("contactUsInfo") but does not show as text on the product page.
/// The supplier gives them to Global Sources: its own website, its
/// address, and its official Chinese registration (name, number,
/// address).
#[derive(Debug, Default, Clone, PartialEq)]
struct SupplierContact {
    address: Option<String>,
    phone: Option<String>,
    /// The supplier's own website - never a Global Sources page.
    website: Option<String>,
    register_name: Option<String>,
    register_number: Option<String>,
    register_address: Option<String>,
}

static CONTACT_DATA: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"contactUsInfo:\{([^{}]*)\}").unwrap());
static DATA_STRING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(\w+):"((?:[^"\\]|\\.)*)""#).unwrap());

/// "https:\u002F\u002Fboxinheadset.com\u002F" -> "https://boxinheadset.com/".
fn decode_js_text(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('u') => {
                let code: String = chars.by_ref().take(4).collect();
                if let Some(ch) = u32::from_str_radix(&code, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            Some('n') | Some('t') | Some('r') => out.push(' '),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

fn supplier_contact(html: &str) -> SupplierContact {
    let Some(block) = CONTACT_DATA.captures(html) else {
        return SupplierContact::default();
    };
    let field = |name: &str| {
        DATA_STRING
            .captures_iter(&block[1])
            .find(|c| &c[1] == name)
            .map(|c| squash(&decode_js_text(&c[2])))
            .filter(|v| !v.is_empty())
    };
    let website = field("othHomePageAddress").filter(|w| {
        let lower = w.to_lowercase();
        (lower.starts_with("http") || lower.contains('.')) && !lower.contains("globalsources.com")
    });
    SupplierContact {
        address: field("address"),
        phone: field("telephone").or_else(|| field("mobile")),
        website,
        register_name: field("registerCompanyName"),
        register_number: field("registerNum"),
        register_address: field("registerAddress"),
    }
}

/// Lines for the company description: the address and the official
/// registration, as the supplier gave them to Global Sources.
fn contact_lines(c: &SupplierContact) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(a) = &c.address {
        lines.push(format!(
            "Address given to Global Sources: {}.",
            a.trim_end_matches('.')
        ));
    }
    let mut registration: Vec<String> = Vec::new();
    if let Some(n) = &c.register_name {
        registration.push(format!("name {n}"));
    }
    if let Some(n) = &c.register_number {
        if n.to_uppercase().contains("XXX") {
            registration.push(format!("number {n} (partly hidden by Global Sources)"));
        } else {
            registration.push(format!("number {n}"));
        }
    }
    if let Some(a) = &c.register_address {
        registration.push(format!("address {a}"));
    }
    if !registration.is_empty() {
        lines.push(format!(
            "Official registration in China (shown by Global Sources): {}.",
            registration.join("; ")
        ));
    }
    lines
}

fn read_page(html: &str) -> PageFacts {
    let document = Html::parse_document(html);
    let root = document.root_element();
    let data = page_data(&document);
    let product = data_of_type(&data, "Product");
    let organization = data_of_type(&data, "Organization");
    let is_product_page = product.is_some();
    let lines = visible_lines(root);

    // The supplier card on a product page, or the whole supplier page.
    let card = document
        .select(&selector(".company-info"))
        .next()
        .unwrap_or(root);

    let company_name = organization
        .and_then(|o| data_text(o, "name"))
        .or_else(|| first_text(card, ".supplier-name .name"))
        .or_else(|| product.and_then(|p| data_text(p, "manufacturer")));

    // The supplier's page: from the page data on the supplier page, or
    // the link to it on the product page.
    let company_url = organization
        .and_then(|o| data_text(o, "url"))
        .or_else(|| {
            document
                .select(&selector("a[href*='/homepage_']"))
                .filter_map(|a| a.value().attr("href"))
                .find(|h| supplier_host(h).is_some())
                .map(str::to_string)
        })
        .map(|u| absolute_url(&u))
        .filter(|u| supplier_host(u).is_some());

    let logo_url = organization.and_then(|o| {
        o.get("logo")
            .and_then(|l| data_text(l, "contentUrl").or_else(|| l.as_str().map(str::to_string)))
            .map(|u| absolute_url(&u))
    });

    let labels = labeled_values(&document, LABEL_PAIRS);
    let label = |name: &str| {
        labels
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    };

    let years_on_platform = first_text(card, ".year-bg .num")
        .or_else(|| label("Years with Global Sources"))
        .and_then(|y| YEARS.captures(&y).and_then(|c| c[1].parse::<u32>().ok()));

    let mut badges: Vec<String> = Vec::new();
    for img in card.select(&selector("img[alt]")) {
        if let Some(alt) = img.value().attr("alt").and_then(clean_value)
            && !badges.contains(&alt)
            && company_name.as_deref() != Some(alt.as_str())
        {
            badges.push(alt);
        }
    }

    let country = first_text(card, "em.gs-tag.dot").or_else(|| label("Country / Region"));
    let business_type = document
        .select(&selector(".Business-Type .tit"))
        .next()
        .and_then(|t| t.next_siblings().find_map(ElementRef::wrap))
        .map(|v| text_of(&v))
        .and_then(|t| clean_value(&t))
        .or_else(|| label("Business Type"));
    let trade_shows = TRADE_SHOWS
        .captures(&lines.join(" "))
        .and_then(|c| c[1].parse::<u32>().ok());

    // The headline box ends with the response time, which is already
    // read from the facts box.
    let highlights = first_text(root, ".overview-list")
        .map(|t| {
            t.split(" Avg Response")
                .next()
                .unwrap_or(&t)
                .trim()
                .to_string()
        })
        .filter(|t| !t.is_empty())
        .map(|t| cut(&t, 400));
    let profile_text = first_text(root, ".txt-content")
        .or_else(|| organization.and_then(|o| data_text(o, "description")))
        .map(|t| cut(&t, MAX_PROFILE_CHARS));

    PageFacts {
        is_product_page,
        company_name,
        company_url,
        logo_url,
        founding_year: organization
            .and_then(|o| data_text(o, "foundingDate"))
            .and_then(|d| year_in(&d)),
        labels,
        years_on_platform,
        badges,
        country,
        business_type,
        trade_shows,
        highlights,
        profile_text,
        slogan: organization.and_then(|o| data_text(o, "slogan")),
        contact: supplier_contact(html),
    }
}

/// The words of a company name, lower case, without legal endings and
/// without a plural "s" ("Electronics" = "Electronic").
fn name_words(name: &str) -> Vec<String> {
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
        .map(|w| w.to_lowercase())
        .filter(|w| !ENDINGS.contains(&w.as_str()))
        .map(|w| w.strip_suffix('s').map(str::to_string).unwrap_or(w))
        .collect()
}

/// True when `found` is the company's own name, even written a little
/// differently ("Electronics Technology Co., Ltd." for "Electronic
/// Technology Co. Ltd") or run together with a heading before it
/// ("... Experience Dongguan Yongfang Electronics ...").
fn is_own_name(found: &[String], own: &[String]) -> bool {
    if own.is_empty() {
        return false;
    }
    // "Yueyinda Electronic" is "Guangdong Yueyinda Electronic" without
    // the province.
    if found.iter().all(|w| own.contains(w)) {
        return true;
    }
    let shared = own.iter().filter(|w| found.contains(w)).count();
    shared * 4 >= own.len() * 3
}

/// Other companies named in the company's own profile text (a factory,
/// a parent company, or a profile copied from another company). The
/// company's own name is left out.
fn other_company_names(profile: &str, own_name: Option<&str>) -> Vec<String> {
    let own = own_name.map(name_words).unwrap_or_default();
    let mut found: Vec<String> = Vec::new();
    let mut seen: Vec<Vec<String>> = Vec::new();
    for m in COMPANY_NAME.captures_iter(profile) {
        let name = m[1].trim().to_string();
        let words = name_words(&name);
        if words.is_empty() || is_own_name(&words, &own) || seen.contains(&words) {
            continue;
        }
        seen.push(words);
        found.push(name);
        if found.len() >= MAX_OTHER_NAMES {
            break;
        }
    }
    found
}

/// "Verified Manufacturer (business registration checked by an outside
/// company); Super Supplier level; 7 years with Global Sources;
/// exhibited at 33 Global Sources trade shows."
fn platform_line(f: &PageFacts) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(v) = f.verified() {
        parts.push(format!(
            "{v} (Global Sources had the business registration checked by an outside company)"
        ));
    }
    if let Some(r) = f.ranking() {
        parts.push(format!("{r} level (a Global Sources membership level)"));
    }
    if let Some(y) = f.years_on_platform {
        parts.push(format!(
            "{y} {} with Global Sources",
            if y == 1 { "year" } else { "years" }
        ));
    }
    if let Some(n) = f.trade_shows {
        parts.push(format!("exhibited at {n} Global Sources trade shows"));
    } else if f.trade_show_exhibitor() {
        parts.push("exhibits at Global Sources trade shows".to_string());
    }
    (!parts.is_empty()).then(|| format!("{PLATFORM_PREFIX}{}.", parts.join("; ")))
}

fn company_lines(f: &PageFacts) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(c) = &f.country {
        lines.push(format!("Country: {c}."));
    }
    lines.extend(contact_lines(&f.contact));
    if let Some(y) = &f.founding_year {
        lines.push(format!("Founded: {y}."));
    }
    if let Some(b) = &f.business_type {
        lines.push(format!("Business type: {b}."));
    }
    for (label, name) in COMPANY_FACTS {
        if matches!(*label, "Country / Region" | "Business Type") {
            continue;
        }
        if let Some(value) = f.label(label) {
            lines.push(format!("{name}: {}.", value.trim_end_matches('.')));
        }
    }
    lines.extend(platform_line(f));
    match (&f.highlights, &f.slogan) {
        (Some(h), _) => lines.push(format!("Supplier's own headline: {h}.")),
        (None, Some(s)) => lines.push(format!("Supplier's own headline: {s}.")),
        _ => {}
    }
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
        lines.push(profile.clone());
    }
    lines
}

/// What the badge text says Global Sources checked (read by
/// signals.rs): "Verified Manufacturer", "Verified Supplier",
/// "Trade show exhibitor", or None.
fn badge_text(f: &PageFacts) -> Option<String> {
    match f.verified() {
        Some(v) => Some(v.to_string()),
        None if f.trade_show_exhibitor() => Some(TRADE_SHOW_ONLY.to_string()),
        None => None,
    }
}

fn supplier_from(f: &PageFacts, profile_url: &str) -> B2bSupplierProfile {
    let lines = company_lines(f);
    B2bSupplierProfile {
        company_name: f.company_name.clone(),
        logo_url: f.logo_url.clone(),
        year_established: f.founding_year.clone(),
        country: f.country.clone(),
        platform_verified_badge: f.verified().is_some(),
        employee_count: None,
        sales_revenue: None,
        export_percentage: None,
        profile_url: profile_url.to_string(),
        source_platform: PLATFORM.to_string(),
        contact_name: None,
        contact_phone: f.contact.phone.clone(),
        badge_honorific: badge_text(f),
        company_description: (!lines.is_empty()).then(|| lines.join("\n")),
        website_url: f.contact.website.clone(),
    }
}

fn record_from(f: &PageFacts) -> Option<SupplierRecord> {
    let record = SupplierRecord {
        platform: PLATFORM_NAME.to_string(),
        checked_by_platform: f.verified().is_some(),
        member_label: f.ranking(),
        years_on_platform: f.years_on_platform,
        business_types: f
            .business_type
            .as_deref()
            .map(|t| {
                t.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        ..Default::default()
    };
    let has_anything = record.checked_by_platform
        || record.member_label.is_some()
        || record.years_on_platform.is_some();
    has_anything.then_some(record)
}

/// The price tiers: "US$ 10.29 (100–499 Pieces); US$ 9.56 (500–999
/// Pieces)", with the first tier as the minimum order ("100 Pieces").
fn price_and_moq(document: &Html) -> (Option<String>, Option<String>) {
    let mut tiers: Vec<(String, String)> = Vec::new();
    for item in document.select(&selector(".price-range .item")) {
        let price = item
            .select(&selector(".num"))
            .next()
            .map(|e| text_of(&e))
            .unwrap_or_default();
        let quantity = item
            .select(&selector(".pieces"))
            .next()
            .map(|e| text_of(&e))
            .unwrap_or_default();
        if price.is_empty() || tiers.iter().any(|(p, q)| *p == price && *q == quantity) {
            continue;
        }
        tiers.push((price, quantity));
    }
    if tiers.is_empty() {
        return (None, None);
    }
    let price = tiers
        .iter()
        .map(|(p, q)| {
            if q.is_empty() {
                p.clone()
            } else {
                format!("{p} ({q})")
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
    (Some(price), moq)
}

/// The supplier's own product text ("Key Specifications/ Special
/// Features" and the description below it), where some suppliers also
/// write their FAQ and payment terms.
fn own_text_lines(document: &Html) -> Vec<String> {
    let mut lines = Vec::new();
    for text_box in document.select(&selector(".tpl_txt_editor, .product-detail-content")) {
        lines.extend(visible_lines(text_box));
    }
    lines.dedup();
    lines
}

/// What the supplier wrote about payment in its own text, e.g. "2: 30%
/// deposit, 70% balance before the shipment." Questions are skipped.
fn payment_notes(own_text: &[String]) -> Vec<String> {
    let mut notes: Vec<String> = Vec::new();
    let mut in_payment = false;
    for line in own_text {
        let lower = line.to_lowercase();
        let starts_section = lower == "payment" || lower == "payment:";
        let mentions = lower.contains("payment")
            || lower.contains("deposit")
            || lower.contains("western union")
            || lower.contains("moneygram")
            || lower.contains("paypal");
        if starts_section {
            in_payment = true;
            continue;
        }
        let looks_like_answer = mentions || line.chars().next().is_some_and(|c| c.is_ascii_digit());
        if (mentions || (in_payment && looks_like_answer))
            && !line.trim_end().ends_with('?')
            && line.chars().count() <= 300
        {
            if !notes.contains(line) {
                notes.push(line.clone());
            }
        } else if in_payment {
            in_payment = false;
        }
        if notes.len() >= MAX_PAYMENT_NOTES {
            break;
        }
    }
    notes
}

/// Adds what the exhibitor page gives and the product page did not
/// (it usually repeats the same supplier record): only missing fields
/// and new description lines.
fn enrich_from_exhibitor_page(
    mut supplier: B2bSupplierProfile,
    page: &show::ShowPage,
) -> B2bSupplierProfile {
    let more = page.supplier_profile(&supplier.profile_url);
    if supplier.company_name.is_none() {
        supplier.company_name = more.company_name;
    }
    if supplier.year_established.is_none() {
        supplier.year_established = more.year_established;
    }
    if supplier.country.is_none() {
        supplier.country = more.country;
    }
    if supplier.logo_url.is_none() {
        supplier.logo_url = more.logo_url;
    }
    supplier.platform_verified_badge |= more.platform_verified_badge;
    if supplier.badge_honorific.is_none() {
        supplier.badge_honorific = more.badge_honorific;
    }
    let mut description = supplier.company_description.take().unwrap_or_default();
    for line in more.company_description.unwrap_or_default().lines() {
        if !description.lines().any(|l| l == line) {
            if !description.is_empty() {
                description.push('\n');
            }
            description.push_str(line);
        }
    }
    supplier.company_description = (!description.is_empty()).then_some(description);
    supplier
}

impl B2bScraper for GlobalsourcesScraper {
    fn matches_platform(&self, platform: &str) -> bool {
        platform == PLATFORM
    }

    fn parse_supplier(&self, html: &str, profile_url: &str) -> B2bSupplierProfile {
        if let Some(page) = show::read(html) {
            return page.supplier_profile(profile_url);
        }
        supplier_from(&read_page(html), profile_url)
    }

    fn parse_listing(&self, html: &str, listing_url: &str) -> B2bListingProfile {
        if let Some(page) = show::read(html) {
            return page.listing(listing_url);
        }
        let facts = read_page(html);
        let document = Html::parse_document(html);
        let data = page_data(&document);
        let product = data_of_type(&data, "Product");
        let mut listing = B2bListingProfile {
            listing_url: listing_url.to_string(),
            source_platform: PLATFORM.to_string(),
            ..Default::default()
        };
        if !facts.is_product_page {
            return listing;
        }

        listing.title = product
            .and_then(|p| data_text(p, "name"))
            .or_else(|| first_text(document.root_element(), "h1"));

        // Photos: the page data, then the gallery (lazy pictures keep
        // their address in data-src).
        let mut images: Vec<String> = match product.and_then(|p| p.get("image")) {
            Some(Value::Array(list)) => list
                .iter()
                .filter_map(Value::as_str)
                .map(absolute_url)
                .collect(),
            Some(Value::String(one)) => vec![absolute_url(one)],
            _ => Vec::new(),
        };
        for img in document.select(&selector("img")) {
            let src = img
                .value()
                .attr("data-src")
                .or_else(|| img.value().attr("src"))
                .unwrap_or("");
            if src.contains("/IMAGES/PDT/") {
                let url = absolute_url(src.split('?').next().unwrap_or(src));
                let plain = |u: &String| u.split('?').next().unwrap_or(u).to_string();
                if !images.iter().any(|i| plain(i) == url) {
                    images.push(url);
                }
            }
        }
        images.truncate(MAX_IMAGES);
        listing.image_urls = images;

        // Product description: key attributes, details, certificate and
        // the supplier's own text. (The page data's description is a
        // Global Sources template, so it is not used.)
        let attributes = labeled_values(&document, ATTRIBUTE_PAIRS);
        let detail_names = [
            "Model Number",
            "Brand Name",
            "origin",
            "Small Orders",
            "Weight per Unit",
            "Dimensions per Unit",
            "HTS Code",
        ];
        let mut shown: Vec<String> = attributes
            .iter()
            .chain(
                facts
                    .labels
                    .iter()
                    .filter(|(n, _)| detail_names.iter().any(|d| d.eq_ignore_ascii_case(n))),
            )
            .filter(|(_, v)| v.chars().count() <= 150)
            .map(|(n, v)| format!("{n}: {v}"))
            .collect();
        shown.dedup();
        shown.truncate(MAX_PRODUCT_DETAILS);
        let mut parts: Vec<String> = Vec::new();
        if !shown.is_empty() {
            parts.push(format!("{PRODUCT_DETAILS_PREFIX}{}.", shown.join("; ")));
        }
        let certificate: Vec<String> = [
            "Certificate Standard",
            "Certificate Number",
            "Validity",
            "Issued By",
        ]
        .iter()
        .filter_map(|n| facts.label(n).map(|v| format!("{n}: {v}")))
        .collect();
        if !certificate.is_empty() {
            parts.push(format!("{CERTIFICATE_PREFIX}{}.", certificate.join("; ")));
        }
        let own_text = own_text_lines(&document);
        if !own_text.is_empty() {
            parts.push(format!(
                "{OWN_TEXT_PREFIX}{}",
                cut(&own_text.join(" "), MAX_OWN_TEXT_CHARS)
            ));
        }
        listing.description = (!parts.is_empty()).then(|| parts.join("\n"));

        let (price, moq) = price_and_moq(&document);
        listing.unit_price = price.or_else(|| {
            let offers = product.and_then(|p| p.get("offers"))?;
            let low = data_text(offers, "lowPrice")?;
            Some(match data_text(offers, "highPrice") {
                Some(high) if high != low => format!("US$ {low}-{high}"),
                _ => format!("US$ {low}"),
            })
        });
        listing.minimum_order_quantity = moq;

        // Payment: the "Payment Method" row, plus what the supplier wrote
        // about payment in its own text (e.g. "30% deposit, 70% balance
        // before the shipment").
        let notes = payment_notes(&own_text);
        let method = facts.label("Payment Method").map(str::to_string);
        listing.payment_type = match (method, notes.is_empty()) {
            (Some(m), true) => Some(m),
            (Some(m), false) => Some(format!("{m}; {PAYMENT_WORDS_PREFIX}{}", notes.join(" "))),
            (None, false) => Some(format!("{PAYMENT_WORDS_PREFIX}{}", notes.join(" "))),
            (None, true) => None,
        };

        listing.preferred_port = facts.label("FOB Port").map(str::to_string);
        listing.incoterms = listing.preferred_port.as_ref().map(|p| format!("FOB {p}"));
        listing.delivery_timeframe = facts.label("Lead Time").map(str::to_string);
        listing.reference = facts.label("Model Number").map(str::to_string);
        let packaging: Vec<String> = [
            "Units per Export Carton",
            "Export Carton Dimensions L/W/H",
            "Export Carton Weight",
        ]
        .iter()
        .filter_map(|n| facts.label(n).map(|v| format!("{n}: {v}")))
        .collect();
        listing.packaging_details = (!packaging.is_empty()).then(|| packaging.join("; "));
        listing
    }

    /// The supplier's page, linked from the product page.
    fn extract_company_profile_url(&self, listing_html: &str) -> Option<String> {
        if let Some(page) = show::read(listing_html) {
            return page
                .is_product_page()
                .then(|| page.exhibitor_url())
                .flatten();
        }
        let facts = read_page(listing_html);
        if facts.is_product_page {
            facts.company_url
        } else {
            None
        }
    }

    /// The supplier's site, e.g. "yongfang.manufacturer.globalsources.com":
    /// the same on its own page and on every product it sells.
    fn company_key(&self, listing_html: &str) -> Option<String> {
        if let Some(page) = show::read(listing_html) {
            return page.company_key();
        }
        read_page(listing_html)
            .company_url
            .as_deref()
            .and_then(supplier_host)
    }

    fn supplier_record(&self, listing_html: &str) -> Option<SupplierRecord> {
        if let Some(page) = show::read(listing_html) {
            return page.record();
        }
        record_from(&read_page(listing_html))
    }

    /// Adds what only the supplier's page shows: founding year, ranking,
    /// export markets and the company's own profile. What the product
    /// page already gave is kept.
    fn enrich_from_company_profile(
        &self,
        mut supplier: B2bSupplierProfile,
        profile_html: &str,
    ) -> B2bSupplierProfile {
        if let Some(page) = show::read(profile_html) {
            return enrich_from_exhibitor_page(supplier, &page);
        }
        let f = read_page(profile_html);
        if supplier.company_name.is_none() {
            supplier.company_name = f.company_name.clone();
        }
        if supplier.year_established.is_none() {
            supplier.year_established = f.founding_year.clone();
        }
        if supplier.country.is_none() {
            supplier.country = f.country.clone();
        }
        if supplier.logo_url.is_none() {
            supplier.logo_url = f.logo_url.clone();
        }
        if supplier.website_url.is_none() {
            supplier.website_url = f.contact.website.clone();
        }
        if supplier.contact_phone.is_none() {
            supplier.contact_phone = f.contact.phone.clone();
        }
        supplier.platform_verified_badge |= f.verified().is_some();
        let better_badge = supplier.badge_honorific.is_none()
            || (f.verified().is_some()
                && !supplier
                    .badge_honorific
                    .as_deref()
                    .is_some_and(|b| b.starts_with(VERIFIED_PREFIX)));
        if better_badge && let Some(b) = badge_text(&f) {
            supplier.badge_honorific = Some(b);
        }

        // Only lines the product page did not already give (by their
        // start, e.g. "Country:"). The profile text is added unless the
        // same text is already there.
        let mut description = supplier.company_description.take().unwrap_or_default();
        for line in company_lines(&f) {
            let key = line.split(':').next().unwrap_or("");
            let is_profile = f.profile_text.as_deref() == Some(line.as_str());
            let already = if is_profile {
                description.lines().any(|l| l == line)
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

    /// Trade-show product pages show no price: it comes from the
    /// exhibitor page's product list.
    fn enrich_listing_from_company_profile(
        &self,
        mut listing: B2bListingProfile,
        profile_html: &str,
    ) -> B2bListingProfile {
        if let Some(page) = show::read(profile_html) {
            page.add_price(&mut listing);
        }
        listing
    }

    fn enrich_record_from_company_profile(
        &self,
        record: Option<SupplierRecord>,
        profile_html: &str,
    ) -> Option<SupplierRecord> {
        let page = match show::read(profile_html) {
            Some(show_page) => show_page.record(),
            None => record_from(&read_page(profile_html)),
        };
        match (record, page) {
            (None, page) => page,
            (Some(r), None) => Some(r),
            (Some(mut r), Some(p)) => {
                r.checked_by_platform |= p.checked_by_platform;
                r.member_label = r.member_label.or(p.member_label);
                r.years_on_platform = r.years_on_platform.or(p.years_on_platform);
                if r.business_types.is_empty() {
                    r.business_types = p.business_types;
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
    fn supplier_host_is_only_a_supplier_site() {
        assert_eq!(
            supplier_host("//yongfang.manufacturer.globalsources.com/homepage_6008825952206.htm")
                .as_deref(),
            Some("yongfang.manufacturer.globalsources.com")
        );
        assert_eq!(
            supplier_host("https://www.globalsources.com/TWS-earbud/x-1182434134p.htm"),
            None
        );
        assert_eq!(
            supplier_host("https://p.globalsources.com/IMAGES/x.jpg"),
            None
        );
        assert_eq!(supplier_host("https://example.com/homepage_1.htm"), None);
    }

    #[test]
    fn the_companys_own_name_written_differently_is_not_another_company() {
        let profile = "Over 29 Years of Global Manufacturing Experience Dongguan Yongfang Electronics Technology Co., Ltd. is a professional OEM/ODM manufacturer. Our factory works with Shenzhen Other Plastics Co., Ltd. for molds.";
        assert_eq!(
            other_company_names(
                profile,
                Some("Dongguan Yongfang Electronic Technology Co. Ltd")
            ),
            vec!["Shenzhen Other Plastics Co., Ltd."]
        );
        assert!(
            other_company_names(
                "Yueyinda Electronic Co., Ltd. was founded in 2016.",
                Some("Guangdong Yueyinda Electronic Co., Ltd.")
            )
            .is_empty(),
            "the name without the province is still the company itself"
        );
    }

    #[test]
    fn payment_lines_are_read_from_the_supplier_text() {
        let text: Vec<String> = [
            "Payment",
            "1: we can accept T/T and Paypal , you can choose yourself.",
            "2: 30% deposit, 70% balance before the shipment.",
            "Shipping",
            "Sample in stock: 1-3 days",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            payment_notes(&text),
            vec![
                "1: we can accept T/T and Paypal , you can choose yourself.",
                "2: 30% deposit, 70% balance before the shipment."
            ]
        );
    }
}
