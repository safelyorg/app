use super::{B2bListingProfile, B2bScraper, B2bSupplierProfile, SupplierRecord};
use chrono::{Datelike, NaiveDate, Utc};
use serde_json::Value;
use std::collections::HashMap;

/// Wholesale Central (wholesalecentral.com): a US wholesale directory.
///
/// Two kinds of page are scanned:
/// - a product page, /products/{id}: the product, plus the supplier's
///   name, city and minimum order. The supplier's phone, address and
///   website are only on the supplier page, so that page is fetched
///   too (extract_company_profile_url).
/// - a supplier page, /suppliers/{name}: everything about the
///   supplier, in one fetch. It lists many products, so no single
///   product is read from it.
///
/// Prices: Wholesale Central shows prices only to signed-in buyers
/// ("Unlock wholesale price"). The page code still has a number for
/// each product, but Safely does not use a price the site hides on
/// purpose, so unit_price is always None here.
pub struct WholesalecentralScraper;

const PLATFORM: &str = "wholesalecentral";
const PLATFORM_NAME: &str = "Wholesale Central";
const SITE: &str = "https://www.wholesalecentral.com";

/// Start of the address line put in front of the company description,
/// the same as on the other B2B platforms.
const ADDRESS_PREFIX: &str = "Address: ";

/// Most of the supplier's own categories kept for the product check.
const MAX_CATEGORIES: usize = 10;

// ---------------------------------------------------------------------
// The page's data
// ---------------------------------------------------------------------
//
// Wholesale Central is built with Next.js. The page's data is not in
// one JSON block (like ThomasNet's __NEXT_DATA__) but in many
// `self.__next_f.push([1,"..."])` scripts. Joined together they make a
// list of rows, one per line: `3b:{"phoneNumber":"$3e",...}`. A value
// like "$3e" points to another row (row 3e holds the phone number).
// A row like `66:T5dd,<text>` is a piece of text 0x5dd bytes long,
// with no line break after it.

/// The page's data rows, by row id.
struct PageData {
    rows: HashMap<String, Value>,
    /// Row ids in page order, so the first match is always the same.
    order: Vec<String>,
}

impl PageData {
    fn read(html: &str) -> Self {
        let payload = page_payload(html);
        let (rows, order) = parse_rows(&payload);
        PageData { rows, order }
    }

    /// Follows "$3e"-style links to the row they point to.
    fn resolve<'a>(&'a self, value: &'a Value) -> &'a Value {
        let mut current = value;
        for _ in 0..8 {
            match current {
                Value::String(s) if is_row_link(s) => match self.rows.get(&s[1..]) {
                    Some(next) => current = next,
                    None => break,
                },
                _ => break,
            }
        }
        current
    }

    /// A field as clean text: links followed, a one-item list
    /// unwrapped, "$undefined" and empty text treated as missing.
    fn text(&self, object: &Value, key: &str) -> Option<String> {
        let value = self.resolve(object.get(key)?);
        let value = match value {
            Value::Array(items) => items
                .iter()
                .map(|v| self.resolve(v))
                .find(|v| !v.is_null())?,
            other => other,
        };
        match value {
            Value::String(s) => clean_text(s),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    }

    /// A field that is a list of rows (links followed for each item).
    fn list<'a>(&'a self, object: &'a Value, key: &str) -> Vec<&'a Value> {
        match object.get(key).map(|v| self.resolve(v)) {
            Some(Value::Array(items)) => items.iter().map(|v| self.resolve(v)).collect(),
            _ => Vec::new(),
        }
    }

    /// The first object on the page that `matches`, in page order.
    fn find_object(
        &self,
        matches: impl Fn(&serde_json::Map<String, Value>) -> bool,
    ) -> Option<&Value> {
        self.order
            .iter()
            .filter_map(|id| self.rows.get(id))
            .find_map(|row| find_in(row, &matches))
    }

    /// The supplier record on a supplier page (the one with contact
    /// details - the page also has a few shorter copies).
    fn supplier(&self) -> Option<&Value> {
        self.find_object(|o| {
            o.contains_key("supplierBusinessName") && o.contains_key("contactInfo")
        })
    }

    /// The product record on a product page.
    fn product(&self) -> Option<&Value> {
        self.find_object(|o| {
            o.contains_key("productVariants")
                && o.contains_key("supplierName")
                && o.contains_key("subdirectory")
        })
    }
}

fn find_in<'a>(
    value: &'a Value,
    matches: &impl Fn(&serde_json::Map<String, Value>) -> bool,
) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            if matches(map) {
                return Some(value);
            }
            map.values().find_map(|v| find_in(v, matches))
        }
        Value::Array(items) => items.iter().find_map(|v| find_in(v, matches)),
        _ => None,
    }
}

