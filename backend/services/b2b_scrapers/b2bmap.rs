use super::{B2bListingProfile, B2bScraper, B2bSupplierProfile, SupplierRecord};
use chrono::{Datelike, Utc};
use scraper::{ElementRef, Html, Selector};

pub struct B2bmapScraper;

/// Longest product description passed on to the check.
const MAX_DESCRIPTION_CHARS: usize = 3000;

/// At most this many product photos are kept (same as the other B2B
/// scrapers).
const MAX_IMAGES: usize = 3;

/// b2bmap's own pages - never a supplier's website.
const B2BMAP_HOST: &str = "b2bmap.com";

fn text_of(el: &ElementRef) -> String {
    collapse_whitespace(&el.text().collect::<Vec<_>>().join(" "))
}

fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clean_optional_text(raw: &str) -> Option<String> {
    let trimmed = collapse_whitespace(raw);
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Values b2bmap shows instead of a real value.
fn non_placeholder(value: Option<String>) -> Option<String> {
    value.filter(|v| {
        let lower = v.to_lowercase();
        !matches!(lower.as_str(), "-" | "n/a" | "not provided" | "negotiable")
    })
}

/// A number b2bmap has masked for visitors without a paid account,
/// e.g. "+848783xxxxx".
fn looks_masked(value: &str) -> bool {
    value.contains('*') || value.to_lowercase().contains('x')
}

/// Reads a value out of any of b2bmap's "label / value" tables. Covers
/// both shapes used on the site: a 2-cell "label | value" row (the
/// product summary and payment tables) and a 3-cell "label | : | value"
/// row (the Member Information table). The value is always the LAST
/// cell; a trailing ':' on the label is ignored.
fn find_table_value(document: &Html, table_selector: &str, label: &str) -> Option<String> {
    let table_sel = Selector::parse(table_selector).ok()?;
    let row_sel = Selector::parse("tr").ok()?;
    let cell_sel = Selector::parse("td").ok()?;

    for table in document.select(&table_sel) {
        for row in table.select(&row_sel) {
            let cells: Vec<_> = row.select(&cell_sel).collect();
            if cells.len() < 2 {
                continue;
            }
            let row_label = text_of(&cells[0]).trim_end_matches(':').trim().to_string();
            if row_label.eq_ignore_ascii_case(label) {
                return clean_optional_text(&text_of(cells.last().unwrap()));
            }
        }
    }
    None
}

/// Every "label: value" row of a table, in page order (labels without
/// the trailing ':').
fn table_pairs(document: &Html, table_selector: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let (Ok(table_sel), Ok(row_sel), Ok(cell_sel)) = (
        Selector::parse(table_selector),
        Selector::parse("tr"),
        Selector::parse("td"),
    ) else {
        return pairs;
    };
    for table in document.select(&table_sel) {
        for row in table.select(&row_sel) {
            let cells: Vec<_> = row.select(&cell_sel).collect();
            if cells.len() < 2 {
                continue;
            }
            let label = text_of(&cells[0]).trim_end_matches(':').trim().to_string();
            let value = text_of(cells.last().unwrap());
            if !label.is_empty() && !value.is_empty() {
                pairs.push((label, value));
            }
        }
    }
    pairs
}

/// Reads a value out of the Company Overview section on the company
/// page - a <div> grid rather than a <table>.
fn find_overview_field(document: &Html, label: &str) -> Option<String> {
    let row_sel = Selector::parse("div.d-md-table-row").ok()?;
    let cell_sel = Selector::parse("div.d-md-table-cell").ok()?;

    for row in document.select(&row_sel) {
        let cells: Vec<_> = row.select(&cell_sel).collect();
        if cells.len() < 2 {
            continue;
        }
        let row_label = text_of(&cells[0]).trim_end_matches(':').trim().to_string();
        if row_label.eq_ignore_ascii_case(label) {
            return clean_optional_text(&text_of(&cells[1]));
        }
    }
    None
}

/// The supplier-name link in the product page's sidebar (e.g. "Loyal
/// Vina Co., Ltd" linking to b2bmap.com/loyal-vina) - both the company
/// name AND the company page address are read from it.
fn find_supplier_name_link(document: &Html) -> Option<ElementRef<'_>> {
    let sel = Selector::parse("h4.text-18.text-lg-22 a").ok()?;
    document.select(&sel).next()
}

/// "https://b2bmap.com/loyal-vina" (or ".../loyal-vina/contact-info")
/// -> "loyal-vina". None for b2bmap's own sections ("/products/...").
fn company_slug(url: &str) -> Option<String> {
    let after_host = url.split(B2BMAP_HOST).nth(1)?;
    let path = after_host.split(['?', '#']).next()?;
    let slug = path
        .split('/')
        .find(|s| !s.is_empty())?
        .trim()
        .to_lowercase();
    let reserved = [
        "products",
        "product-list",
        "companies",
        "company-list",
        "buyleads",
        "business-directory",
        "pricing",
        "myzone",
        "contact-info",
    ];
    if slug.is_empty() || reserved.contains(&slug.as_str()) {
        None
    } else {
        Some(slug)
    }
}

