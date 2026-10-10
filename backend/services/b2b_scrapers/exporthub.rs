use crate::services::b2b_scrapers::{
    B2bListingProfile, B2bScraper, B2bSupplierProfile, SupplierRecord,
};
use chrono::{Datelike, Utc};
use scraper::{Html, Selector};
use std::collections::HashMap;

pub struct ExporthubScraper;

/// Longest product description passed on. ExportHub sellers often paste
/// long keyword lists; the first part is what matters for the check.
const MAX_DESCRIPTION_CHARS: usize = 3000;

impl B2bScraper for ExporthubScraper {
    fn matches_platform(&self, platform: &str) -> bool {
        platform == "exporthub"
    }

    fn parse_supplier(&self, html: &str, profile_url: &str) -> B2bSupplierProfile {
        let document = Html::parse_document(html);

        let company_name = select_text(&document, ".product-del_sidebar__comp-ttl");

        // Images are lazy-loaded: the real URL is in data-src, not src.
        let logo_url = select_img_src(&document, ".product-del_sidebar__comp-imgspn img")
            .filter(|url| !is_exporthub_placeholder_image(url));

        // The "About [company]" box on the listing page.
        let about_fields = extract_about_box_pairs(&document);

        let year_established = about_fields
            .get("Year of Establishment")
            .and_then(|v| non_placeholder(v));
        let country = about_fields
            .get("Country / Region")
            .and_then(|v| non_placeholder(v));
        let sales_revenue = about_fields
            .get("Total Annual Revenue")
            .and_then(|v| non_placeholder(v));

        // The seal is a paid membership tier ("Standard Membership",
        // "Premium Membership") or "Free Member" - ExportHub sells it, it
        // does not verify the company. So it is kept as the tier name
        // but never counted as a verified badge.
        let badge_honorific = select_attr(&document, ".product-del_sidebar__seal img", "alt")
            .or_else(|| select_text(&document, ".eh-seal-free"));
        let platform_verified_badge = false;

        // The street address in the sidebar ("Meşrutiyet Mah. ... Şişli,
        // Istanbul, Turkey"). Passed on in the description, so the check
        // knows a street address is shown.
        let company_description = sidebar_address(&document).map(|a| address_line(&a));

        B2bSupplierProfile {
            company_name,
            logo_url,
            year_established,
            country,
            platform_verified_badge,
            employee_count: None,
            sales_revenue,
            export_percentage: None,
            profile_url: profile_url.to_string(),
            source_platform: "exporthub".to_string(),
            contact_name: None,
            contact_phone: None,
            badge_honorific,
            company_description,
            website_url: None,
        }
    }

    fn parse_listing(&self, html: &str, listing_url: &str) -> B2bListingProfile {
        let document = Html::parse_document(html);

        let title = select_text(&document, "h1.prod-dtl_ttl");
        let unit_price = select_text(&document, ".prod-dtl_sl__pr");
        let description = extract_description(&document);

        let attrs = extract_colon_pairs(&document, ".prod-dtl_atr__box");

        // Payment icons carry the method name in their title attribute
        // (aria-label kept as a fallback for older markup).
        let mut payment_methods = Vec::new();
        if let Ok(sel) = Selector::parse(".pm-icon") {
            for el in document.select(&sel) {
                let label = el
                    .value()
                    .attr("title")
                    .or_else(|| el.value().attr("aria-label"))
                    .map(str::trim)
                    .filter(|l| !l.is_empty());
                if let Some(label) = label {
                    if !payment_methods.iter().any(|m: &String| m == label) {
                        payment_methods.push(label.to_string());
                    }
                }
            }
        }
        let payment_type = if payment_methods.is_empty() {
            None
        } else {
            Some(payment_methods.join(", "))
        };

        let image_urls = extract_image_urls(&document);
        // ExportHub's automatic paragraph names the freight terms ("offer
        // the following freight options; FOB, CFR").
        let incoterms = incoterms_in(description.as_deref());

        B2bListingProfile {
            title,
            description,
            image_urls,
            unit_price,
            fob_price: None,
            minimum_order_quantity: attrs.get("Minimum Order Quantity").cloned(),
            payment_type,
            preferred_port: attrs.get("Shipment Port").cloned(),
            reference: None,
            // "High" and "Standard" are ExportHub's default choices, not
            // real details, so they don't count as filled in.
            production_capacity: attrs
                .get("Production Capacity")
                .and_then(|v| real_detail(v)),
            delivery_timeframe: attrs.get("Shipment Delivery Time").cloned(),
            incoterms,
            packaging_details: attrs.get("Packaging").and_then(|v| real_detail(v)),
            listing_url: listing_url.to_string(),
            source_platform: "exporthub".to_string(),
        }
    }