/// "$3e" (a link to row 3e). Not "$undefined", "$D2017-..." (a date)
/// or "$$300" (the text "$300").
fn is_row_link(s: &str) -> bool {
    s.len() > 1 && s.starts_with('$') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Text with the page's markers removed: "$undefined" is missing,
/// "$D2017-06-26..." is the date, "$$300" is "$300".
fn clean_text(raw: &str) -> Option<String> {
    let text = if raw == "$undefined" {
        ""
    } else if let Some(date) = raw.strip_prefix("$D") {
        date
    } else if let Some(rest) = raw.strip_prefix("$$") {
        return clean_text(&format!("${rest}"));
    } else {
        raw
    };
    let text = text.replace('\r', "");
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Joins the text of every `self.__next_f.push([1,"..."])` script.
fn page_payload(html: &str) -> String {
    const MARKER: &str = "self.__next_f.push(";
    let mut payload = String::new();
    let mut rest = html;
    while let Some(at) = rest.find(MARKER) {
        rest = &rest[at + MARKER.len()..];
        let mut stream = serde_json::Deserializer::from_str(rest).into_iter::<Value>();
        // Only [1, "text"] pushes carry the page's data.
        let text = match stream.next() {
            Some(Ok(Value::Array(parts))) if parts.first().and_then(|v| v.as_i64()) == Some(1) => {
                parts.get(1).and_then(|v| v.as_str()).map(str::to_string)
            }
            _ => None,
        };
        if let Some(text) = text {
            payload.push_str(&text);
        }
    }
    payload
}

/// Splits the joined text into rows (see "The page's data" above).
fn parse_rows(payload: &str) -> (HashMap<String, Value>, Vec<String>) {
    let bytes = payload.as_bytes();
    let mut rows = HashMap::new();
    let mut order = Vec::new();
    let mut i = 0;
    let next_line = |from: usize| {
        bytes[from..]
            .iter()
            .position(|&c| c == b'\n')
            .map(|n| from + n)
            .unwrap_or(bytes.len())
    };

    while i < bytes.len() {
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
            i += 1;
        }
        if i == start || i >= bytes.len() || bytes[i] != b':' {
            // Not the start of a row: skip to the next line.
            i = next_line(i) + 1;
            continue;
        }
        let id = payload[start..i].to_string();
        i += 1;

        // A text row: T{length in hex},{text}
        if bytes.get(i) == Some(&b'T') {
            let len_start = i + 1;
            let mut j = len_start;
            while j < bytes.len() && bytes[j].is_ascii_hexdigit() {
                j += 1;
            }
            let stated_len = if bytes.get(j) == Some(&b',') {
                usize::from_str_radix(&payload[len_start..j], 16).ok()
            } else {
                None
            };
            if let Some(len) = stated_len {
                let text_start = j + 1;
                let text_end = json_text_end(bytes, text_start)
                    .unwrap_or_else(|| (text_start + len).min(bytes.len()));
                let text = String::from_utf8_lossy(&bytes[text_start..text_end]).into_owned();
                rows.insert(id.clone(), Value::String(text));
                order.push(id);
                i = text_end;
                continue;
            }
        }

        let end = next_line(i);
        if let Ok(value) = serde_json::from_slice::<Value>(&bytes[i..end]) {
            rows.insert(id.clone(), value);
            order.push(id);
        }
        i = end + 1;
    }
    (rows, order)
}

/// Where a text row ends when its text is a JSON object (the
/// descriptions are). The row's stated length was found to be a few
/// bytes off on real pages, which made the next rows unreadable (the
/// supplier's description came out as "$32"), so the end of the JSON
/// itself is used. None for plain text: the stated length is used.
fn json_text_end(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    let mut stream = serde_json::Deserializer::from_slice(&bytes[start..]).into_iter::<Value>();
    match stream.next() {
        Some(Ok(_)) => Some(start + stream.byte_offset()),
        _ => None,
    }
}

// ---------------------------------------------------------------------
// Small readers
// ---------------------------------------------------------------------

/// The plain text of a description. Wholesale Central keeps
/// descriptions in the editor's format ({"root":{"children":[...]}});
/// each paragraph becomes one line. Plain text is kept as it is.
fn description_text(raw: &str) -> Option<String> {
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return clean_text(raw);
    };
    let Some(root) = value.get("root") else {
        return clean_text(raw);
    };
    let mut lines = Vec::new();
    collect_paragraphs(root, &mut lines);
    let text = lines
        .into_iter()
        .filter_map(|l| clean_text(&l))
        .collect::<Vec<_>>()
        .join("\n");
    clean_text(&text)
}

fn collect_paragraphs(node: &Value, lines: &mut Vec<String>) {
    let kind = node.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if matches!(kind, "paragraph" | "heading" | "listitem" | "quote") {
        let mut text = String::new();
        collect_text(node, &mut text);
        lines.push(text);
        return;
    }
    if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
        for child in children {
            collect_paragraphs(child, lines);
        }
    }
}