/// The phone shown in the product page's sidebar. b2bmap masks the
/// desktop copy for visitors without a paid account ("+9203009xxxxx")
/// but often shows the full number in the page's mobile copy of the
/// same box. The full number is used only when it starts with the
/// same visible digits as the masked one, so a different number on
/// the page is never taken by mistake.
fn sidebar_phone(document: &Html) -> Option<String> {
    let desktop = Selector::parse(
        ".col-lg-4.col-xl-3.d-lg-down-none span.d-flex.mb-3.align-items-center span.text-muted",
    )
    .ok()
    .and_then(|s| document.select(&s).next())
    .and_then(|el| clean_optional_text(&text_of(&el)))?;
    if !looks_masked(&desktop) {
        return Some(desktop);
    }
    let visible: String = desktop
        .chars()
        .take_while(|c| !matches!(c, 'x' | 'X' | '*'))
        .collect();
    let visible = visible.trim();
    if visible.chars().filter(|c| c.is_ascii_digit()).count() < 4 {
        return None;
    }
    let all = Selector::parse("span.d-flex.mb-3.align-items-center span.text-muted").ok()?;
    document
        .select(&all)
        .filter_map(|el| clean_optional_text(&text_of(&el)))
        .find(|p| !looks_masked(p) && p.starts_with(visible) && p.len() > visible.len())
}

/// Delivery terms b2bmap suppliers write in free text ("international
/// sea freight (FOB / CIF)").
const INCOTERMS: &[&str] = &[
    "EXW", "FCA", "FAS", "FOB", "CFR", "CNF", "CIF", "CPT", "CIP", "DAP", "DPU", "DDP", "DAT",
    "DDU",
];

/// The Incoterms named in the given texts, each once, in page order
/// ("FOB, CIF"). Only whole upper-case words count, so "fob" inside a
/// longer word never matches.
fn incoterms_in(texts: &[Option<&str>]) -> Option<String> {
    let mut found: Vec<&str> = Vec::new();
    for text in texts.iter().flatten() {
        for word in text.split(|c: char| !c.is_ascii_alphanumeric()) {
            if let Some(term) = INCOTERMS.iter().find(|t| **t == word) {
                if !found.contains(term) {
                    found.push(term);
                }
            }
        }
    }
    (!found.is_empty()).then(|| found.join(", "))
}

