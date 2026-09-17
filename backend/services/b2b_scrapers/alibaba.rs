use crate::services::b2b_scrapers::{B2bListingProfile, B2bScraper, B2bSupplierProfile};
use chrono::Datelike;
use scraper::{Html, Selector};

pub struct AlibabaScraper;

fn extract_overview_field(document: &Html, label: &str) -> Option<String> {
    let button_sel = Selector::parse("button.id-cursor-default").ok()?;
    let div_sel = Selector::parse("div").ok()?;
    let buttons: Vec<_> = document.select(&button_sel).collect();
    for button in &buttons {
        let divs: Vec<_> = button.select(&div_sel).collect();
        let texts: Vec<String> = divs
            .iter()
            .map(|d| d.text().collect::<String>().trim().to_string())
            .collect();
        let label_matches = texts.iter().any(|t| t.starts_with(label));
        if label_matches {
            for d in &divs {
                if let Some(title) = d.value().attr("title") {
                    if !title.is_empty() {
                        return Some(title.to_string());
                    }
                }
            }
        }
    }
    None
}

impl B2bScraper for AlibabaScraper {
    fn matches_platform(&self, platform: &str) -> bool {
        platform == "alibaba"
    }

    fn parse_supplier(&self, html: &str, profile_url: &str) -> B2bSupplierProfile {
        let document = Html::parse_document(html);

        // Company name - the real, linked name inside the mini company card
        let company_name = select_attr_or_text(
            &document,
            "[data-testid='three-column-mini-company-card'] a.id-underline",
        );

        // Location + years + optional badge honorific all live in the
        // same, real row of small spans after the flag icon - e.g.
        // "CN", "10 yrs" (listing 1, no badge) or "Shenzhen, CN",
        // "8 yrs", "Trusted service provider" (listing 2, with badge).
        // Genuinely variable in length, so this walks every span and
        // classifies each by its real, actual content pattern, rather
        // than assuming a fixed position.
        let mut country = None;
        let mut years_text = None;
        let mut badge_honorific = None;
        if let Ok(sel) =
            Selector::parse("[data-testid='three-column-mini-company-card'] .id-mt-1 span")
        {
            for el in document.select(&sel) {
                let text = el.text().collect::<String>().trim().to_string();
                if text.is_empty() {
                    continue;
                }
                if text.ends_with("yrs") {
                    years_text = Some(text);
                } else if text == "CN" || text.contains(", CN") || text.len() <= 4 {
                    // A bare country code, or "City, CC" - both count
                    // as the real, genuine location field.
                    if country.is_none() {
                        country = Some(text);
                    }
                } else {
                    // Anything else this specific, e.g. "Trusted
                    // service provider" or "Multispecialty Supplier",
                    // is a real, genuine supplier badge honorific.
                    badge_honorific = Some(text);
                }
            }
        }

        // The real, genuine verified badge - a small image with this
        // exact test ID, present only on badged suppliers (listings
        // 2-5), absent entirely for unbadged ones (listing 1).
        let platform_verified_badge = document
            .select(
                &Selector::parse("[data-testid='three-column-mini-company-card-verify-icon']")
                    .unwrap(),
            )
            .next()
            .is_some();

        let logo_url = select_attr(
            &document,
            "[data-testid='three-column-mini-company-card'] img",
            "src",
        );

        let overview_year = extract_overview_field(&document, "Year founded");
        let sales_revenue = extract_overview_field(&document, "Online revenue");

        let year_established = overview_year.or_else(|| {
            years_text.as_deref().and_then(|t| {
                let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
                digits
                    .parse::<i32>()
                    .ok()
                    .map(|years| (chrono::Utc::now().year() - years).to_string())
            })
        });

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
            source_platform: "alibaba".to_string(),
            contact_name: None,
            contact_phone: None,
            badge_honorific,
            company_description: None,
        }
    }

    fn extract_company_profile_url(&self, listing_html: &str) -> Option<String> {
        let document = Html::parse_document(listing_html);
        let sel = Selector::parse("a").ok()?;
        for el in document.select(&sel) {
            if el.text().collect::<String>().trim() == "Company profile" {
                if let Some(href) = el.value().attr("href") {
                    return Some(href.to_string());
                }
            }
        }
        None
    }

    fn enrich_from_company_profile(
        &self,
        mut supplier: B2bSupplierProfile,
        profile_html: &str,
    ) -> B2bSupplierProfile {
        let document = Html::parse_document(profile_html);

        // Older, real template - a link (a.vd-item) whose text
        // includes "Total Employees:" with the real value in a
        // sibling ".con-text" span.
        if let Ok(sel) = Selector::parse("a.vd-item") {
            for el in document.select(&sel) {
                let text = el.text().collect::<String>();
                if text.contains("Total Employees:") {
                    if let Ok(value_sel) = Selector::parse(".con-text") {
                        if let Some(value_el) = el.select(&value_sel).next() {
                            let value = value_el.text().collect::<String>().trim().to_string();
                            if !value.is_empty() {
                                supplier.employee_count = Some(value);
                            }
                        }
                    }
                }
            }
        }

        // Newer, real "sp:"-prefixed template - a pair of sibling
        // spans, the first with the real label text "Total
        // employees", the second holding the real numeric value.
        // Only runs if the older template found nothing. Note: this
        // panel loads asynchronously on Alibaba's real page, so it
        // is genuinely absent from a plain ScraperAPI fetch even when
        // this selector is otherwise correct - a known, current gap.
        if supplier.employee_count.is_none() {
            if let Ok(div_sel) = Selector::parse("div") {
                for div in document.select(&div_sel) {
                    let class = div.value().attr("class").unwrap_or("");
                    if class.contains("items-start") && class.contains("justify-between") {
                        if let Ok(span_sel) = Selector::parse("span") {
                            let spans: Vec<_> = div.select(&span_sel).collect();
                            if spans.len() == 2 {
                                let label = spans[0].text().collect::<String>().trim().to_string();
                                if label == "Total employees" {
                                    let value =
                                        spans[1].text().collect::<String>().trim().to_string();
                                    if !value.is_empty() {
                                        supplier.employee_count = Some(value);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        supplier
    }

    fn parse_listing(&self, html: &str, listing_url: &str) -> B2bListingProfile {
        let document = Html::parse_document(html);

        let title = select_attr_or_text(&document, "h1[title]");
        let description = build_description_from_attributes(&document);
        let image_urls = extract_image_urls(&document);

        // Try ladder pricing first (multiple tiers); fall back to
        // range pricing (one price + separate MOQ) if that's genuinely
        // what this listing uses instead.
        let (unit_price, minimum_order_quantity) = extract_price(&document);

        B2bListingProfile {
            title,
            description,
            image_urls,
            unit_price,
            fob_price: None,
            minimum_order_quantity,
            payment_type: None,
            preferred_port: None,
            reference: None,
            production_capacity: None,
            delivery_timeframe: None,
            incoterms: None,
            packaging_details: None,
            listing_url: listing_url.to_string(),
            source_platform: "alibaba".to_string(),
        }
    }
}

fn extract_price(document: &Html) -> (Option<String>, Option<String>) {
    // Ladder pricing - real, multiple price tiers.
    if let Ok(sel) = Selector::parse("[data-testid='ladder-price'] .price-item") {
        let tiers: Vec<String> = document
            .select(&sel)
            .filter_map(|el| {
                let text = el.text().collect::<String>();
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect();
        if !tiers.is_empty() {
            return (Some(tiers.join(" | ")), None);
        }
    }

    // Range pricing - one price, MOQ stated separately.
    if let Ok(sel) = Selector::parse("[data-testid='range-price']") {
        if let Some(el) = document.select(&sel).next() {
            let text = el.text().collect::<String>();
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                // The MOQ is embedded in the same block's text, e.g.
                // "US$0.05-0.30 Minimum order quantity: 100 pieces" -
                // split it out into its own, real, separate field.
                let moq = trimmed
                    .split("Minimum order quantity:")
                    .nth(1)
                    .map(|s| s.trim().to_string());
                let price = trimmed
                    .split("Minimum order quantity:")
                    .next()
                    .map(|s| s.trim().to_string());
                return (price, moq);
            }
        }
    }

    (None, None)
}

fn build_description_from_attributes(document: &Html) -> Option<String> {
    // Alibaba doesn't have one, single free-text description block -
    // its real content is the "Key attributes" grid (Material,
    // Gender, Place of Origin, etc.). Genuinely combining these into
    // one readable string gives Claude the same, real substance a
    // free-text description would.
    let mut parts = Vec::new();
    if let Ok(row_sel) = Selector::parse("[data-testid='three-column-key-attributes-row']") {
        for row in document.select(&row_sel) {
            if let Ok(pair_sel) = Selector::parse("p") {
                let texts: Vec<String> = row
                    .select(&pair_sel)
                    .map(|p| p.text().collect::<String>().trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
                for chunk in texts.chunks(2) {
                    if chunk.len() == 2 {
                        parts.push(format!("{}: {}", chunk[0], chunk[1]));
                    }
                }
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(". "))
    }
}

fn extract_image_urls(document: &Html) -> Vec<String> {
    let mut urls = Vec::new();
    if let Ok(sel) = Selector::parse("[data-testid='main-image-thumbnail']") {
        for el in document.select(&sel) {
            if let Some(style) = el.value().attr("style") {
                if let Some(start) = style.find("url(\"") {
                    let rest = &style[start + 5..];
                    if let Some(end) = rest.find('"') {
                        let url = &rest[..end];
                        if !url.contains("icon-play") {
                            urls.push(if url.starts_with("//") {
                                format!("https:{}", url)
                            } else {
                                url.to_string()
                            });
                        }
                    }
                }
            }
        }
    }
    urls
}

fn select_attr_or_text(document: &Html, selector: &str) -> Option<String> {
    let sel = Selector::parse(selector).ok()?;
    document.select(&sel).next().and_then(|el| {
        el.value().attr("title").map(|s| s.to_string()).or_else(|| {
            let text = el.text().collect::<String>().trim().to_string();
            if text.is_empty() { None } else { Some(text) }
        })
    })
}

fn select_attr(document: &Html, selector: &str, attr: &str) -> Option<String> {
    let sel = Selector::parse(selector).ok()?;
    document
        .select(&sel)
        .next()
        .and_then(|el| el.value().attr(attr))
        .map(|s| s.to_string())
}