fn collect_text(node: &Value, out: &mut String) {
    if let Some(t) = node.get("text").and_then(|t| t.as_str()) {
        out.push_str(t);
    }
    if node.get("type").and_then(|t| t.as_str()) == Some("linebreak") {
        out.push('\n');
    }
    if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
        for child in children {
            collect_text(child, out);
        }
    }
}

/// The supplier's website as "https://bloomwholesale.com". The page
/// gives either the home page, a link with tracking extras
/// ("upsellwholesale.com/?utm_source=...") or, on a product page, the
/// product on the supplier's own shop - only the site itself is kept.
fn website_from(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let (scheme, rest) = match raw.split_once("://") {
        Some((scheme, rest)) => (scheme.to_lowercase(), rest),
        None => ("https".to_string(), raw),
    };
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    if host.is_empty() || !host.contains('.') || host.contains(' ') {
        return None;
    }
    Some(format!("{scheme}://{host}"))
}

/// The date part of "2017-06-26T07:52:17.610Z".
fn date_from(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw.get(..10)?, "%Y-%m-%d").ok()
}

/// Whole years between `since` and `today`.
fn full_years(since: NaiveDate, today: NaiveDate) -> u32 {
    today.years_since(since).unwrap_or(0)
}

/// "No minimum order" or "$300 minimum order".
fn minimum_order_text(amount: Option<String>) -> Option<String> {
    let amount = amount?;
    let number: f64 = amount.parse().ok()?;
    if number <= 0.0 {
        Some("No minimum order".to_string())
    } else {
        let shown = if number.fract() == 0.0 {
            format!("{}", number as i64)
        } else {
            format!("{number:.2}")
        };
        Some(format!("${shown} minimum order"))
    }
}

fn join_unique(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for v in values {
        if !out.iter().any(|o| o.eq_ignore_ascii_case(&v)) {
            out.push(v);
        }
    }
    out
}

/// The supplier page's link for a supplier's subdirectory.
fn supplier_page_url(subdirectory: &str) -> String {
    format!("{SITE}/suppliers/{subdirectory}")
}

// ---------------------------------------------------------------------
// The supplier
// ---------------------------------------------------------------------

/// Everything the supplier page says about the supplier.
fn supplier_from_supplier_page(
    data: &PageData,
    company: &Value,
    profile_url: &str,
) -> B2bSupplierProfile {
    let name = data.text(company, "supplierBusinessName");

    let contact = data.list(company, "contactInfo").into_iter().next();
    let phone = contact.and_then(|c| {
        data.text(c, "salesNumber")
            .or_else(|| data.text(c, "phoneNumber"))
            .or_else(|| data.text(c, "billingNumber"))
    });
    let emails = join_unique(
        [
            contact.and_then(|c| data.text(c, "salesEmail")),
            data.text(company, "email"),
            contact.and_then(|c| data.text(c, "emailAddress")),
        ]
        .into_iter()
        .flatten(),
    );

    let address = data.list(company, "addressInfo").into_iter().next();
    let street = address.and_then(|a| {
        let parts: Vec<String> = ["streetAddress", "unitNumber"]
            .iter()
            .filter_map(|k| data.text(a, k))
            .collect();
        (!parts.is_empty()).then(|| parts.join(" "))
    });
    let city = address
        .and_then(|a| data.text(a, "city"))
        .or_else(|| data.text(company, "companyCity"));
    let state_code = address
        .and_then(|a| a.get("stateOrProvince").map(|s| data.resolve(s)))
        .and_then(|s| data.text(s, "code"))
        .or_else(|| data.text(company, "companyStateCode"))
        .or_else(|| data.text(company, "companyState"));
    let zip = address.and_then(|a| data.text(a, "postalCode"));
    let country = address
        .and_then(|a| a.get("country").map(|c| data.resolve(c)))
        .and_then(|c| data.text(c, "name"))
        .or_else(|| data.text(company, "companyCountry"));

    // "Long Beach, CA, United States"
    let location = {
        let parts: Vec<String> = [city.clone(), state_code.clone(), country.clone()]
            .into_iter()
            .flatten()
            .collect();
        (!parts.is_empty()).then(|| parts.join(", "))
    };

    // "Address: 2850 E. 29th St., Long Beach, CA 90806, United States."
    let address_line = street.map(|street| {
        let state_zip = match (state_code.clone(), zip) {
            (Some(s), Some(z)) => Some(format!("{s} {z}")),
            (s, z) => s.or(z),
        };
        let parts: Vec<String> = [Some(street), city, state_zip, country]
            .into_iter()
            .flatten()
            .collect();
        format!(
            "{ADDRESS_PREFIX}{}.",
            parts.join(", ").trim_end_matches('.')
        )
    });

    let about = data
        .text(company, "supplierBusinessDescription")
        .and_then(|d| description_text(&d));
    let selling_points = data
        .text(company, "companyKeySellingPoints")
        .map(|p| format!("Key selling points:\n{p}"));
    let email_line =
        (!emails.is_empty()).then(|| format!("Email on {PLATFORM_NAME}: {}.", emails.join(", ")));
    let minimum_line = minimum_order_text(data.text(company, "minOrder")).map(|m| format!("{m}."));
    let sample_line = (company.get("provideSamplePack").and_then(|v| v.as_bool()) == Some(true))
        .then(|| "Offers a sample pack.".to_string());

    let company_description = {
        let parts: Vec<String> = [
            address_line,
            about,
            selling_points,
            email_line,
            minimum_line,
            sample_line,
        ]
        .into_iter()
        .flatten()
        .collect();
        (!parts.is_empty()).then(|| parts.join("\n"))
    };

    B2bSupplierProfile {
        company_name: name,
        logo_url: data.text(company, "supplierBusinessIcon"),
        // Wholesale Central does not show when a company was founded.
        // "Member since" is when it joined Wholesale Central, so it goes
        // into the supplier record (years_on_platform), not here.
        year_established: None,
        country: location,
        // Wholesale Central has no verification of its own.
        platform_verified_badge: false,
        employee_count: None,
        sales_revenue: None,
        export_percentage: None,
        profile_url: profile_url.to_string(),
        source_platform: PLATFORM.to_string(),
        contact_name: None,
        contact_phone: phone,
        badge_honorific: None,
        company_description,
        website_url: data
            .text(company, "wholesaleLink")
            .and_then(|w| website_from(&w)),
    }
}