/// "Production Capacity: 8,000 to 11,000 Blocks per shift" from the
/// product text's bullet points or its specification table.
fn production_capacity(document: &Html) -> Option<String> {
    let label = "production capacity";
    if let Ok(sel) = Selector::parse(".product-details-content li, .product-details-content p") {
        for el in document.select(&sel) {
            let text = text_of(&el);
            if text.to_lowercase().starts_with(label) {
                let value = text[label.len()..].trim_start_matches([':', ' ']).trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    table_pairs(document, "table.specification-table")
        .into_iter()
        .find(|(l, _)| l.trim().eq_ignore_ascii_case(label))
        .map(|(_, v)| v)
}

/// The city and region from the company page's structured data
/// ("Lahore", "Punjab", "54000"), which the visible "Register Address"
/// row often leaves out.
fn structured_address_parts(document: &Html) -> Vec<String> {
    let Ok(sel) = Selector::parse(r#"script[type="application/ld+json"]"#) else {
        return Vec::new();
    };
    for script in document.select(&sel) {
        let raw = script.text().collect::<String>();
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let Some(address) = json.get("address") else {
            continue;
        };
        return ["addressLocality", "addressRegion", "postalCode"]
            .iter()
            .filter_map(|k| address.get(*k).and_then(|v| v.as_str()))
            .map(collapse_whitespace)
            .filter(|v| !v.is_empty())
            .collect();
    }
    Vec::new()
}

/// "24 Sep 2026" -> 2026.
fn year_from_date(text: &str) -> Option<i32> {
    text.split_whitespace()
        .last()?
        .parse::<i32>()
        .ok()
        .filter(|y| (1990..=2100).contains(y))
}

/// "Mr. Minh Trung  (Sale Manager)" -> ("Mr. Minh Trung", Some("Sale Manager")).
fn split_name_and_role(raw: &str) -> (String, Option<String>) {
    let cleaned = collapse_whitespace(raw);
    if let (Some(open), true) = (cleaned.find('('), cleaned.ends_with(')')) {
        let name = cleaned[..open].trim().to_string();
        let role = cleaned[open + 1..cleaned.len() - 1].trim().to_string();
        let role = if role.is_empty() { None } else { Some(role) };
        return (name, role);
    }
    (cleaned, None)
}

/// A real web address, not "Show", not an email, not a b2bmap page.
fn looks_like_website(value: &str) -> bool {
    let v = value.trim().to_lowercase();
    !v.contains(' ')
        && !v.contains('@')
        && v.contains('.')
        && !v.contains(B2BMAP_HOST)
        && v.len() > 4
}

/// The list items of the first "Business Type" list in the product
/// page's sidebar ("Supplier, Exporter").
fn sidebar_business_type(document: &Html) -> Option<String> {
    let ul_sel =
        Selector::parse(".col-lg-4.col-xl-3.d-lg-down-none ul.d-flex.flex-wrap.pl-3").ok()?;
    let li_sel = Selector::parse("li").ok()?;
    let ul = document.select(&ul_sel).next()?;
    let types: Vec<String> = ul
        .select(&li_sel)
        .map(|li| text_of(&li))
        .filter(|t| !t.is_empty())
        .collect();
    if types.is_empty() {
        None
    } else {
        Some(types.join(", "))
    }
}

impl B2bScraper for B2bmapScraper {
    fn matches_platform(&self, platform: &str) -> bool {
        platform == "b2bmap"
    }

    fn parse_supplier(&self, html: &str, profile_url: &str) -> B2bSupplierProfile {
        let document = Html::parse_document(html);

        let company_name = find_supplier_name_link(&document).map(|el| text_of(&el));

        // "Can Tho, Vietnam" - the line right under the company name in
        // the desktop sidebar.
        let country = Selector::parse(".col-lg-4.col-xl-3.d-lg-down-none p.text-muted.mb-2")
            .ok()
            .and_then(|s| document.select(&s).next())
            .and_then(|el| clean_optional_text(&text_of(&el)));

        // "Year Established: 2025" - strip the label, keep the year.
        let year_established = Selector::parse(".col-lg-4.col-xl-3.d-lg-down-none p.mb-2")
            .ok()
            .and_then(|s| {
                document
                    .select(&s)
                    .find(|el| text_of(el).contains("Year Established"))
            })
            .map(|el| text_of(&el))
            .and_then(|t| t.split(':').nth(1).map(|s| s.trim().to_string()))
            .filter(|t| !t.is_empty());

        // The phone in the product page's sidebar (see sidebar_phone for
        // how a masked number is handled).
        let contact_phone = sidebar_phone(&document);

        // Business type ("Supplier, Exporter") is what the company is,
        // not a badge - it is passed on in the description instead.
        let company_description =
            sidebar_business_type(&document).map(|t| format!("Business type: {}.", t));

        B2bSupplierProfile {
            company_name,
            logo_url: None,
            year_established,
            country,
            // b2bmap's membership levels (Free / paid) are not a check on
            // the company, so this is never set.
            platform_verified_badge: false,
            employee_count: None,
            sales_revenue: None,
            export_percentage: None,
            profile_url: profile_url.to_string(),
            source_platform: "b2bmap".to_string(),
            contact_name: None,
            contact_phone,
            badge_honorific: None,
            company_description,
            website_url: None,
        }
    }

    fn extract_company_profile_url(&self, listing_html: &str) -> Option<String> {
        let document = Html::parse_document(listing_html);
        let href = find_supplier_name_link(&document)?
            .value()
            .attr("href")?
            .trim()
            .to_string();
        let absolute = if href.starts_with("http") {
            href
        } else {
            format!("https://{}{}", B2BMAP_HOST, href)
        };
        // Keep only "https://b2bmap.com/{company}".
        company_slug(&absolute).map(|slug| format!("https://{}/{}", B2BMAP_HOST, slug))
    }

    /// One seller record per company: the company page's slug, e.g.
    /// "https://b2bmap.com/loyal-vina" -> "loyal-vina". Without this,
    /// every b2bmap seller landed on one shared "unknown" record.
    fn company_key(&self, listing_html: &str) -> Option<String> {
        company_slug(&self.extract_company_profile_url(listing_html)?)
    }

    fn enrich_from_company_profile(
        &self,
        mut supplier: B2bSupplierProfile,
        profile_html: &str,
    ) -> B2bSupplierProfile {
        let document = Html::parse_document(profile_html);

        // The full "About" text - the product page only shows a short,
        // cut-off preview.
        let about = Selector::parse(".clean-link")
            .ok()
            .and_then(|sel| document.select(&sel).next())
            .and_then(|el| clean_optional_text(&text_of(&el)));

        supplier.employee_count =
            non_placeholder(find_table_value(&document, "table", "Employees"))
                .or(supplier.employee_count);
        if supplier.year_established.is_none() {
            supplier.year_established =
                non_placeholder(find_table_value(&document, "table", "Founded in"));
        }

        // Contact person, e.g. "Mr. Minh Trung (Sale Manager)" - the name
        // and the job title are kept apart.
        let mut role = None;
        if let Some(el) = Selector::parse("a.text-14.text-muted")
            .ok()
            .and_then(|sel| document.select(&sel).next())
        {
            let (name, r) = split_name_and_role(&text_of(&el));
            if !name.is_empty() {
                supplier.contact_name = Some(name);
            }
            role = r;
        }

        // Membership level ("Free Member", or a paid level). Shown for
        // information only - it is a paid plan, not a check on the
        // company, so it never counts as verification.
        if let Some(membership) =
            non_placeholder(find_table_value(&document, "table", "Membership Type"))
        {
            supplier.badge_honorific = Some(membership);
        }
        supplier.platform_verified_badge = false;

        // Website, when the company page shows one (never a b2bmap
        // page, never a masked or "Show" value).
        if supplier.website_url.is_none() {
            supplier.website_url = ["Website", "Web Site", "Company Website"]
                .iter()
                .find_map(|label| find_overview_field(&document, label))
                .filter(|w| looks_like_website(w));
        }

        if supplier.logo_url.is_none() {
            supplier.logo_url = Selector::parse(".company-navbar-brand img")
                .ok()
                .and_then(|sel| document.select(&sel).next())
                .and_then(|img| img.value().attr("src").map(|s| s.to_string()))
                .filter(|s| s.starts_with("http"));
        }

        // Fallback fill only - the product page's "Can Tho, Vietnam" is
        // more specific than this page's bare "Vietnam".
        if supplier.country.is_none() {
            supplier.country = find_overview_field(&document, "Country");
        }
        if supplier.company_name.is_none() {
            supplier.company_name = find_overview_field(&document, "Company Name");
        }

        // The Contact Number / Whatsapp here are masked ("+848783xxxxx")
        // without a paid account. Used only if the product page gave no
        // phone and this one is not masked.
        if supplier.contact_phone.is_none() {
            supplier.contact_phone =
                find_overview_field(&document, "Contact Number").filter(|p| !looks_masked(p));
        }

        // What Claude needs to judge the company: what it is, what it
        // sells, who the contact is, how long it has been on b2bmap, and
        // its own description.
        let mut parts: Vec<String> = Vec::new();
        let business_type = find_overview_field(&document, "Business Type");
        match (&supplier.company_description, business_type) {
            (Some(existing), _) => parts.push(existing.clone()),
            (None, Some(t)) => parts.push(format!("Business type: {}.", t)),
            (None, None) => {}
        }
        if let Some(v) = find_overview_field(&document, "Main Products") {
            parts.push(format!("Main products: {}.", v));
        }
        if let Some(r) = role {
            parts.push(format!("Contact person's role: {}.", r));
        }
        if let Some(v) = find_table_value(&document, "table", "Member Since") {
            parts.push(format!("Member of b2bmap since: {}.", v));
        }
        if let Some(v) = find_overview_field(&document, "Register Address") {
            // Add the city, region and postcode when the address row
            // leaves them out ("...main multan road" -> "..., Lahore,
            // Punjab, 54000").
            let lower = v.to_lowercase();
            let mut address = vec![v.clone()];
            address.extend(
                structured_address_parts(&document)
                    .into_iter()
                    .filter(|p| !lower.contains(&p.to_lowercase())),
            );
            parts.push(format!("Registered address: {}.", address.join(", ")));
        }
        if let Some(a) = about {
            parts.push(a);
        }
        if !parts.is_empty() {
            supplier.company_description = Some(cap_chars(&parts.join(" "), MAX_DESCRIPTION_CHARS));
        }

        supplier
    }

    /// When the company joined b2bmap ("Member Since: 24 Sep 2026"). A
    /// company that says it is decades old but joined only recently gets
    /// a note on its Account age card (see apply_supplier_record).
    fn enrich_record_from_company_profile(
        &self,
        record: Option<SupplierRecord>,
        profile_html: &str,
    ) -> Option<SupplierRecord> {
        let document = Html::parse_document(profile_html);
        let Some(joined) =
            find_table_value(&document, "table", "Member Since").and_then(|v| year_from_date(&v))
        else {
            return record;
        };
        let mut record = record.unwrap_or_else(|| SupplierRecord {
            platform: "b2bmap".to_string(),
            ..Default::default()
        });
        record.joined_platform_year = Some(joined);
        record.years_on_platform = u32::try_from(Utc::now().year() - joined).ok();
        Some(record)
    }

    fn parse_listing(&self, html: &str, listing_url: &str) -> B2bListingProfile {
        let document = Html::parse_document(html);

        let title = Selector::parse("h1.text-18.text-md-26.text-strong")
            .ok()
            .and_then(|s| document.select(&s).next())
            .and_then(|el| clean_optional_text(&text_of(&el)));

        let summary = "table.product-summery-table";
        // "Negotiable" is not a price - it is noted in the description
        // instead, so it doesn't count as a filled-in price.
        let raw_price = find_table_value(&document, summary, "Price");
        let unit_price = non_placeholder(raw_price.clone());
        let minimum_order_quantity = non_placeholder(find_table_value(&document, summary, "MOQ"));
        let reference = non_placeholder(find_table_value(&document, summary, "HS Code"));

        let details = "div.table-responsive.mt-4 table";
        let payment_type = non_placeholder(find_table_value(&document, details, "Payment Terms"));
        let packaging_details =
            non_placeholder(find_table_value(&document, details, "Packaging Info"));
        let delivery_timeframe =
            non_placeholder(find_table_value(&document, details, "Delivery Info"));

        let description = extract_description(&document, raw_price.as_deref());
        // "FOB / CIF" written in the delivery text or the description.
        let incoterms = incoterms_in(&[delivery_timeframe.as_deref(), description.as_deref()]);

        B2bListingProfile {
            title,
            description,
            image_urls: extract_image_urls(&document),
            unit_price,
            fob_price: None,
            minimum_order_quantity,
            payment_type,
            preferred_port: None,
            reference,
            production_capacity: production_capacity(&document),
            delivery_timeframe,
            incoterms,
            packaging_details,
            listing_url: listing_url.to_string(),
            source_platform: "b2bmap".to_string(),
        }
    }
}

/// The product text (paragraphs AND bullet points - the specification
/// is usually a bullet list), then the Product Specification table
/// ("Model Number: LV-DRAGON-001"), the usages, and "Price: Negotiable"
/// when no price is given.
fn extract_description(document: &Html, raw_price: Option<&str>) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();

    if let Ok(sel) = Selector::parse(".product-details-content p, .product-details-content li") {
        for el in document.select(&sel) {
            let text = text_of(&el);
            if !text.is_empty() {
                parts.push(text);
            }
        }
    }

    let specs: Vec<String> = table_pairs(document, "table.specification-table")
        .into_iter()
        .map(|(l, v)| format!("{}: {}", l, v))
        .collect();
    if !specs.is_empty() {
        parts.push(format!("Specification: {}", specs.join("; ")));
    }

    if let Some(usage) = Selector::parse(".product-usage")
        .ok()
        .and_then(|s| document.select(&s).next())
        .and_then(|el| clean_optional_text(&text_of(&el)))
    {
        parts.push(format!("Usage: {}", usage));
    }

    if let Some(p) = raw_price.filter(|p| p.eq_ignore_ascii_case("negotiable")) {
        parts.push(format!("Price: {}", p));
    }

    if parts.is_empty() {
        None
    } else {
        Some(cap_chars(&parts.join("\n"), MAX_DESCRIPTION_CHARS))
    }
}

/// The product photos (no duplicates, at most MAX_IMAGES).
fn extract_image_urls(document: &Html) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();
    if let Ok(sel) = Selector::parse("#viewProductImages img") {
        for img in document.select(&sel) {
            let src = img
                .value()
                .attr("src")
                .or_else(|| img.value().attr("data-src"))
                .unwrap_or("")
                .trim();
            if src.starts_with("http") && !urls.iter().any(|u| u == src) && urls.len() < MAX_IMAGES
            {
                urls.push(src.to_string());
            }
        }
    }
    urls
}

fn cap_chars(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        s.chars().take(max).collect()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from the real Loyal Vina dragon fruit product page (Oct 2026).
    const LISTING: &str = r##"<html><body>
<div class="col-lg-8 col-xl-9">
  <h1 class="text-18 text-md-26 text-strong mb-3 d-lg-none">Fresh Vietnamese Dragon Fruit - Red and White Flesh, 350–400 g, Export Grade for Wholesale</h1>
  <table class="table table-sm table-bordered product-summery-table"><tbody>
    <tr><td>Brand</td><td>Loyal Vina</td></tr>
    <tr><td>Country of Origin</td><td>Vietnam </td></tr>
    <tr><td>MOQ</td><td class="min_order_unit"> 1 Tons </td></tr>
    <tr><td>Price</td><td class="price_info">Negotiable</td></tr>
    <tr><td>HS Code</td><td class="price_info">081090</td></tr>
  </tbody></table>
</div>
<div class="col-lg-4 col-xl-3 d-lg-down-none">
  <h4 class="text-18 text-lg-22">
    <a href="https://b2bmap.com/loyal-vina" class="d-block text-strong">Loyal Vina Co., Ltd</a>
  </h4>
  <p class="text-muted mb-2">
    <img src="https://b2bmap.com/public/flags/16x16/vn.png" alt="Vietnam" class="mr-1">
    Can Tho, Vietnam
  </p>
  <p class="mb-2"><span>Year Established:</span> 2025
  </p>
  <div class="mb-2">
    <p class="text-strong text-strong mb-1">Business Type:</p>
    <ul class="d-flex flex-wrap pl-3">
      <li class="text-muted mr-4 mb-2">Supplier</li>
      <li class="text-muted mr-4 mb-2">Exporter</li>
    </ul>
  </div>
  <div class="mb-3">
    <span class="d-flex mb-3 align-items-center">
      <span class="box-30 border rounded-circle bg-light-white mr-2"><i class="fa fa-phone mr-2 text-13"></i></span>
      <span class="text-muted">
          +84878369911
      </span>
    </span>
  </div>
</div>
<div class="mb-2 product-details-content" style="word-break: break-word">
  <p>Fresh Vietnamese Dragon Fruit supplied by Loyal Vina for international wholesale and export markets.</p><p><b>Specification</b></p><ul><li>Product Origin: Vietnam</li><li>Fruit Weight: 350–400 g/fruit</li><li>Shelf Life: 21–30 days</li></ul>
</div>
<div class="table-responsive mt-4">
  <table class="table table-sm table-bordered text-14"><tbody>
    <tr><td class="text-theme text-nowrap">Payment Terms:</td><td class="text-strong">LC,T/T</td></tr>
    <tr><td class="text-theme text-nowrap">Packaging Info:</td><td class="text-strong">Individual net bag, carton or customized according to buyer requirements.</td></tr>
    <tr><td class="text-theme text-nowrap">Delivery Info:</td><td class="text-strong">Delivery schedule arranged according to buyer requirements.</td></tr>
  </tbody></table>
</div>
<table class="table table-sm table-borderless specification-table"><tbody>
  <tr><td class="text-theme pl-0">Model Number :</td><td class="text-strong"> LV-DRAGON-001 </td></tr>
  <tr><td class="text-theme pl-0">Grade :</td><td class="text-strong"> Export Grade </td></tr>
</tbody></table>
<div class="product-usage line-height-180">
  Fresh fruit consumption, retail, wholesale distribution.
</div>
<div class="bg-lg-white border-lg mb-lg-4" id="viewProductImages"><ul>
  <li><img src="https://b2bmap.com/product-image/202609/fresh-vietnamese-dragon-fruit-08443.png" class="img-fluid"></li>
  <li><img src="https://b2bmap.com/product-image/202609/fresh-vietnamese-dragon-fruit-19902.jpg" class="img-fluid"></li>
  <li><img src="https://b2bmap.com/product-image/202609/fresh-vietnamese-dragon-fruit-29924.png" class="img-fluid"></li>
  <li><img src="https://b2bmap.com/product-image/202609/fresh-vietnamese-dragon-fruit-4.png" class="img-fluid"></li>
</ul></div>
<div class="d-lg-none mb-3">
  <h4 class="text-18 text-lg-22"><a href="https://b2bmap.com/loyal-vina" class="d-block text-strong">Loyal Vina Co., Ltd</a></h4>
  <ul class="d-flex flex-wrap pl-3"><li class="text-muted mr-4 mb-2">WRONG_TYPE</li></ul>
  <span class="d-flex mb-3 align-items-center"><span class="text-muted">+19999999999</span></span>
</div>
</body></html>"##;

    // Trimmed from the real Loyal Vina company page.
    const PROFILE: &str = r##"<html><body>
<div class="company-header bg-white py-3">
  <a href="https://b2bmap.com/loyal-vina" class="company-navbar-brand ">
    <img src="https://b2bmap.com/public/uploads/companylogo/2026/loyal-vina-co-ltd-1786331571.png" alt="Loyal Vina Co., Ltd" class="img-fluid">
  </a>
</div>
<div class="border-left border-4px border-business-secondary-close px-3 py-2">
  <a href="https://b2bmap.com/loyal-vina" class="d-block text-strong">Loyal Vina Co., Ltd</a>
  <a href="https://b2bmap.com/loyal-vina/contact-info" class="text-14 text-muted"> Mr. Minh Trung  (Sale Manager)  <i class="fa fa-envelope ml-1"></i></a>
</div>
<div class="clean-link">
  <p>From Vietnamese origin to global opportunity,     Loyal Vina Co.,     Ltd was formed from the desire to bring Vietnamese agricultural value closer to international markets.</p>
</div>
<table class="table table-sm table-borderless w-auto mb-0"><tbody>
  <tr><td >Business Type</td><td class="px-2">:</td><td><ul class="list-inline mb-0"><li class="list-inline-item">Supplier</li><li class="list-inline-item">Exporter</li></ul></td></tr>
  <tr><td >Founded in</td><td class="px-2 ">:</td><td >2025</td></tr>
  <tr><td >Employees</td><td class="px-2 ">:</td><td >1-5</td></tr>
</tbody></table>
<table class="table table-sm table-borderless w-auto mb-0"><tbody>
  <tr><td >Member Since</td><td class="px-2 text-muted">:</td><td >10 Aug 2026</td></tr>
  <tr><td >Membership Type</td><td class="px-2 text-muted">:</td><td >
      Free Member
  </td></tr>
</tbody></table>
<div class="d-md-table d-company-info-table w-100">
  <div class="d-flex d-md-table-row">
    <div class="d-md-table-cell"><span class="d-md-down-none text-nowrap">Company Name:</span></div>
    <div class="d-md-table-cell"><span class="d-inline-block">Loyal Vina Co., Ltd</span></div>
  </div>
  <div class="d-flex d-md-table-row">
    <div class="d-md-table-cell"><span class="d-md-down-none text-nowrap">Business Type:</span></div>
    <div class="d-md-table-cell"><ul class="list-inline mb-0"><li>Supplier</li><li>Exporter</li></ul></div>
  </div>
  <div class="d-md-table-row">
    <div class="d-md-table-cell text-nowrap pb-0 pb-md-2">Main Products:</div>
    <div class="d-md-table-cell pt-1 pt-md-2">Fresh Seedless Lime, Fresh Pomelo, Fresh Coconut, Hoa Loc Mango, Fresh Tropical Fruits</div>
  </div>
  <div class="d-flex d-md-table-row">
    <div class="d-md-table-cell"><span class="d-md-down-none">Product List:</span></div>
    <div class="d-md-table-cell"><a href="https://b2bmap.com/loyal-vina/products" class="text-link">https://b2bmap.com/loyal-vina/products</a></div>
  </div>
  <div class="d-flex d-md-table-row">
    <div class="d-md-table-cell"><span class="d-md-down-none text-nowrap">Contact Number:</span></div>
    <div class="d-md-table-cell"><span data-toggle="modal" class="cursor">+848783xxxxx</span></div>
  </div>
  <div class="d-flex d-md-table-row">
    <div class="d-md-table-cell"><span class="d-md-down-none text-nowrap">Register Address:</span></div>
    <div class="d-md-table-cell">372 Tra Quyt  Market, Phu Tam Commune, Can Tho City</div>
  </div>
  <div class="d-flex d-md-table-row">
    <div class="d-md-table-cell text-nowrap"><span>Country:</span></div>
    <div class="d-md-table-cell"><a href="https://b2bmap.com/vietnam">Vietnam</a></div>
  </div>
</div>
</body></html>"##;

    fn full_supplier() -> B2bSupplierProfile {
        let s = B2bmapScraper;
        s.enrich_from_company_profile(s.parse_supplier(LISTING, "u"), PROFILE)
    }

    #[test]
    fn matches_platform_is_true_only_for_b2bmap() {
        assert!(B2bmapScraper.matches_platform("b2bmap"));
        assert!(!B2bmapScraper.matches_platform("thomasnet"));
    }

    #[test]
    fn company_key_and_link_are_the_company_page() {
        assert_eq!(
            B2bmapScraper.company_key(LISTING).as_deref(),
            Some("loyal-vina")
        );
        assert_eq!(
            B2bmapScraper
                .extract_company_profile_url(LISTING)
                .as_deref(),
            Some("https://b2bmap.com/loyal-vina")
        );
        assert_eq!(
            company_slug("https://b2bmap.com/loyal-vina/contact-info").as_deref(),
            Some("loyal-vina")
        );
        assert_eq!(
            company_slug("https://b2bmap.com/products/fresh-vietnamese-dragon-fruit"),
            None
        );
        let relative = r#"<h4 class="text-18 text-lg-22"><a href="/loyal-vina">X</a></h4>"#;
        assert_eq!(
            B2bmapScraper.company_key(relative).as_deref(),
            Some("loyal-vina")
        );
        assert_eq!(B2bmapScraper.company_key("<html></html>"), None);
    }

    #[test]
    fn listing_page_supplier_fields() {
        let s = B2bmapScraper.parse_supplier(LISTING, "u");
        assert_eq!(s.company_name.as_deref(), Some("Loyal Vina Co., Ltd"));
        assert_eq!(s.country.as_deref(), Some("Can Tho, Vietnam"));
        assert_eq!(s.year_established.as_deref(), Some("2025"));
        assert_eq!(
            s.contact_phone.as_deref(),
            Some("+84878369911"),
            "desktop sidebar, not the mobile copy"
        );
        assert_eq!(s.badge_honorific, None, "business type is not a badge");
        assert_eq!(
            s.company_description.as_deref(),
            Some("Business type: Supplier, Exporter.")
        );
        assert!(!s.platform_verified_badge);
    }

    #[test]
    fn company_page_fields() {
        let s = full_supplier();
        assert_eq!(
            s.contact_name.as_deref(),
            Some("Mr. Minh Trung"),
            "job title kept out of the name"
        );
        assert_eq!(s.employee_count.as_deref(), Some("1-5"));
        assert_eq!(s.badge_honorific.as_deref(), Some("Free Member"));
        assert!(!s.platform_verified_badge);
        assert_eq!(
            s.contact_phone.as_deref(),
            Some("+84878369911"),
            "masked number never replaces the real one"
        );
        assert_eq!(s.country.as_deref(), Some("Can Tho, Vietnam"));
        assert!(s.logo_url.as_deref().unwrap().contains("/companylogo/"));
        assert_eq!(s.website_url, None, "this company shows no website");
        let d = s.company_description.unwrap();
        assert!(d.starts_with("Business type: Supplier, Exporter."));
        assert!(d.contains("Main products: Fresh Seedless Lime"));
        assert!(d.contains("Contact person's role: Sale Manager."));
        assert!(d.contains("Member of b2bmap since: 10 Aug 2026."));
        assert!(d.contains("Registered address: 372 Tra Quyt Market"));
        assert!(
            d.contains("Loyal Vina Co., Ltd was formed"),
            "extra spaces collapsed: {}",
            d
        );
    }

    #[test]
    fn paid_membership_is_never_verification() {
        let paid = PROFILE.replace("Free Member", "Gold Member");
        let s = B2bmapScraper
            .enrich_from_company_profile(B2bmapScraper.parse_supplier(LISTING, "u"), &paid);
        assert_eq!(s.badge_honorific.as_deref(), Some("Gold Member"));
        assert!(!s.platform_verified_badge);
    }

    #[test]
    fn website_is_read_when_the_company_page_shows_one() {
        let row = r#"<div class="d-flex d-md-table-row"><div class="d-md-table-cell">Website:</div><div class="d-md-table-cell"><a href="https://loyalvina.com">https://loyalvina.com</a></div></div>"#;
        let with_site = PROFILE.replace(
            r#"<div class="d-md-table d-company-info-table w-100">"#,
            &format!(
                r#"<div class="d-md-table d-company-info-table w-100">{}"#,
                row
            ),
        );
        let s =
            B2bmapScraper.enrich_from_company_profile(B2bSupplierProfile::default(), &with_site);
        assert_eq!(s.website_url.as_deref(), Some("https://loyalvina.com"));

        let b2bmap_link = with_site.replace(
            "https://loyalvina.com",
            "https://b2bmap.com/loyal-vina/products",
        );
        let s =
            B2bmapScraper.enrich_from_company_profile(B2bSupplierProfile::default(), &b2bmap_link);
        assert_eq!(
            s.website_url, None,
            "a b2bmap page is not the company's website"
        );
    }

    #[test]
    fn listing_fields() {
        let l = B2bmapScraper.parse_listing(LISTING, "u");
        assert_eq!(
            l.title.as_deref(),
            Some(
                "Fresh Vietnamese Dragon Fruit - Red and White Flesh, 350–400 g, Export Grade for Wholesale"
            )
        );
        assert_eq!(l.unit_price, None, "'Negotiable' is not a price");
        assert_eq!(l.minimum_order_quantity.as_deref(), Some("1 Tons"));
        assert_eq!(l.reference.as_deref(), Some("081090"));
        assert_eq!(l.payment_type.as_deref(), Some("LC,T/T"));
        assert!(
            l.packaging_details
                .as_deref()
                .unwrap()
                .starts_with("Individual net bag")
        );
        assert!(l.delivery_timeframe.is_some());
        assert_eq!(l.image_urls.len(), 3);
        assert!(l.image_urls[0].ends_with("08443.png"));
    }

    #[test]
    fn description_has_text_bullets_specs_usage_and_negotiable_price() {
        let d = B2bmapScraper
            .parse_listing(LISTING, "u")
            .description
            .unwrap();
        assert!(d.contains("supplied by Loyal Vina"));
        assert!(
            d.contains("Shelf Life: 21–30 days"),
            "bullet points are read: {}",
            d
        );
        assert!(d.contains("Specification: Model Number: LV-DRAGON-001; Grade: Export Grade"));
        assert!(d.contains("Usage: Fresh fruit consumption"));
        assert!(d.contains("Price: Negotiable"));
    }

    /// Trimmed from the real Golden Steel Mills product page: the
    /// desktop sidebar masks the phone, the mobile copy shows it.
    const GOLDEN_LISTING: &str = r##"<html><body>
<div class="col-lg-4 col-xl-3 d-lg-down-none">
  <h4 class="text-18 text-lg-22"><a href="https://b2bmap.com/golden-steel-mills" class="d-block text-strong">Golden Steel Mills</a></h4>
  <span class="d-flex mb-3 align-items-center">
    <span class="box-30 border rounded-circle bg-light-white mr-2"><i class="fa fa-phone mr-2 text-13"></i></span>
    <span class="text-muted">
      <span data-toggle="modal" data-target="#popupLoginFormModal" class="cursor">+9203009xxxxx</span>
    </span>
  </span>
  <div class="table-responsive mt-4"><table class="table table-sm table-bordered text-14"><tbody>
    <tr><td class="text-theme text-nowrap">Payment Terms:</td><td class="text-strong">L/C, T/T</td></tr>
    <tr><td class="text-theme text-nowrap">Delivery Info:</td><td class="text-strong">Dispatched within 30 to 45 days after order confirmation. Available for nationwide delivery in Pakistan and international sea freight (FOB / CIF).</td></tr>
  </tbody></table></div>
</div>
<div class="mb-2 product-details-content"><p>The GSM-50 is a stationary hydraulic plant.</p><ul><li>Production Capacity: 50 Paver Tiles / 8,000 to 11,000 Blocks per shift<br><br></li><li>Hydraulic Pressure: 21 - 25 MPa</li></ul></div>
<div class="d-lg-none mb-3">
  <h4 class="text-18 text-lg-22"><a href="https://b2bmap.com/golden-steel-mills" class="d-block text-strong">Golden Steel Mills</a></h4>
  <span class="d-flex mb-3 align-items-center">
    <span class="box-30 border rounded-circle bg-light-white mr-2"><i class="fa fa-phone mr-2 text-13"></i></span>
    <span class="text-muted">+9203009436019</span>
  </span>
</div>
</body></html>"##;

    const GOLDEN_PROFILE: &str = r##"<html><body>
<table class="table table-sm table-borderless w-auto mb-0"><tbody>
  <tr><td >Founded in</td><td class="px-2 ">:</td><td >1989</td></tr>
</tbody></table>
<table class="table table-sm table-borderless w-auto mb-0"><tbody>
  <tr><td >Member Since</td><td class="px-2 text-muted">:</td><td >24 Sep 2026</td></tr>
  <tr><td >Membership Type</td><td class="px-2 text-muted">:</td><td > Free Member </td></tr>
</tbody></table>
<div class="d-md-table d-company-info-table w-100">
  <div class="d-flex d-md-table-row">
    <div class="d-md-table-cell"><span class="d-md-down-none text-nowrap">Register Address:</span></div>
    <div class="d-md-table-cell">15 Km Near Ring road interchange main multan road</div>
  </div>
</div>
<script type="application/ld+json">
  { "@context": "https://schema.org/", "@type": "LocalBusiness", "name": "Golden Steel Mills",
    "address": { "@type": "PostalAddress", "streetAddress": "15 Km Near Ring road interchange main multan road",
      "addressLocality": "Lahore", "addressRegion": "Punjab", "postalCode": "54000", "addressCountry": "Pakistan" } }
</script>
</body></html>"##;

    #[test]
    fn masked_sidebar_phone_uses_the_matching_full_number() {
        let s = B2bmapScraper.parse_supplier(GOLDEN_LISTING, "u");
        assert_eq!(s.contact_phone.as_deref(), Some("+9203009436019"));

        // A full number that does not match the visible digits is never used.
        let other = GOLDEN_LISTING.replace("+9203009436019", "+441234567890");
        assert_eq!(
            B2bmapScraper.parse_supplier(&other, "u").contact_phone,
            None
        );

        // Masked everywhere: no phone.
        let masked = GOLDEN_LISTING.replace("+9203009436019", "+9203009xxxxx");
        assert_eq!(
            B2bmapScraper.parse_supplier(&masked, "u").contact_phone,
            None
        );
    }

    #[test]
    fn incoterms_and_capacity_are_read_from_the_text() {
        let l = B2bmapScraper.parse_listing(GOLDEN_LISTING, "u");
        assert_eq!(l.incoterms.as_deref(), Some("FOB, CIF"));
        assert_eq!(
            l.production_capacity.as_deref(),
            Some("50 Paver Tiles / 8,000 to 11,000 Blocks per shift")
        );
        // The Loyal Vina page names no Incoterms and no capacity.
        let l = B2bmapScraper.parse_listing(LISTING, "u");
        assert_eq!(l.incoterms, None);
        assert_eq!(l.production_capacity, None);
        assert_eq!(incoterms_in(&[Some("Comfortable fobbing")]), None);
    }

    #[test]
    fn address_gets_the_city_and_join_year_is_recorded() {
        let s = B2bmapScraper
            .enrich_from_company_profile(B2bSupplierProfile::default(), GOLDEN_PROFILE);
        assert!(s.company_description.unwrap().contains(
            "Registered address: 15 Km Near Ring road interchange main multan road, Lahore, Punjab, 54000."
        ));
        let r = B2bmapScraper
            .enrich_record_from_company_profile(None, GOLDEN_PROFILE)
            .expect("record");
        assert_eq!(r.platform, "b2bmap");
        assert_eq!(r.joined_platform_year, Some(2026));
        assert!(r.years_on_platform.is_some());
        assert!(
            B2bmapScraper
                .enrich_record_from_company_profile(None, "<html></html>")
                .is_none()
        );
    }

    #[test]
    fn name_and_role_are_split() {
        assert_eq!(
            split_name_and_role(" Mr. Minh Trung  (Sale Manager) "),
            (
                "Mr. Minh Trung".to_string(),
                Some("Sale Manager".to_string())
            )
        );
        assert_eq!(
            split_name_and_role("Mr. Mike Huang"),
            ("Mr. Mike Huang".to_string(), None)
        );
    }

    #[test]
    fn empty_page_does_not_panic() {
        let s = B2bmapScraper.parse_supplier("<html></html>", "u");
        assert!(s.company_name.is_none() && s.contact_phone.is_none());
        let l = B2bmapScraper.parse_listing("<html></html>", "u");
        assert!(l.title.is_none() && l.description.is_none() && l.image_urls.is_empty());
    }
}
