use crate::services::b2b_scrapers::{B2bListingProfile, B2bScraper, B2bSupplierProfile};
use scraper::{Html, Selector};

pub struct B2brazilScraper;

impl B2bScraper for B2brazilScraper {
    fn matches_platform(&self, platform: &str) -> bool {
        platform == "b2brazil"
    }

    fn parse_supplier(&self, html: &str, profile_url: &str) -> B2bSupplierProfile {
        let document = Html::parse_document(html);

        let logo_url = select_attr(&document, "#header-info-thumb img", "src");
        let company_name = select_text(&document, "#header-info-company h1");

        let platform_verified_badge = Selector::parse("#header-info-company-verify")
            .ok()
            .and_then(|sel| document.select(&sel).next())
            .map(|el| {
                let class = el.value().attr("class").unwrap_or("");
                !class.contains("not-verify")
            })
            .unwrap_or(false);

        let mut year_established = None;
        let mut country = None;
        if let Ok(sel) = Selector::parse(".actions-item h4") {
            for el in document.select(&sel) {
                let text = el.text().collect::<String>();
                let trimmed = text.trim();
                if let Some(year) = trimmed.strip_prefix("Since ") {
                    year_established = Some(year.trim().to_string());
                } else if !trimmed.is_empty() && year_established.is_some() && country.is_none() {
                    country = Some(trimmed.to_string());
                }
            }
        }

        let mut employee_count = None;
        let mut sales_revenue = None;
        let mut export_percentage = None;
        if let Ok(sel) = Selector::parse(".section-content-more-info-item") {
            for item in document.select(&sel) {
                let label =
                    select_text(&Html::parse_fragment(&item.html()), "p").unwrap_or_default();
                let value = select_text(&Html::parse_fragment(&item.html()), "h4");
                match label.as_str() {
                    "Employees" => employee_count = value,
                    "Sales volume (USD)" => sales_revenue = value,
                    "% Export sales" => export_percentage = value,
                    _ => {}
                }
            }
        }

        B2bSupplierProfile {
            company_name,
            logo_url,
            year_established,
            country,
            platform_verified_badge,
            employee_count,
            sales_revenue,
            export_percentage,
            profile_url: profile_url.to_string(),
            source_platform: "b2brazil".to_string(),
        }
    }

    fn parse_listing(&self, html: &str, listing_url: &str) -> B2bListingProfile {
        let document = Html::parse_document(html);

        let title = select_text(&document, ".section-product-title");
        let description = select_text(&document, ".section-content-about.product-description p");

        let mut fields = std::collections::HashMap::new();
        if let Ok(sel) = Selector::parse(".section-product-item") {
            for item in document.select(&sel) {
                let fragment = Html::parse_fragment(&item.html());
                let label = select_text(&fragment, "h3").unwrap_or_default();
                let label = label.trim_end_matches(':').to_string();
                // Most fields wrap their value in <p>; Incoterms in the
                // real page has no <p> at all, just plain trailing text.
                let value = select_text(&fragment, "p")
                    .or_else(|| {
                        let full_text = fragment.root_element().text().collect::<String>();
                        let after_label = full_text.splitn(2, &label).nth(1)?;
                        let cleaned = after_label.trim_start_matches(':').trim();
                        if cleaned.is_empty() {
                            None
                        } else {
                            Some(cleaned.to_string())
                        }
                    })
                    .filter(|v| v != "Not informed");
                fields.insert(label, value);
            }
        }

        B2bListingProfile {
            title,
            description,
            unit_price: fields.remove("Unit Price").flatten(),
            fob_price: fields.remove("FOB Price").flatten(),
            minimum_order_quantity: fields.remove("Minimum Order Quantity").flatten(),
            payment_type: fields.remove("Type of Payment").flatten(),
            preferred_port: fields.remove("Preferred Port").flatten(),
            reference: fields.remove("Reference").flatten(),
            production_capacity: fields.remove("Production Capacity").flatten(),
            delivery_timeframe: fields.remove("Delivery Timeframe").flatten(),
            incoterms: fields.remove("Incoterms").flatten(),
            packaging_details: fields.remove("Packaging Details").flatten(),
            listing_url: listing_url.to_string(),
            source_platform: "b2brazil".to_string(),
        }
    }
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

    const REAL_SAMPLE_HTML: &str = r#"
        <div id="header-info-thumb"><img src="https://cdn.b2brazil.com/logo.png"></div>
        <div id="header-info-company">
            <h1>Akurat Consultoria Empresarial</h1>
            <div id="header-info-company-verify" class="not-verify">Unverified company</div>
        </div>
        <div class="actions-item"><h4>Since 2013</h4></div>
        <div class="actions-item"><h4>Brazil</h4></div>
        <div class="section-content-more-info-item"><h4>0-10</h4><p>Employees</p></div>
        <div class="section-content-more-info-item"><h4>200K - 500K</h4><p>Sales volume (USD)</p></div>
        <div class="section-content-more-info-item"><h4>10%</h4><p>% Export sales</p></div>
        <h2 class="section-product-title">Precision Microcast Parts - Precision Casting</h2>
        <div class="section-content-about product-description"><p>Real audit description here.</p></div>
        <div class="section-product-item"><h3>Unit Price:</h3><p>Not informed</p></div>
        <div class="section-product-item"><h3>Minimum Order Quantity:</h3><p>500 units</p></div>
    "#;

    #[test]
    fn parse_supplier_extracts_real_company_data() {
        let scraper = B2brazilScraper;
        let result = scraper.parse_supplier(REAL_SAMPLE_HTML, "https://b2brazil.com/test");

        assert_eq!(
            result.company_name,
            Some("Akurat Consultoria Empresarial".to_string())
        );
        assert_eq!(result.year_established, Some("2013".to_string()));
        assert_eq!(result.country, Some("Brazil".to_string()));
        assert_eq!(result.platform_verified_badge, false);
        assert_eq!(result.employee_count, Some("0-10".to_string()));
        assert_eq!(result.sales_revenue, Some("200K - 500K".to_string()));
        assert_eq!(result.export_percentage, Some("10%".to_string()));
    }

    #[test]
    fn parse_listing_filters_out_not_informed_but_keeps_real_values() {
        let scraper = B2brazilScraper;
        let result = scraper.parse_listing(REAL_SAMPLE_HTML, "https://b2brazil.com/test");

        assert_eq!(
            result.title,
            Some("Precision Microcast Parts - Precision Casting".to_string())
        );
        assert_eq!(result.unit_price, None); // "Not informed" correctly filtered out
        assert_eq!(result.minimum_order_quantity, Some("500 units".to_string()));
    }
}