/// What a product page says about its supplier: name, logo, city and
/// minimum order. Phone, address and website come from the supplier
/// page (enrich_from_company_profile).
fn supplier_from_product_page(
    data: &PageData,
    product: &Value,
    page_url: &str,
) -> B2bSupplierProfile {
    let city = data.text(product, "companyCity");
    let state = data
        .text(product, "companyStateCode")
        .or_else(|| data.text(product, "companyState"));
    let location = {
        let parts: Vec<String> = [city, state].into_iter().flatten().collect();
        (!parts.is_empty()).then(|| parts.join(", "))
    };
    let about = data.text(product, "supplierProductLine");
    let email_line = data
        .text(product, "email")
        .map(|e| format!("Email on {PLATFORM_NAME}: {e}."));
    let minimum_line =
        minimum_order_text(data.text(product, "supplierMinimumOrder")).map(|m| format!("{m}."));
    let company_description = {
        let parts: Vec<String> = [about, email_line, minimum_line]
            .into_iter()
            .flatten()
            .collect();
        (!parts.is_empty()).then(|| parts.join("\n"))
    };

    B2bSupplierProfile {
        company_name: data.text(product, "supplierName"),
        logo_url: data.text(product, "supplierBusinessIcon"),
        country: location,
        profile_url: page_url.to_string(),
        source_platform: PLATFORM.to_string(),
        company_description,
        // The product's link on the supplier's own shop gives the site.
        website_url: data
            .text(product, "wholesaleLink")
            .and_then(|w| website_from(&w)),
        ..Default::default()
    }
}

/// Years on Wholesale Central and the supplier's categories.
fn record_from(data: &PageData, object: &Value, categories: Vec<String>) -> Option<SupplierRecord> {
    let joined = data.text(object, "memberSince").and_then(|d| date_from(&d));
    if joined.is_none() && categories.is_empty() {
        return None;
    }
    let today = Utc::now().date_naive();
    Some(SupplierRecord {
        platform: PLATFORM_NAME.to_string(),
        years_on_platform: joined.map(|d| full_years(d, today)),
        joined_platform_year: joined.map(|d| d.year()),
        products_offered: categories,
        ..Default::default()
    })
}

fn supplier_categories(data: &PageData, company: &Value) -> Vec<String> {
    join_unique(
        data.list(company, "categories")
            .into_iter()
            .filter_map(|c| data.text(c, "name")),
    )
    .into_iter()
    .take(MAX_CATEGORIES)
    .collect()
}

// ---------------------------------------------------------------------
// The scraper
// ---------------------------------------------------------------------

impl B2bScraper for WholesalecentralScraper {
    fn matches_platform(&self, platform: &str) -> bool {
        platform == PLATFORM
    }

    fn parse_supplier(&self, html: &str, profile_url: &str) -> B2bSupplierProfile {
        let data = PageData::read(html);
        if let Some(company) = data.supplier() {
            return supplier_from_supplier_page(&data, company, profile_url);
        }
        if let Some(product) = data.product() {
            return supplier_from_product_page(&data, product, profile_url);
        }
        B2bSupplierProfile {
            profile_url: profile_url.to_string(),
            source_platform: PLATFORM.to_string(),
            ..Default::default()
        }
    }

