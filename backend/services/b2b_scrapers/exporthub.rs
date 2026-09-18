use crate::services::b2b_scrapers::{B2bListingProfile, B2bScraper, B2bSupplierProfile};
use scraper::{Html, Selector};
use std::collections::HashMap;

pub struct ExporthubScraper;

impl B2bScraper for ExporthubScraper {
    fn matches_platform(&self, platform: &str) -> bool {
        platform == "exporthub"
    }

    fn parse_supplier(&self, html: &str, profile_url: &str) -> B2bSupplierProfile {
        let document = Html::parse_document(html);

        let company_name = select_text(&document, ".product-del_sidebar__comp-ttl");

        let logo_url = select_attr(&document, ".product-del_sidebar__comp-imgspn img", "src")
            .filter(|url| !is_exporthub_placeholder_image(url));

        // Real, genuine "About [company]" box already sits directly
        // on the listing page itself - no second fetch strictly
        // needed for these core fields.
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

        // Real "Premium Membership" seal - a badge image whose alt
        // text names the real tier.
        let badge_honorific = select_attr(&document, ".product-del_sidebar__seal img", "alt");
        let platform_verified_badge = badge_honorific.is_some();

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
            company_description: None,
            website_url: None,
        }
    }

    fn parse_listing(&self, html: &str, listing_url: &str) -> B2bListingProfile {
        let document = Html::parse_document(html);

        let title = select_text(&document, "h1.prod-dtl_ttl");
        let unit_price = select_text(&document, ".prod-dtl_sl__pr");
        let description = select_text(&document, "#detail p");

        // Real, plain "Label:  Value" divs - genuinely different
        // markup shape from TradeWheel's <td> pairs, so this platform
        // needs its own, real colon-split parser.
        let attrs = extract_colon_pairs(&document, ".prod-dtl_atr__box");

        // Real payment-method icons, each carrying its own real
        // method name in aria-label - joined into one readable value.
        let mut payment_methods = Vec::new();
        if let Ok(sel) = Selector::parse(".pm-icon") {
            for el in document.select(&sel) {
                if let Some(label) = el.value().attr("aria-label") {
                    payment_methods.push(label.to_string());
                }
            }
        }
        let payment_type = if payment_methods.is_empty() {
            None
        } else {
            Some(payment_methods.join(", "))
        };

        let image_urls = extract_image_urls(&document);

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
            production_capacity: attrs.get("Production Capacity").cloned(),
            delivery_timeframe: attrs.get("Shipment Delivery Time").cloned(),
            incoterms: None,
            packaging_details: attrs.get("Packaging").cloned(),
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

    // The real, extended "profile.html" tab - same company, one more
    // URL segment. ExportHub's own real links always end in "/",
    // so this simple, direct concatenation is genuinely reliable -
    // confirmed against real, live URLs for both a premium supplier
    // (pujiang-grace-crystal-co-ltd6772/) and a free one
    // (rehman-brothers-10521212/).
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

        // This page's real description is genuinely more complete
        // than the main profile page's version - confirmed directly
        // against Pujiang's real data (a longer, cleaner narrative
        // here vs. a shorter one on the main profile page that
        // trails into contact info). Always prefer it when found.
        if let Some(description) = extract_first_real_paragraph(&document) {
            supplier.company_description = Some(description);
        }

        // Real, structured fallback table - only fills genuinely
        // still-missing fields, since the main profile page's data
        // is already trusted where present. The real "Company
        // Website" row here is self-referential (just links back to
        // the supplier's own ExportHub profile, not a real, external
        // site) - deliberately never extracted.
        if let Ok(row_sel) = Selector::parse(".rmp-comp--prof_table tr") {
            for row in document.select(&row_sel) {
                let fragment = Html::parse_fragment(&row.html());
                if let Ok(td_sel) = Selector::parse("td") {
                    let cells: Vec<String> = fragment
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

        supplier
    }

    fn enrich_from_company_profile(
        &self,
        mut supplier: B2bSupplierProfile,
        profile_html: &str,
    ) -> B2bSupplierProfile {
        let document = Html::parse_document(profile_html);

        if let Some(address) = select_text(&document, ".product-del_sidebar__comp-addrs") {
            let cleaned = address
                .trim_start_matches("Address:")
                .trim_start_matches("address:")
                .trim()
                .to_string();
            if !cleaned.is_empty() {
                supplier.country = Some(cleaned);
            }
        }

        if let Some(name) = select_text(&document, ".product-del_sidebar__comp-ttl") {
            supplier.contact_name = Some(name);
        }

        // Real "comp-dtl-ic_box" pairs - Business Nature, No.
        // Employees, Year Established, Annual Turnover,
        // Country/Region, Main Products, Legal Status, Payment Terms.
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
            supplier.company_description = Some(description);
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

/// Real ExportHub-specific parser for the listing page's plain
/// "Label:  Value" divs (e.g. .prod-dtl_atr__box) - genuinely
/// different markup from TradeWheel's <td> pairs, so this platform
/// needs its own colon-split logic.
fn extract_colon_pairs(document: &Html, selector: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    if let Ok(sel) = Selector::parse(selector) {
        for el in document.select(&sel) {
            let text = el.text().collect::<String>();
            if let Some((label, value)) = text.split_once(':') {
                let label = label.trim().to_string();
                let value = value.trim().to_string();
                if !label.is_empty() && !value.is_empty() {
                    fields.insert(label, value);
                }
            }
        }
    }
    fields
}

/// Real "About [company]" box on the listing page - each real row is
/// a <span>Label: </span> followed by plain trailing text as the
/// value, inside one shared div.
fn extract_about_box_pairs(document: &Html) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    if let Ok(row_sel) = Selector::parse(".prod-dtl_desp__abt-atr") {
        for row in document.select(&row_sel) {
            let fragment = Html::parse_fragment(&row.html());
            let span_text = select_text(&fragment, "span").unwrap_or_default();
            let label = span_text.trim_end_matches(':').trim().to_string();
            let full_text = row.text().collect::<String>();
            // The real span text (e.g. "Year of Establishment: ")
            // must be stripped from the START of the real, full row
            // text - trim_start, not raw strip_prefix, since the raw
            // text often has leading whitespace before the span's own
            // text even begins.
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

fn extract_image_urls(document: &Html) -> Vec<String> {
    let mut urls = Vec::new();
    if let Some(src) = select_attr(document, "#show-img", "src") {
        if !src.contains("noimage") {
            urls.push(src);
        }
    }
    urls
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