    fn extract_company_profile_url(&self, listing_html: &str) -> Option<String> {
        select_attr(
            &Html::parse_document(listing_html),
            ".product-del_sidebar__comp-nm a",
            "href",
        )
    }

    // The company's "profile.html" tab - same company, one more URL
    // segment. ExportHub's company links always end in "/".
    fn build_extended_profile_url(&self, profile_url: &str) -> Option<String> {
        if profile_url.ends_with('/') {
            Some(format!("{}profile.html", profile_url))
        } else {
            Some(format!("{}/profile.html", profile_url))
        }
    }

    fn enrich_from_extended_profile(
        &self,
        mut supplier: B2bSupplierProfile,
        extended_html: &str,
    ) -> B2bSupplierProfile {
        let document = Html::parse_document(extended_html);

        // This tab's description is the fuller version - prefer it (the
        // address line found earlier is kept in front of it).
        if let Some(description) = extract_first_real_paragraph(&document) {
            supplier.company_description = Some(keep_address_line(
                supplier.company_description.as_deref(),
                description,
            ));
        }

        // Structured fallback table - only fills fields still missing.
        // Its "Company Website" row just links back to the ExportHub
        // profile itself, so it is deliberately never used as a website.
        if let Ok(row_sel) = Selector::parse(".rmp-comp--prof_table tr") {
            if let Ok(td_sel) = Selector::parse("td") {
                for row in document.select(&row_sel) {
                    let cells: Vec<String> = row
                        .select(&td_sel)
                        .map(|c| c.text().collect::<String>().trim().to_string())
                        .collect();
                    if cells.len() < 2 {
                        continue;
                    }
                    let label = cells[0].as_str();
                    let Some(value) = non_placeholder(&cells[1]) else {
                        continue;
                    };
                    match label {
                        "Total Workforce" => {
                            if supplier.employee_count.is_none() {
                                supplier.employee_count = Some(value);
                            }
                        }
                        "Year Incorporated" => {
                            if supplier.year_established.is_none() {
                                supplier.year_established = Some(value);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if supplier.contact_phone.is_none() {
            supplier.contact_phone = extract_structured_phone(extended_html);
        }

        supplier
    }

    fn enrich_from_company_profile(
        &self,
        mut supplier: B2bSupplierProfile,
        profile_html: &str,
    ) -> B2bSupplierProfile {
        let document = Html::parse_document(profile_html);

        // On the company page this sidebar holds the contact person
        // (e.g. "Mario Grimaldi"), not the company name.
        if let Some(name) = select_text(&document, ".product-del_sidebar__comp-ttl") {
            if supplier.company_name.as_deref() != Some(name.as_str()) {
                supplier.contact_name = Some(name);
            }
        }

        // ExportHub hides the phone behind "View Phone!" for non-paying
        // visitors, but the same page publishes it for search engines in
        // its structured data (contactPoint.telephone).
        if let Some(phone) = extract_structured_phone(profile_html) {
            supplier.contact_phone = Some(phone);
        }

        // The listing page's "Lisboa, Portugal" is kept; the full street
        // address is only a fallback when no country was found. The street
        // address always goes into the description when the listing page
        // did not already give it.
        let address = sidebar_address(&document);
        if supplier.country.is_none() {
            supplier.country = address.clone();
        }
        if let Some(address) = address {
            let has_address = supplier
                .company_description
                .as_deref()
                .is_some_and(|d| d.starts_with(ADDRESS_PREFIX));
            if !has_address {
                supplier.company_description = Some(match supplier.company_description.take() {
                    Some(d) => format!("{}\n{}", address_line(&address), d),
                    None => address_line(&address),
                });
            }
        }

        // Company facts boxes - Business Nature, Year Established,
        // Annual Turnover, Country/Region, Legal Status, Payment Terms.
        if let Ok(box_sel) = Selector::parse(".comp-dtl-ic_box") {
            for b in document.select(&box_sel) {
                let fragment = Html::parse_fragment(&b.html());
                let label = select_text(&fragment, "h4.comp-dtl_rgtnm").unwrap_or_default();
                let value = select_text(&fragment, "p.comp-dtl_rgtp");
                let value = value.as_deref().and_then(non_placeholder);
                match label.as_str() {
                    "No. Employees" => {
                        if supplier.employee_count.is_none() {
                            supplier.employee_count = value;
                        }
                    }
                    "Year Established" => {
                        if supplier.year_established.is_none() {
                            supplier.year_established = value;
                        }
                    }
                    "Annual Turnover" => {
                        if supplier.sales_revenue.is_none() {
                            supplier.sales_revenue = value;
                        }
                    }
                    "Country/Region" => {
                        if supplier.country.is_none() {
                            supplier.country = value;
                        }
                    }
                    _ => {}
                }
            }
        }

        if let Some(description) = extract_first_real_paragraph(&document) {
            supplier.company_description = Some(keep_address_line(
                supplier.company_description.as_deref(),
                description,
            ));
        }

        if let Ok(row_sel) = Selector::parse("p.list-div") {
            for row in document.select(&row_sel) {
                let text = row.text().collect::<String>();
                if let Some((label, value)) = text.split_once(':') {
                    let label = label.trim();
                    let Some(value) = non_placeholder(value) else {
                        continue;
                    };
                    match label {
                        "Export Percentage" => {
                            if supplier.export_percentage.is_none() {
                                supplier.export_percentage = Some(value);
                            }
                        }
                        "Estimated Employees" => {
                            if supplier.employee_count.is_none() {
                                supplier.employee_count = Some(value);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        supplier
    }

    /// One seller record per company: the company page's URL slug,
    /// e.g. "https://www.exporthub.com/nutridiet-lda-10772764/" ->
    /// "nutridiet-lda-10772764". Every product from that company links
    /// to the same page, so every scan lands on the same record.
    fn company_key(&self, listing_html: &str) -> Option<String> {
        let url = self.extract_company_profile_url(listing_html)?;
        company_slug(&url)
    }

    /// When the company joined ExportHub ("Member Since: 2024"). With no
    /// founding year, the Account age card then shows how long it has
    /// been on ExportHub; with one, a late join is noted (see
    /// apply_supplier_record).
    fn enrich_record_from_company_profile(
        &self,
        record: Option<SupplierRecord>,
        profile_html: &str,
    ) -> Option<SupplierRecord> {
        let document = Html::parse_document(profile_html);
        let Some(joined) =
            select_text(&document, ".rmp-comp--yrs").and_then(|t| member_since_year(&t))
        else {
            return record;
        };
        let mut record = record.unwrap_or_else(|| SupplierRecord {
            platform: "ExportHub".to_string(),
            ..Default::default()
        });
        record.joined_platform_year = Some(joined);
        record.years_on_platform = u32::try_from(Utc::now().year() - joined).ok();
        Some(record)
    }
}

/// Start of the address line put in front of the company description.
const ADDRESS_PREFIX: &str = "Address: ";

fn address_line(address: &str) -> String {
    format!("{}{}.", ADDRESS_PREFIX, address.trim_end_matches('.'))
}

/// The sidebar address, without its "Address:" label and with ExportHub's
/// stray spaces before commas removed: "Meşrutiyet Mah. Ebe Kızı Sok.
/// No:4 D:6 Şişli, Istanbul, Istanbul, Turkey".
fn sidebar_address(document: &Html) -> Option<String> {
    let raw = select_text(document, ".product-del_sidebar__comp-addrs")?;
    let without_label = raw
        .trim()
        .trim_start_matches("Address:")
        .trim_start_matches("address:");
    let cleaned = collapse_whitespace(without_label).replace(" ,", ",");
    let cleaned = cleaned.trim_matches([',', ' ']).to_string();
    (!cleaned.is_empty()).then_some(cleaned)
}

/// A new company description that keeps the address line already found.
fn keep_address_line(old: Option<&str>, new_description: String) -> String {
    match old
        .and_then(|d| d.lines().next())
        .filter(|l| l.starts_with(ADDRESS_PREFIX))
    {
        Some(line) if !new_description.starts_with(ADDRESS_PREFIX) => {
            format!("{}\n{}", line, new_description)
        }
        _ => new_description,
    }
}

/// "Member Since: 2024" -> 2024.
fn member_since_year(text: &str) -> Option<i32> {
    text.split(':')
        .nth(1)?
        .trim()
        .parse::<i32>()
        .ok()
        .filter(|y| (1990..=2100).contains(y))
}

/// ExportHub's default choices that say nothing about the product.
fn real_detail(value: &str) -> Option<String> {
    let v = value.trim();
    let lower = v.to_lowercase();
    if v.is_empty()
        || ["high", "standard", "inquire", "contact", "low", "medium"].contains(&lower.as_str())
    {
        None
    } else {
        Some(v.to_string())
    }
}

const INCOTERMS: &[&str] = &[
    "EXW", "FCA", "FAS", "FOB", "CFR", "CNF", "CIF", "CPT", "CIP", "DAP", "DPU", "DDP", "DAT",
    "DDU",
];

/// The Incoterms named in the text, each once ("FOB, CFR"). Only whole
/// upper-case words count.
fn incoterms_in(text: Option<&str>) -> Option<String> {
    let mut found: Vec<&str> = Vec::new();
    for word in text?.split(|c: char| !c.is_ascii_alphanumeric()) {
        if let Some(term) = INCOTERMS.iter().find(|t| **t == word) {
            if !found.contains(term) {
                found.push(term);
            }
        }
    }
    (!found.is_empty()).then(|| found.join(", "))
}

/// "https://www.exporthub.com/nutridiet-lda-10772764/" -> "nutridiet-lda-10772764".
/// Only accepts ExportHub company pages (one path segment, no ".html").
fn company_slug(url: &str) -> Option<String> {
    let rest = url.split("exporthub.com/").nth(1)?;
    let rest = rest.split(|c| c == '?' || c == '#').next()?;
    let segments: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() != 1 {
        return None;
    }
    let slug = segments[0].trim().to_lowercase();
    if slug.is_empty() || slug.ends_with(".html") || slug == "product" {
        None
    } else {
        Some(slug)
    }
}

/// Reads `"telephone": "351936140278"` from the page's JSON-LD. The
/// block is not valid JSON on ExportHub (stray braces), so this reads
/// the value directly instead of parsing the whole block.
fn extract_structured_phone(html: &str) -> Option<String> {
    let start = html.find("\"telephone\"")?;
    let after_key = &html[start + "\"telephone\"".len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?.trim_start();
    let value = after_colon.strip_prefix('"')?;
    let end = value.find('"')?;
    let raw = value[..end].trim();

    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    // Too short to be a real international number (or a placeholder).
    if digits.len() < 7 {
        return None;
    }
    Some(format!("+{}", digits))
}

/// The product description sits in several <p> tags (the outer one is
/// usually empty because the seller's HTML nests paragraphs), so every
/// non-empty paragraph is joined rather than taking only the first.
fn extract_description(document: &Html) -> Option<String> {
    let sel = Selector::parse("#detail p").ok()?;
    let mut parts: Vec<String> = Vec::new();
    for el in document.select(&sel) {
        let text = collapse_whitespace(&el.text().collect::<String>());
        if !text.is_empty() && !parts.iter().any(|p| p.contains(&text)) {
            parts.push(text);
        }
    }
    if parts.is_empty() {
        return None;
    }
    let joined = parts.join("\n");
    Some(if joined.chars().count() > MAX_DESCRIPTION_CHARS {
        joined.chars().take(MAX_DESCRIPTION_CHARS).collect()
    } else {
        joined
    })
}

fn extract_first_real_paragraph(document: &Html) -> Option<String> {
    let sel = Selector::parse(".rmp-comp--desp_cont p").ok()?;
    let first_real_text = document
        .select(&sel)
        .map(|el| el.text().collect::<String>())
        .map(|t| t.trim().to_string())
        .find(|t| !t.is_empty())?;

    let description = match first_real_text.find(" Name:") {
        Some(idx) => first_real_text[..idx].trim().to_string(),
        None => first_real_text,
    };

    if description.is_empty() {
        None
    } else {
        Some(description)
    }
}

/// The listing page's "Label:  Value" divs (.prod-dtl_atr__box).
fn extract_colon_pairs(document: &Html, selector: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    if let Ok(sel) = Selector::parse(selector) {
        for el in document.select(&sel) {
            let text = el.text().collect::<String>();
            if let Some((label, value)) = text.split_once(':') {
                let label = collapse_whitespace(label);
                let value = collapse_whitespace(value);
                if !label.is_empty() && !value.is_empty() {
                    fields.insert(label, value);
                }
            }
        }
    }
    fields
}

/// The "About [company]" box: each row is <span>Label: </span> followed
/// by the value as plain text.
fn extract_about_box_pairs(document: &Html) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    if let Ok(row_sel) = Selector::parse(".prod-dtl_desp__abt-atr") {
        for row in document.select(&row_sel) {
            let fragment = Html::parse_fragment(&row.html());
            let span_text = select_text(&fragment, "span").unwrap_or_default();
            let label = span_text.trim_end_matches(':').trim().to_string();
            let full_text = row.text().collect::<String>();
            let value = full_text
                .trim_start()
                .strip_prefix(span_text.trim())
                .unwrap_or(&full_text)
                .trim()
                .to_string();
            if !label.is_empty() && !value.is_empty() {
                fields.insert(label, value);
            }
        }
    }
    fields
}

/// Main product image, then the gallery thumbnails, then the page's
/// og:image as a last resort (always a plain URL, even if the fetched
/// page was rendered with a different lazy-loading state).
fn extract_image_urls(document: &Html) -> Vec<String> {
    fn push(urls: &mut Vec<String>, url: String) {
        if !url.contains("noimage") && !urls.contains(&url) {
            urls.push(url);
        }
    }
    let mut urls: Vec<String> = Vec::new();
    if let Some(src) = select_img_src(document, "#show-img") {
        push(&mut urls, src);
    }
    if let Ok(sel) = Selector::parse("#small-img-roll img") {
        for el in document.select(&sel) {
            if let Some(src) = img_url(&el) {
                push(&mut urls, src);
            }
        }
    }
    if urls.is_empty() {
        if let Some(og) = select_attr(document, r#"meta[property="og:image"]"#, "content") {
            if og.starts_with("http") {
                push(&mut urls, og);
            }
        }
    }
    urls
}

/// Image URL of the first element matching `selector`.
fn select_img_src(document: &Html, selector: &str) -> Option<String> {
    let sel = Selector::parse(selector).ok()?;
    img_url(&document.select(&sel).next()?)
}

/// Real URL of an <img>, whichever attribute holds it. Lazy-loaded
/// images keep it in data-src while src may be empty or a tiny
/// "data:" placeholder, so each attribute is checked in turn.
fn img_url(el: &scraper::ElementRef) -> Option<String> {
    ["src", "data-src", "data-original", "data-lazy-src"]
        .iter()
        .filter_map(|a| el.value().attr(a))
        .map(str::trim)
        .find(|s| !s.is_empty() && !s.starts_with("data:"))
        .map(|s| s.to_string())
}

fn select_text(document: &Html, selector: &str) -> Option<String> {
    let sel = Selector::parse(selector).ok()?;
    let text = document.select(&sel).next()?.text().collect::<String>();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_exporthub_placeholder_image(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("noimg") || lower.contains("/avatar.jpg")
}

fn non_placeholder(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("not provided") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn select_attr(document: &Html, selector: &str, attr: &str) -> Option<String> {
    let sel = Selector::parse(selector).ok()?;
    document
        .select(&sel)
        .next()?
        .value()
        .attr(attr)
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from the real Nutridiet listing page (Sep 2026).
    const LISTING: &str = r#"<html><body>
<div class="prod-dtl_img__box"><div class="show"><img data-src="https://img.exporthub.com/storage/app/images/products/5/9/o_1784811835_59.jpeg_.webp" id="show-img" class="prod-dtl_img lazyload"></div></div>
<h1 class="prod-dtl_ttl">Cpu Ceramic Scrap, Gold Plated Pin Scrap Wholesale</h1>
<span class="prod-dtl_sl__pr">USD 8 / Kilogram</span>
<div class="prod-dtl_atr__box">
    Minimum Order Quantity:&nbsp;
        250 Kilogram
</div>
<div class="prod-dtl_atr__box">
    Shipment Delivery Time:&nbsp;
        7 days
</div>
<div class="prod-dtl_atr__box">Shipment Port:&nbsp; Lisbon</div>
<div class="prod-dtl_atr__box">Production Capacity:&nbsp;10000 kg Kilogram</div>
<div class="prod-dtl_atr__box">Packaging:&nbsp; cartons</div>
<span class="prod-dtl_payment__img">
<i class="pm-icon tt" data-toggle="tooltip" title="Bank wire (T/T)"></i>
<i class="pm-icon moneygram" data-toggle="tooltip" title="MoneyGram"></i>
<i class="pm-icon wu" data-toggle="tooltip" title="Western Union (WU)"></i>
</span>
<div id="detail" class="prod-dtl_desp__dtlCont">
<div class="prod-dtl_desp__hd">Details</div>
<p>
<p><span>PREMIUM E-SCRAP &amp; GOLD RECOVERY MATERIALS</span><br />
<span>NUTRIDIET is a trusted European wholesale supplier.</span></p>
</p>
</div>
<div id="about">
<div class="prod-dtl_desp__abt-atr"><span>Business Type: </span>
    Manufacturer
</div>
<div class="prod-dtl_desp__abt-atr"><span>Country / Region: </span>
    Lisboa, Portugal</div>
<div class="prod-dtl_desp__abt-atr"><span>Total Annual Revenue: </span>
    US$1 Million - US$2.5 Million</div>
<div class="prod-dtl_desp__abt-atr"><span>Year of Establishment: </span>
    2011</div>
</div>
<div class="product-del_sidebar__comp">
<div class="product-del_sidebar__comp-nm">
<a href="https://www.exporthub.com/nutridiet-lda-10772764/" title="NUTRIDIET LDA">
<span class="product-del_sidebar__comp-imgspn"><img data-src="https://img.exporthub.com/frontend/company/images/noimg-215x215px.jpg" class="lazyload"></span>
<div class="product-del_sidebar__comp-ttl">NUTRIDIET LDA</div>
</a></div>
<div class="product-del_sidebar__seal"><img class="lazyload" data-src="https://img.exporthub.com/frontend/packages/s_standard.gif" alt="Standard Membership"></div>
</div>
</body></html>"#;

    // Trimmed from the real Nutridiet company page (Sep 2026). The
    // JSON-LD is copied as-is, including its invalid braces.
    const PROFILE: &str = r#"<html><body>
<script type="application/ld+json">
{
    "@type": "Organization",
    "name": "NUTRIDIET LDA",
    "logo": "https://img.exporthub.com/frontend/company/images/noimg-215x215px.jpg"
},
"address": { "addressCountry": "Portugal" },
"contactPoint":
{
    "@type": "ContactPoint",
    "telephone": "351936140278",
    "contactType": "customer service"
}
</script>
<div class="comp-dtl-ic_box"><h4 class="comp-dtl_rgtnm">Year Established</h4><p class="comp-dtl_rgtp">2011</p></div>
<div class="rmp-comp--desp_cont">
<h3 class="rmp-comp--box_ttl">About NUTRIDIET LDA</h3>
<p class="rmp-comp--desp_pera">
<p class="list-div"><p>Nutridiet LDA is a wholesale supplier and distributors of Chemicals</p>
</p></div>
<div class="product-del_sidebar__comp">
<div class="product-del_sidebar__comp-nm"><span title="Anne Hoang" class="product-del_sidebar__comp-nmlnk">
<div class="product-del_sidebar__comp-ttl">
    Mario Grimaldi
</div></span></div>
<div class="product-del_sidebar__comp-addrs">
    Address:<br />
        Alameda das Comunidades Portuguesas,
        Lisboa,
        Portugal
</div>
</div>
</body></html>"#;

    const FREE_LISTING_SEAL: &str = r#"<div class="product-del_sidebar__comp">
<div class="ms-comp-seal sd-cmp_seals__cont"><span class="eh-seal-big-free"><span class="eh-seal-free">Free Member</span></span></div>
</div>"#;

    fn full_supplier() -> B2bSupplierProfile {
        let s = ExporthubScraper;
        let supplier =
            s.parse_supplier(LISTING, "https://www.exporthub.com/product/x-24940942.html");
        s.enrich_from_company_profile(supplier, PROFILE)
    }

    #[test]
    fn listing_fields_are_read() {
        let l = ExporthubScraper.parse_listing(LISTING, "u");
        assert_eq!(
            l.title.as_deref(),
            Some("Cpu Ceramic Scrap, Gold Plated Pin Scrap Wholesale")
        );
        assert_eq!(l.unit_price.as_deref(), Some("USD 8 / Kilogram"));
        assert_eq!(l.minimum_order_quantity.as_deref(), Some("250 Kilogram"));
        assert_eq!(l.delivery_timeframe.as_deref(), Some("7 days"));
        assert_eq!(l.preferred_port.as_deref(), Some("Lisbon"));
        assert_eq!(l.packaging_details.as_deref(), Some("cartons"));
    }

    #[test]
    fn payment_methods_come_from_the_title_attribute() {
        let l = ExporthubScraper.parse_listing(LISTING, "u");
        assert_eq!(
            l.payment_type.as_deref(),
            Some("Bank wire (T/T), MoneyGram, Western Union (WU)")
        );
    }

    #[test]
    fn lazy_loaded_product_image_is_found() {
        let l = ExporthubScraper.parse_listing(LISTING, "u");
        assert_eq!(l.image_urls.len(), 1);
        assert!(l.image_urls[0].ends_with("o_1784811835_59.jpeg_.webp"));
    }

    #[test]
    fn image_found_even_with_a_placeholder_src() {
        let html = r#"<img id="show-img" src="data:image/gif;base64,R0lGOD" data-src="https://img.exporthub.com/a.webp">"#;
        let l = ExporthubScraper.parse_listing(html, "u");
        assert_eq!(
            l.image_urls,
            vec!["https://img.exporthub.com/a.webp".to_string()]
        );

        let og_only = r#"<html><head><meta property="og:image" content="https://img.exporthub.com/o.jpeg" /></head><body><img id="show-img"></body></html>"#;
        let l = ExporthubScraper.parse_listing(og_only, "u");
        assert_eq!(
            l.image_urls,
            vec!["https://img.exporthub.com/o.jpeg".to_string()]
        );
    }

    #[test]
    fn nested_paragraph_description_is_not_lost() {
        let l = ExporthubScraper.parse_listing(LISTING, "u");
        let d = l.description.expect("description");
        assert!(d.contains("PREMIUM E-SCRAP & GOLD RECOVERY MATERIALS"));
        assert!(d.contains("trusted European wholesale supplier"));
    }

    #[test]
    fn supplier_fields_from_listing_and_company_page() {
        let s = full_supplier();
        assert_eq!(s.company_name.as_deref(), Some("NUTRIDIET LDA"));
        assert_eq!(s.contact_name.as_deref(), Some("Mario Grimaldi"));
        assert_eq!(s.contact_phone.as_deref(), Some("+351936140278"));
        assert_eq!(s.country.as_deref(), Some("Lisboa, Portugal"));
        assert_eq!(s.year_established.as_deref(), Some("2011"));
        assert_eq!(
            s.sales_revenue.as_deref(),
            Some("US$1 Million - US$2.5 Million")
        );
        assert_eq!(s.logo_url, None, "noimg placeholder must be dropped");
        assert_eq!(s.website_url, None);
        assert_eq!(
            s.company_description.as_deref(),
            Some(
                "Address: Alameda das Comunidades Portuguesas, Lisboa, Portugal.\nNutridiet LDA is a wholesale supplier and distributors of Chemicals"
            )
        );
    }

    #[test]
    fn membership_tiers() {
        let paid = ExporthubScraper.parse_supplier(LISTING, "u");
        assert!(
            !paid.platform_verified_badge,
            "a paid tier is not verification"
        );
        assert_eq!(paid.badge_honorific.as_deref(), Some("Standard Membership"));

        let free = ExporthubScraper.parse_supplier(FREE_LISTING_SEAL, "u");
        assert!(!free.platform_verified_badge);
        assert_eq!(free.badge_honorific.as_deref(), Some("Free Member"));
    }

    #[test]
    fn address_is_only_a_fallback_for_country() {
        let empty = ExporthubScraper.parse_supplier("<html></html>", "u");
        let s = ExporthubScraper.enrich_from_company_profile(empty, PROFILE);
        assert_eq!(
            s.country.as_deref(),
            Some("Alameda das Comunidades Portuguesas, Lisboa, Portugal")
        );
    }

    /// Trimmed from the real HYM Textile pages (Oct 2026).
    const HYM_LISTING: &str = r#"<html><body>
<div class="prod-dtl_atr__box">Production Capacity: High</div>
<div class="prod-dtl_atr__box">Packaging:&nbsp; Standard</div>
<div id="detail"><p>To facilitate our consumers, offer the following freight options; FOB, CFR. As an international Manufacturer, we accepts all payments methods like T/T, MoneyGram.</p></div>
<div class="product-del_sidebar__comp">
<div class="product-del_sidebar__comp-nm"><a href="https://www.exporthub.com/hym-textile-11125866/"><div class="product-del_sidebar__comp-ttl">HYM Textile</div></a></div>
<div class="product-del_sidebar__comp-addrs">
    Meşrutiyet Mah. Ebe Kızı Sok. No:4 D:6 Şişli , Istanbul , Istanbul , Turkey
</div>
</div>
</body></html>"#;

    const HYM_PROFILE: &str = r#"<html><body>
<span class="rmp-comp--yrs">Member Since: 2024</span>
<div class="rmp-comp--desp_cont"><p>Modern and eye-catching lines Gabbiacci is a pioneering international men's textile brand.</p></div>
<div class="product-del_sidebar__comp">
<div class="product-del_sidebar__comp-ttl">Kristina Chalyshkan</div>
<div class="product-del_sidebar__comp-addrs">
    Address:<br />
        Meşrutiyet Mah. Ebe Kızı Sok. No:4 D:6 Şişli ,
        Istanbul,
        Istanbul,
        Turkey
</div>
</div>
</body></html>"#;

    #[test]
    fn street_address_reaches_the_description() {
        let s = ExporthubScraper.parse_supplier(HYM_LISTING, "u");
        assert_eq!(
            s.company_description.as_deref(),
            Some(
                "Address: Meşrutiyet Mah. Ebe Kızı Sok. No:4 D:6 Şişli, Istanbul, Istanbul, Turkey."
            )
        );
        let s = ExporthubScraper.enrich_from_company_profile(s, HYM_PROFILE);
        let d = s.company_description.unwrap();
        assert!(d.starts_with("Address: Meşrutiyet Mah."), "{d}");
        assert!(
            d.contains("\nModern and eye-catching lines Gabbiacci"),
            "{d}"
        );
        assert_eq!(d.matches("Address:").count(), 1, "address only once");
        assert_eq!(s.contact_name.as_deref(), Some("Kristina Chalyshkan"));

        // Company page alone still gives the address.
        let s = ExporthubScraper
            .enrich_from_company_profile(B2bSupplierProfile::default(), HYM_PROFILE);
        assert!(
            s.company_description
                .unwrap()
                .starts_with("Address: Meşrutiyet Mah. Ebe Kızı Sok. No:4 D:6 Şişli, Istanbul")
        );
    }

    #[test]
    fn default_choices_do_not_count_and_incoterms_are_read() {
        let l = ExporthubScraper.parse_listing(HYM_LISTING, "u");
        assert_eq!(
            l.production_capacity, None,
            "\"High\" is ExportHub's default"
        );
        assert_eq!(
            l.packaging_details, None,
            "\"Standard\" is ExportHub's default"
        );
        assert_eq!(l.incoterms.as_deref(), Some("FOB, CFR"));
        // Real values still count.
        let l = ExporthubScraper.parse_listing(LISTING, "u");
        assert_eq!(l.production_capacity.as_deref(), Some("10000 kg Kilogram"));
        assert_eq!(l.packaging_details.as_deref(), Some("cartons"));
        assert_eq!(l.incoterms, None);
    }

    #[test]
    fn member_since_is_recorded() {
        let r = ExporthubScraper
            .enrich_record_from_company_profile(None, HYM_PROFILE)
            .expect("record");
        assert_eq!(r.platform, "ExportHub");
        assert_eq!(r.joined_platform_year, Some(2024));
        assert!(
            ExporthubScraper
                .enrich_record_from_company_profile(None, PROFILE)
                .is_none()
        );
    }

    #[test]
    fn structured_phone() {
        assert_eq!(
            extract_structured_phone(PROFILE).as_deref(),
            Some("+351936140278")
        );
        assert_eq!(
            extract_structured_phone(r#""telephone":"8617728195735""#).as_deref(),
            Some("+8617728195735")
        );
        assert_eq!(extract_structured_phone(r#""telephone": "" "#), None);
        assert_eq!(extract_structured_phone("<html>no phone</html>"), None);
    }

    #[test]
    fn company_key_is_the_company_page_slug() {
        assert_eq!(
            ExporthubScraper.company_key(LISTING).as_deref(),
            Some("nutridiet-lda-10772764")
        );
        assert_eq!(
            company_slug(
                "https://www.exporthub.com/guangzhou-bingfeng-information-technology-10676854/"
            )
            .as_deref(),
            Some("guangzhou-bingfeng-information-technology-10676854")
        );
        assert_eq!(
            company_slug("https://www.exporthub.com/24-hours-printing/").as_deref(),
            Some("24-hours-printing")
        );
        assert_eq!(
            company_slug("https://www.exporthub.com/product/x-24940942.html"),
            None
        );
        assert_eq!(
            company_slug("https://www.exporthub.com/nutridiet-lda-10772764/profile.html"),
            None
        );
    }
}