    /// The product on a product page. A supplier page lists many
    /// products, so its listing is left empty - the panel then asks the
    /// buyer to open a product to check it.
    fn parse_listing(&self, html: &str, listing_url: &str) -> B2bListingProfile {
        let empty = B2bListingProfile {
            listing_url: listing_url.to_string(),
            source_platform: PLATFORM.to_string(),
            ..Default::default()
        };
        let data = PageData::read(html);
        let Some(product) = data.product() else {
            return empty;
        };

        let description = data
            .text(product, "description")
            .and_then(|d| description_text(&d))
            .or_else(|| {
                data.text(product, "detailedDescription")
                    .and_then(|d| description_text(&d))
            });
        let category = data.text(product, "category");
        let description = match (description, category) {
            (Some(d), Some(c)) => Some(format!("{d}\nCategory on {PLATFORM_NAME}: {c}.")),
            (d, c) => d.or_else(|| c.map(|c| format!("Category on {PLATFORM_NAME}: {c}."))),
        };

        let image_urls = join_unique(
            ["largeImage", "image", "thumbnail"]
                .iter()
                .filter_map(|k| data.text(product, k))
                .filter(|u| u.starts_with("http")),
        );

        let reference = data
            .list(product, "productVariants")
            .into_iter()
            .find_map(|v| data.text(v, "sku"))
            .or_else(|| data.text(product, "code"));

        B2bListingProfile {
            title: data.text(product, "name"),
            description,
            image_urls,
            // Hidden from visitors who are not signed in - not used.
            unit_price: None,
            fob_price: None,
            minimum_order_quantity: minimum_order_text(data.text(product, "supplierMinimumOrder")),
            reference,
            ..empty
        }
    }

    /// On a product page: the supplier's page, which has the phone,
    /// address and website. On a supplier page: None (it already is
    /// the supplier's page).
    fn extract_company_profile_url(&self, listing_html: &str) -> Option<String> {
        let data = PageData::read(listing_html);
        if data.supplier().is_some() {
            return None;
        }
        let product = data.product()?;
        data.text(product, "subdirectory")
            .map(|s| supplier_page_url(&s))
    }

    /// The supplier page's details win; what the product page gave is
    /// kept only where the supplier page has nothing.
    fn enrich_from_company_profile(
        &self,
        supplier: B2bSupplierProfile,
        profile_html: &str,
    ) -> B2bSupplierProfile {
        let data = PageData::read(profile_html);
        let Some(company) = data.supplier() else {
            return supplier;
        };
        let full = supplier_from_supplier_page(&data, company, &supplier.profile_url);
        B2bSupplierProfile {
            company_name: full.company_name.or(supplier.company_name),
            logo_url: full.logo_url.or(supplier.logo_url),
            country: full.country.or(supplier.country),
            contact_phone: full.contact_phone.or(supplier.contact_phone),
            company_description: full.company_description.or(supplier.company_description),
            website_url: full.website_url.or(supplier.website_url),
            ..supplier
        }
    }

    /// One record per supplier: Wholesale Central's own number for the
    /// company ("40952"). The same on its supplier page and on every
    /// one of its product pages, so they share one Safely history.
    fn company_key(&self, listing_html: &str) -> Option<String> {
        let data = PageData::read(listing_html);
        if let Some(company) = data.supplier() {
            return data
                .text(company, "id")
                .or_else(|| data.text(company, "subdirectory").map(|s| s.to_lowercase()));
        }
        let product = data.product()?;
        data.text(product, "supplierId")
            .or_else(|| data.text(product, "subdirectory").map(|s| s.to_lowercase()))
    }

    /// How long the supplier has been on Wholesale Central ("Member
    /// since 2017") and what it sells.
    fn supplier_record(&self, listing_html: &str) -> Option<SupplierRecord> {
        let data = PageData::read(listing_html);
        if let Some(company) = data.supplier() {
            let categories = supplier_categories(&data, company);
            return record_from(&data, company, categories);
        }
        let product = data.product()?;
        let categories = data.text(product, "category").into_iter().collect();
        record_from(&data, product, categories)
    }

    /// Adds the supplier page's categories (a product page knows only
    /// the product's own category).
    fn enrich_record_from_company_profile(
        &self,
        record: Option<SupplierRecord>,
        profile_html: &str,
    ) -> Option<SupplierRecord> {
        let data = PageData::read(profile_html);
        let Some(company) = data.supplier() else {
            return record;
        };
        let categories = supplier_categories(&data, company);
        let from_page = record_from(&data, company, categories);
        match (record, from_page) {
            (Some(mut r), Some(page)) => {
                r.years_on_platform = r.years_on_platform.or(page.years_on_platform);
                r.joined_platform_year = r.joined_platform_year.or(page.joined_platform_year);
                let all = r.products_offered.drain(..).chain(page.products_offered);
                r.products_offered = join_unique(all);
                Some(r)
            }
            (r, page) => r.or(page),
        }
    }
}

