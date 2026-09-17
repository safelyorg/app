use crate::services::b2b_scrapers::{B2bListingProfile, B2bScraper, B2bSupplierProfile};
use scraper::{Html, Selector};
use std::collections::HashMap;

pub struct TradewheelScraper;

impl B2bScraper for TradewheelScraper {
    fn matches_platform(&self, platform: &str) -> bool {
        platform == "tradewheel"
    }

    fn parse_supplier(&self, html: &str, profile_url: &str) -> B2bSupplierProfile {
        let document = Html::parse_document(html);

        let company_name = select_text(&document, ".comp-info h2");

        // Country sits as plain text inside .bo-flag, alongside a
        // flag <i> icon with no text of its own - collecting all
        // text and trimming correctly leaves just the real country
        // name, e.g. "United Kingdom".
        let country = select_text(&document, ".bo-flag");

        // The badge image's real filename (e.g. "gold-txt1.png")
        // names the real membership tier - used the same way
        // Alibaba's badge_honorific captures a supplier tier label.
        let badge_honorific = select_attr(&document, ".comp-info img", "src").and_then(|src| {
            if src.to_lowercase().contains("gold") {
                Some("Gold".to_string())
            } else {
                None
            }
        });
        let platform_verified_badge = badge_honorific.is_some();

        B2bSupplierProfile {
            company_name,
            logo_url: None,
            year_established: None,
            country,
            platform_verified_badge,
            employee_count: None,
            sales_revenue: None,
            export_percentage: None,
            profile_url: profile_url.to_string(),
            source_platform: "tradewheel".to_string(),
            contact_name: None,
            contact_phone: None,
            badge_honorific,
            company_description: None,
            website_url: None,
        }
    }

    fn parse_listing(&self, html: &str, listing_url: &str) -> B2bListingProfile {
        let document = Html::parse_document(html);

        let title = select_text(&document, "h1.pd-heading");

        let description = select_all_text(&document, ".product-details-container p");

        let mut fields = extract_label_value_pairs(&document, ".po-box table tr");
        for (k, v) in extract_label_value_pairs(&document, "table.attr_table tr") {
            fields.entry(k).or_insert(v);
        }
        for (k, v) in extract_label_value_pairs(&document, "table.quick_details_table tr") {
            fields.entry(k).or_insert(v);
        }

        let image_urls = extract_image_urls(&document);

        B2bListingProfile {
            title,
            description,
            image_urls,
            unit_price: fields.remove("Price"),
            fob_price: None,
            minimum_order_quantity: fields.remove("Quantity"),
            payment_type: None,
            preferred_port: fields.remove("Port"),
            reference: None,
            production_capacity: None,
            delivery_timeframe: fields.remove("Lead Time"),
            incoterms: None,
            packaging_details: fields.remove("Packaging"),
            listing_url: listing_url.to_string(),
            source_platform: "tradewheel".to_string(),
        }
    }

    fn extract_company_profile_url(&self, listing_html: &str) -> Option<String> {
        select_attr(&Html::parse_document(listing_html), ".comp-info a", "href")
    }

    fn enrich_from_company_profile(
        &self,
        mut supplier: B2bSupplierProfile,
        profile_html: &str,
    ) -> B2bSupplierProfile {
        let document = Html::parse_document(profile_html);

        // Two real, separate tables - "Company Information" and
        // "Trading Information" - each under its own
        // .co-specification-container, identified by its own
        // h3.secondary-heading text.
        if let Ok(container_sel) = Selector::parse(".co-specification-container") {
            for container in document.select(&container_sel) {
                let fragment = Html::parse_fragment(&container.html());
                let heading = select_text(&fragment, "h3.secondary-heading").unwrap_or_default();
                let fields = extract_label_value_pairs(&fragment, "table tr");

                match heading.as_str() {
                    "Company Information" => {
                        if let Some(v) = fields.get("Established Year") {
                            supplier.year_established = non_empty(v);
                        }
                        if let Some(v) = fields.get("Total Employees") {
                            supplier.employee_count = non_empty(v);
                        }
                    }
                    "Trading Information" => {
                        if let Some(v) = fields.get("Total Revenue") {
                            supplier.sales_revenue = non_empty(v);
                        }
                        if let Some(v) = fields.get("Export Percentage") {
                            supplier.export_percentage = non_empty(v);
                        }
                    }
                    "Contact Details" => {
                        if let Some(name) = select_text(&fragment, ".contact_p_txt1") {
                            supplier.contact_name = Some(name);
                        }
                        if let Some(logo) = select_attr(&fragment, "#m_img", "src") {
                            supplier.logo_url = Some(logo);
                        }
                        if let Ok(row_sel) = Selector::parse("tr") {
                            for row in fragment.select(&row_sel) {
                                let row_text = row.text().collect::<String>();
                                let trimmed_row_text = row_text.trim_start();
                                if trimmed_row_text.starts_with("Website:") {
                                    let value = trimmed_row_text
                                        .trim_start_matches("Website:")
                                        .trim()
                                        .to_string();
                                    if !value.is_empty() && value.to_lowercase() != "show" {
                                        supplier.website_url = Some(value);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        supplier
    }
}

/// Real, shared table-row parser: for each <tr>, takes its <td>
/// cells in order and treats the first as the label, the second as
/// the value - matching TradeWheel's real, consistent "td.td1 label,
/// plain td value" pattern used across every info table on the site.
fn extract_label_value_pairs(document: &Html, row_selector: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    if let Ok(row_sel) = Selector::parse(row_selector) {
        for row in document.select(&row_sel) {
            let fragment = Html::parse_fragment(&row.html());
            if let Ok(td_sel) = Selector::parse("td") {
                let cells: Vec<String> = fragment
                    .select(&td_sel)
                    .map(|td| td.text().collect::<String>().trim().to_string())
                    .collect();
                // Real rows may hold two label/value pairs side by
                // side (Quick Details), so walk every consecutive
                // pair, not just the first two cells.
                for pair in cells.chunks(2) {
                    if pair.len() == 2 && !pair[0].is_empty() {
                        fields.insert(pair[0].clone(), pair[1].clone());
                    }
                }
            }
        }
    }
    fields
}

fn extract_image_urls(document: &Html) -> Vec<String> {
    let mut urls = Vec::new();
    if let Ok(sel) = Selector::parse(".pd-thumbs a") {
        for el in document.select(&sel).take(3) {
            let value = el.value();
            if let Some(url) = value
                .attr("data-zoom-image")
                .or_else(|| value.attr("data-image"))
            {
                urls.push(url.to_string());
            }
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

fn select_all_text(document: &Html, selector: &str) -> Option<String> {
    let sel = Selector::parse(selector).ok()?;
    let parts: Vec<String> = document
        .select(&sel)
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
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

fn non_empty(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