/// A Wholesale Central page Safely scans: a product page
/// (/products/33417104) or a supplier page (/suppliers/luxiso0001).
/// Category, search and other list pages show many suppliers, so they
/// are not scanned.
pub fn is_wholesalecentral_scan_url(path: &str) -> bool {
    let parts: Vec<&str> = path
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    matches!(parts.as_slice(), ["products", id] if id.bytes().all(|b| b.is_ascii_digit()))
        || matches!(parts.as_slice(), ["suppliers", name] if !name.is_empty())
}

pub fn matches_wholesalecentral_hostname(hostname: &str) -> bool {
    hostname == "www.wholesalecentral.com" || hostname == "wholesalecentral.com"
}

#[cfg(test)]
mod tests {
    use super::*;

    // Small pages built the same way as the real ones: the data rows
    // inside self.__next_f.push([1,"..."]) scripts.
    fn page(rows: &str) -> String {
        let pushed = serde_json::to_string(&serde_json::json!([1, rows])).unwrap();
        format!(
            "<!DOCTYPE html><html><body><script>self.__next_f.push({pushed})</script></body></html>"
        )
    }

    fn supplier_page() -> String {
        page(concat!(
            "2e:{\"id\":40952,\"supplierBusinessName\":\"Bloom - USA Wholesale\",",
            "\"supplierBusinessDescription\":\"{\\\"root\\\":{\\\"children\\\":[{\\\"children\\\":[{\\\"text\\\":\\\"Trusted USA based wholesaler.\\\",\\\"type\\\":\\\"text\\\"}],\\\"type\\\":\\\"paragraph\\\"}],\\\"type\\\":\\\"root\\\"}}\",",
            "\"supplierBusinessIcon\":\"https://cdn.example/logo\",\"memberSince\":\"$D2017-06-26T07:52:17.610Z\",",
            "\"minOrder\":300,\"companyCity\":\"Long Beach\",\"companyStateCode\":\"CA\",\"companyCountry\":\"United States\",",
            "\"companyKeySellingPoints\":\"\",\"subdirectory\":\"luxiso0001\",\"provideSamplePack\":false,",
            "\"wholesaleLink\":\"bloomwholesale.com/?utm_source=wholesalecentral\",\"email\":\"info@luxisocool.com\",",
            "\"categories\":[\"$33\"],\"contactInfo\":\"$3a\",\"addressInfo\":\"$62\"}\n",
            "33:{\"id\":1,\"name\":\"Apparel / Clothing\"}\n",
            "3a:[\"$3b\"]\n",
            "3b:{\"emailAddress\":\"$3c\",\"phoneNumber\":\"$3e\",\"salesEmail\":\"$3f\",\"salesNumber\":\"$40\"}\n",
            "3c:[\"luxisocool@gmail.com\"]\n",
            "3e:[\"916-217-3478\"]\n",
            "3f:[\"info@luxisocool.com\"]\n",
            "40:[null]\n",
            "62:[\"$63\"]\n",
            "63:{\"streetAddress\":\"2850 E. 29th St.\",\"city\":\"Long Beach\",\"postalCode\":\"90806\",\"country\":\"$64\",\"stateOrProvince\":\"$65\"}\n",
            "64:{\"name\":\"United States\"}\n",
            "65:{\"name\":\"California\",\"code\":\"CA\"}\n",
        ))
    }

    fn product_page() -> String {
        page(concat!(
            "66:T27,{\"root\":{\"children\":[]},\"x\":\"text row\"}",
            "b:[\"$\",\"div\",null,{\"product\":{\"id\":33417104,\"name\":\"Patchwork Top\",",
            "\"image\":\"https://cdn.shopify.com/top.jpg\",\"thumbnail\":\"\",\"largeImage\":null,",
            "\"description\":\"Soft cotton top.\",\"category\":\"Plus Size Clothing\",",
            "\"supplierName\":\"Bloom - USA Wholesale\",\"supplierId\":40952,\"supplierMinimumOrder\":0,",
            "\"memberSince\":\"2017-06-26T07:52:17.61\",\"email\":\"info@luxisocool.com\",",
            "\"companyCity\":\"Long Beach\",\"companyStateCode\":\"CA\",\"subdirectory\":\"luxiso0001\",",
            "\"wholesaleLink\":\"https://bloomwholesale.com/products/top?utm_source=wholesale_central\",",
            "\"pricing\":[],\"productVariants\":[{\"priceAdjustmentAmount\":25,\"sku\":\"212-Bin72\"}]}}]\n",
        ))
    }

    #[test]
    fn matches_platform_is_true_only_for_wholesalecentral() {
        assert!(WholesalecentralScraper.matches_platform("wholesalecentral"));
        assert!(!WholesalecentralScraper.matches_platform("thomasnet"));
    }

    #[test]
    fn only_product_and_supplier_pages_are_scanned() {
        assert!(is_wholesalecentral_scan_url("/products/33417104"));
        assert!(is_wholesalecentral_scan_url("/suppliers/luxiso0001"));
        assert!(is_wholesalecentral_scan_url(
            "/suppliers/upsell0001?tab=about"
        ));
        assert!(!is_wholesalecentral_scan_url("/category/Electronics"));
        assert!(!is_wholesalecentral_scan_url("/products/"));
        assert!(!is_wholesalecentral_scan_url("/products/abc"));
        assert!(!is_wholesalecentral_scan_url("/"));
        assert!(!is_wholesalecentral_scan_url("/search?q=toys"));
    }

    #[test]
    fn hostname_accepts_www_and_bare_forms() {
        assert!(matches_wholesalecentral_hostname(
            "www.wholesalecentral.com"
        ));
        assert!(matches_wholesalecentral_hostname("wholesalecentral.com"));
        assert!(!matches_wholesalecentral_hostname("wholesalecentral.co"));
    }

    #[test]
    fn rows_follow_links_and_skip_text_rows() {
        let data = PageData::read(&supplier_page());
        let company = data.supplier().unwrap();
        assert_eq!(data.text(company, "id").as_deref(), Some("40952"));
        let contact = data.list(company, "contactInfo")[0];
        assert_eq!(
            data.text(contact, "phoneNumber").as_deref(),
            Some("916-217-3478")
        );
        // [null] is missing, not the text "null".
        assert_eq!(data.text(contact, "salesNumber"), None);
        // A text row (T) does not break the rows after it.
        let data = PageData::read(&product_page());
        assert!(data.product().is_some());
    }

    #[test]
    fn a_text_row_with_a_wrong_length_does_not_break_the_next_rows() {
        // Real pages: the stated length (5dd) was a few bytes longer than
        // the text, which swallowed the start of the next row.
        let data = PageData::read(&page(concat!(
            "2d:T5dd,{\"root\":{\"children\":[]}}",
            "2e:T722,{\"root\":{\"children\":[{\"type\":\"paragraph\",\"children\":[{\"text\":\"Hello\"}]}]}}",
            "31:{\"supplierBusinessName\":\"X\",\"contactInfo\":[],\"supplierBusinessDescription\":\"$2e\"}\n",
        )));
        let company = data.supplier().unwrap();
        let raw = data.text(company, "supplierBusinessDescription").unwrap();
        assert_eq!(description_text(&raw).as_deref(), Some("Hello"));
    }

    #[test]
    fn page_markers_are_cleaned() {
        assert_eq!(clean_text("$undefined"), None);
        assert_eq!(
            clean_text("$D2017-06-26T07:52:17.610Z").as_deref(),
            Some("2017-06-26T07:52:17.610Z")
        );
        assert_eq!(
            clean_text("$$300 Min Order").as_deref(),
            Some("$300 Min Order")
        );
        assert_eq!(
            clean_text("  South El Monte \r\n").as_deref(),
            Some("South El Monte")
        );
        assert!(is_row_link("$3e"));
        assert!(!is_row_link("$undefined"));
        assert!(!is_row_link("$D2017-06-26"));
    }

    #[test]
    fn websites_keep_only_the_site() {
        assert_eq!(
            website_from("upsellwholesale.com/?utm_source=wholesalecentral&utm_medium=directory")
                .as_deref(),
            Some("https://upsellwholesale.com")
        );
        assert_eq!(
            website_from("https://bloomwholesale.com/products/top?utm_source=wholesale_central")
                .as_deref(),
            Some("https://bloomwholesale.com")
        );
        assert_eq!(
            website_from("https://www.PrimePalletLLC.com").as_deref(),
            Some("https://www.primepalletllc.com")
        );
        assert_eq!(website_from(""), None);
        assert_eq!(website_from("not a site"), None);
    }

    #[test]
    fn minimum_order_reads_like_the_page() {
        assert_eq!(
            minimum_order_text(Some("0".into())).as_deref(),
            Some("No minimum order")
        );
        assert_eq!(
            minimum_order_text(Some("300".into())).as_deref(),
            Some("$300 minimum order")
        );
        assert_eq!(minimum_order_text(None), None);
    }

    #[test]
    fn supplier_page_gives_the_whole_supplier() {
        let s = WholesalecentralScraper.parse_supplier(&supplier_page(), "u");
        assert_eq!(s.company_name.as_deref(), Some("Bloom - USA Wholesale"));
        assert_eq!(s.contact_phone.as_deref(), Some("916-217-3478"));
        assert_eq!(s.website_url.as_deref(), Some("https://bloomwholesale.com"));
        assert_eq!(s.country.as_deref(), Some("Long Beach, CA, United States"));
        assert_eq!(
            s.year_established, None,
            "member since is not the founding year"
        );
        assert!(
            !s.platform_verified_badge,
            "Wholesale Central verifies no one"
        );
        assert_eq!(
            s.company_description.as_deref(),
            Some(
                "Address: 2850 E. 29th St., Long Beach, CA 90806, United States.\nTrusted USA based wholesaler.\nEmail on Wholesale Central: info@luxisocool.com, luxisocool@gmail.com.\n$300 minimum order."
            )
        );
    }

    #[test]
    fn supplier_page_has_no_single_product() {
        let l = WholesalecentralScraper.parse_listing(&supplier_page(), "u");
        assert_eq!(l.title, None);
        assert!(l.image_urls.is_empty());
        assert_eq!(
            WholesalecentralScraper.extract_company_profile_url(&supplier_page()),
            None
        );
    }

    #[test]
    fn product_page_gives_the_product_but_never_the_hidden_price() {
        let l = WholesalecentralScraper.parse_listing(&product_page(), "u");
        assert_eq!(l.title.as_deref(), Some("Patchwork Top"));
        assert_eq!(
            l.description.as_deref(),
            Some("Soft cotton top.\nCategory on Wholesale Central: Plus Size Clothing.")
        );
        assert_eq!(l.image_urls, vec!["https://cdn.shopify.com/top.jpg"]);
        assert_eq!(l.reference.as_deref(), Some("212-Bin72"));
        assert_eq!(
            l.minimum_order_quantity.as_deref(),
            Some("No minimum order")
        );
        assert_eq!(
            l.unit_price, None,
            "the price is hidden from visitors - never used"
        );
        assert_eq!(l.fob_price, None);
    }

    #[test]
    fn product_page_points_to_its_supplier_page() {
        assert_eq!(
            WholesalecentralScraper
                .extract_company_profile_url(&product_page())
                .as_deref(),
            Some("https://www.wholesalecentral.com/suppliers/luxiso0001")
        );
    }

    #[test]
    fn supplier_page_fills_in_the_product_pages_supplier() {
        let from_product = WholesalecentralScraper.parse_supplier(&product_page(), "u");
        assert_eq!(from_product.contact_phone, None);
        assert_eq!(from_product.country.as_deref(), Some("Long Beach, CA"));
        let s = WholesalecentralScraper.enrich_from_company_profile(from_product, &supplier_page());
        assert_eq!(s.contact_phone.as_deref(), Some("916-217-3478"));
        assert_eq!(s.country.as_deref(), Some("Long Beach, CA, United States"));
        assert!(
            s.company_description
                .unwrap()
                .starts_with("Address: 2850 E. 29th St.")
        );
        assert_eq!(s.profile_url, "u");
    }

    #[test]
    fn product_and_supplier_page_share_one_company_key() {
        assert_eq!(
            WholesalecentralScraper
                .company_key(&supplier_page())
                .as_deref(),
            Some("40952")
        );
        assert_eq!(
            WholesalecentralScraper
                .company_key(&product_page())
                .as_deref(),
            Some("40952")
        );
        assert_eq!(WholesalecentralScraper.company_key("<html></html>"), None);
    }

    #[test]
    fn record_has_years_on_wholesale_central_and_categories() {
        let r = WholesalecentralScraper
            .supplier_record(&supplier_page())
            .unwrap();
        assert_eq!(r.platform, "Wholesale Central");
        assert_eq!(r.joined_platform_year, Some(2017));
        assert!(r.years_on_platform.unwrap() >= 9);
        assert_eq!(r.products_offered, vec!["Apparel / Clothing"]);
        assert!(!r.checked_by_platform);

        let from_product = WholesalecentralScraper.supplier_record(&product_page());
        let r = WholesalecentralScraper
            .enrich_record_from_company_profile(from_product, &supplier_page())
            .unwrap();
        assert_eq!(
            r.products_offered,
            vec!["Plus Size Clothing", "Apparel / Clothing"]
        );
        assert_eq!(r.joined_platform_year, Some(2017));
    }

    #[test]
    fn full_years_counts_whole_years_only() {
        let d = |s| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        assert_eq!(full_years(d("2026-05-29"), d("2026-10-23")), 0);
        assert_eq!(full_years(d("2017-06-26"), d("2026-10-23")), 9);
        assert_eq!(full_years(d("2024-10-24"), d("2026-10-23")), 1);
    }
}
