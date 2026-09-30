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

        // The header shows "Since 2026" then the country. "Since" is the
        // year the supplier JOINED B2Brazil, not the company's founding
        // year, so it is only used here to locate the country that
        // follows it. The real founding year is the "Established" item
        // further down (read below).
        let mut country = None;
        let mut saw_since = false;
        if let Ok(sel) = Selector::parse(".actions-item h4") {
            for el in document.select(&sel) {
                let text = el.text().collect::<String>();
                let trimmed = text.trim();
                if trimmed.starts_with("Since ") {
                    saw_since = true;
                } else if !trimmed.is_empty() && saw_since && country.is_none() {
                    country = Some(trimmed.to_string());
                }
            }
        }

        let (contact_name, contact_phone, contact_location) =
            extract_contact_and_location(&document);
        let contact_name = contact_name.and_then(|n| clean_masked_name(&n));
        let country = contact_location.or(country);

        let mut year_established = None;
        let mut employee_count = None;
        let mut sales_revenue = None;
        let mut export_percentage = None;
        if let Ok(sel) = Selector::parse(".section-content-more-info-item") {
            for item in document.select(&sel) {
                let label =
                    select_text(&Html::parse_fragment(&item.html()), "p").unwrap_or_default();
                let value = select_text(&Html::parse_fragment(&item.html()), "h4");
                match label.as_str() {
                    // "Established" keeps its value in a <span> inside
                    // the icon box; its <h4> just says "Year".
                    "Established" => {
                        year_established = select_text(
                            &Html::parse_fragment(&item.html()),
                            ".section-content-more-info-item-img span",
                        )
                        .filter(|y| y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()));
                    }
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
            contact_name,
            contact_phone,
            badge_honorific: None,
            company_description: None,
            website_url: None,
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

        let mut image_urls = Vec::new();
        if let Ok(img_sel) = Selector::parse("ul.section-product-slider-items li img") {
            for img in document.select(&img_sel).take(3) {
                let value = img.value();
                let real_src = value
                    .attr("data-src")
                    .or_else(|| value.attr("src"))
                    .filter(|src| !src.contains("loading-"));
                if let Some(src) = real_src {
                    image_urls.push(src.to_string());
                }
            }
        }

        B2bListingProfile {
            title,
            description,
            image_urls,
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
    fn extract_company_profile_url(&self, listing_html: &str) -> Option<String> {
        let document = Html::parse_document(listing_html);
        let sel = Selector::parse("a.nav-home").ok()?;
        document
            .select(&sel)
            .next()?
            .value()
            .attr("href")
            .map(|href| {
                if href.starts_with("http") {
                    href.to_string()
                } else {
                    format!("https://b2brazil.com{}", href)
                }
            })
    }

    fn enrich_from_company_profile(
        &self,
        mut supplier: B2bSupplierProfile,
        profile_html: &str,
    ) -> B2bSupplierProfile {
        let document = Html::parse_document(profile_html);
        if let Ok(sel) = Selector::parse(".section-content-about div") {
            if let Some(el) = document.select(&sel).next() {
                let text = el.text().collect::<String>();
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    supplier.company_description = Some(decode_leftover_entities(trimmed));
                }
            }
        }
        supplier
    }

    /// The company's hotsite slug, e.g. "asmetecgmbh" from
    /// "/hotsite/asmetecgmbh" - the same on every product that company
    /// lists. It is the exact value analyze.rs already derived from the
    /// listing URL, so existing B2Brazil seller records keep matching.
    fn company_key(&self, listing_html: &str) -> Option<String> {
        let url = self.extract_company_profile_url(listing_html)?;
        hotsite_slug(&url)
    }
}

/// "https://b2brazil.com/hotsite/asmetecgmbh/tractor" -> "asmetecgmbh".
fn hotsite_slug(url: &str) -> Option<String> {
    let rest = url.split("/hotsite/").nth(1)?;
    let slug = rest
        .split(|c| c == '/' || c == '?' || c == '#')
        .next()?
        .trim();
    if slug.is_empty() {
        None
    } else {
        Some(slug.to_string())
    }
}

/// B2Brazil masks contact names for non-paying visitors, e.g.
/// "Smith ********". Keeps the part the platform does show ("Smith")
/// so it never reaches Claude, the database or the social search with
/// the asterisks in it; a name that is entirely masked becomes None.
fn clean_masked_name(raw: &str) -> Option<String> {
    let cleaned = raw.replace('*', "");
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// B2Brazil double-encodes some characters in company descriptions, so
/// after normal HTML parsing text like "&ldquo;" or "Hot&amp;Cold" is
/// still left behind. Decodes the ones seen on real pages; "&amp;" goes
/// last so it can't create new entities.
fn decode_leftover_entities(text: &str) -> String {
    text.replace("&ldquo;", "\u{201c}")
        .replace("&rdquo;", "\u{201d}")
        .replace("&lsquo;", "\u{2018}")
        .replace("&rsquo;", "\u{2019}")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
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
        <div class="section-content-more-info-item"><div class="section-content-more-info-item-img"><span>2009</span></div><h4>Year</h4><p>Established</p></div>
        <div class="section-content-more-info-item"><h4>0-10</h4><p>Employees</p></div>
        <div class="section-content-more-info-item"><h4>200K - 500K</h4><p>Sales volume (USD)</p></div>
        <div class="section-content-more-info-item"><h4>10%</h4><p>% Export sales</p></div>
        <h2 class="section-product-title">Precision Microcast Parts - Precision Casting</h2>
        <div class="section-content-about product-description"><p>Real audit description here.</p></div>
        <div class="section-product-item"><h3>Unit Price:</h3><p>Not informed</p></div>
        <div class="section-product-item"><h3>Minimum Order Quantity:</h3><p>500 units</p></div>
    "#;

    #[test]
    fn parse_listing_returns_empty_image_list_when_none_present() {
        let scraper = B2brazilScraper;
        let result = scraper.parse_listing(REAL_SAMPLE_HTML, "https://b2brazil.com/test");
        assert_eq!(result.image_urls, Vec::<String>::new());
    }

    const SAMPLE_WITH_IMAGES: &str = r#"
        <h2 class="section-product-title">Test Product</h2>
        <ul class="section-product-slider-items">
            <li><img class="uk-cover" src="https://cdn.b2brazil.com/real-image-1.jpg" alt="product"></li>
            <li><img class="lazyload uk-cover" data-src="https://cdn.b2brazil.com/real-image-2.jpg" src="//cdn.b2brazil.com/assets/images/loading-aH4uwG80c9b336.svg" alt="product"></li>
        </ul>
    "#;

    #[test]
    fn parse_listing_extracts_real_images_preferring_data_src_over_placeholder() {
        let scraper = B2brazilScraper;
        let result = scraper.parse_listing(SAMPLE_WITH_IMAGES, "https://b2brazil.com/test");
        assert_eq!(
            result.image_urls,
            vec![
                "https://cdn.b2brazil.com/real-image-1.jpg".to_string(),
                "https://cdn.b2brazil.com/real-image-2.jpg".to_string(),
            ]
        );
    }

    #[test]
    fn parse_supplier_extracts_real_company_data() {
        let scraper = B2brazilScraper;
        let result = scraper.parse_supplier(REAL_SAMPLE_HTML, "https://b2brazil.com/test");

        assert_eq!(
            result.company_name,
            Some("Akurat Consultoria Empresarial".to_string())
        );
        // "Since 2013" is when they joined B2Brazil; the founding year is
        // the separate "Established" item.
        assert_eq!(result.year_established, Some("2009".to_string()));
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

    #[test]
    fn matches_platform_correctly_identifies_b2brazil_only() {
        let scraper = B2brazilScraper;
        assert!(scraper.matches_platform("b2brazil"));
        assert!(!scraper.matches_platform("olx"));
        assert!(!scraper.matches_platform("alibaba"));
    }

    const VERIFIED_SAMPLE_HTML: &str = r#"
        <div id="header-info-company">
            <h1>Real Verified Company</h1>
            <div id="header-info-company-verify" class="verified">Verified company</div>
        </div>
    "#;

    #[test]
    fn parse_supplier_correctly_detects_a_genuinely_verified_badge() {
        let scraper = B2brazilScraper;
        let result = scraper.parse_supplier(VERIFIED_SAMPLE_HTML, "https://b2brazil.com/test");
        assert_eq!(result.platform_verified_badge, true);
    }

    const INCOTERMS_NO_P_TAG_HTML: &str = r#"
        <div class="section-product-item">
            <h3>Incoterms:</h3>
            FOB
        </div>
    "#;

    #[test]
    fn parse_listing_handles_incoterms_with_no_p_tag_wrapper() {
        let scraper = B2brazilScraper;
        let result = scraper.parse_listing(INCOTERMS_NO_P_TAG_HTML, "https://b2brazil.com/test");
        assert_eq!(result.incoterms, Some("FOB".to_string()));
    }

    const FIVE_IMAGES_HTML: &str = r#"
        <ul class="section-product-slider-items">
            <li><img src="https://cdn.b2brazil.com/img1.jpg"></li>
            <li><img src="https://cdn.b2brazil.com/img2.jpg"></li>
            <li><img src="https://cdn.b2brazil.com/img3.jpg"></li>
            <li><img src="https://cdn.b2brazil.com/img4.jpg"></li>
            <li><img src="https://cdn.b2brazil.com/img5.jpg"></li>
        </ul>
    "#;

    #[test]
    fn parse_listing_caps_image_urls_at_three_even_when_more_are_present() {
        let scraper = B2brazilScraper;
        let result = scraper.parse_listing(FIVE_IMAGES_HTML, "https://b2brazil.com/test");
        assert_eq!(result.image_urls.len(), 3);
    }

    #[test]
    fn parse_supplier_and_listing_do_not_panic_on_genuinely_empty_html() {
        let scraper = B2brazilScraper;
        let supplier = scraper.parse_supplier("", "https://b2brazil.com/test");
        let listing = scraper.parse_listing("", "https://b2brazil.com/test");
        assert_eq!(supplier.company_name, None);
        assert_eq!(listing.title, None);
    }

    #[test]
    fn parse_supplier_and_listing_correctly_record_the_real_url_and_platform() {
        let scraper = B2brazilScraper;
        let supplier =
            scraper.parse_supplier(REAL_SAMPLE_HTML, "https://b2brazil.com/real-profile");
        let listing = scraper.parse_listing(REAL_SAMPLE_HTML, "https://b2brazil.com/real-listing");
        assert_eq!(supplier.profile_url, "https://b2brazil.com/real-profile");
        assert_eq!(supplier.source_platform, "b2brazil");
        assert_eq!(listing.listing_url, "https://b2brazil.com/real-listing");
        assert_eq!(listing.source_platform, "b2brazil");
    }

    #[test]
    fn since_year_alone_is_not_used_as_founding_year() {
        let html = r#"<div class="actions-item"><h4>Since 2026</h4></div><div class="actions-item"><h4>Germany</h4></div>"#;
        let s = B2brazilScraper.parse_supplier(html, "u");
        assert_eq!(s.year_established, None);
        assert_eq!(s.country.as_deref(), Some("Germany"));
    }

    #[test]
    fn masked_contact_name_keeps_only_the_visible_part() {
        let html = r#"<ul class="section-content-more-info-list">
            <li><img data-src="//cdn.b2brazil.com/assets/images/user-icon-x.svg"> Smith ********</li>
            <li><img data-src="//cdn.b2brazil.com/assets/images/phone-x.svg"> +49  1********</li>
            <li><img data-src="//cdn.b2brazil.com/assets/images/map-marker-x.svg"> Kirchheimbolanden / Rheinland-Pfalz | Germany</li>
        </ul>"#;
        let s = B2brazilScraper.parse_supplier(html, "u");
        assert_eq!(s.contact_name.as_deref(), Some("Smith"));
        assert_eq!(s.contact_phone, None);
        assert_eq!(
            s.country.as_deref(),
            Some("Kirchheimbolanden / Rheinland-Pfalz | Germany")
        );
    }

    #[test]
    fn fully_masked_contact_name_is_none() {
        assert_eq!(clean_masked_name("********"), None);
        assert_eq!(
            clean_masked_name("  SHOUNAN   ******** "),
            Some("SHOUNAN".to_string())
        );
    }

    #[test]
    fn company_key_is_the_hotsite_slug() {
        let html = r#"<a class="nav-home" href="/hotsite/asmetecgmbh">Home</a>"#;
        assert_eq!(
            B2brazilScraper.company_key(html).as_deref(),
            Some("asmetecgmbh")
        );
        assert_eq!(
            hotsite_slug(
                "https://en.b2colombia.com/hotsite/yanbianstatexurong2/hair-styling-wand?x=1"
            )
            .as_deref(),
            Some("yanbianstatexurong2")
        );
        assert_eq!(hotsite_slug("https://b2brazil.com/plans"), None);
    }

    #[test]
    fn leftover_entities_are_decoded() {
        assert_eq!(
            decode_leftover_entities("&ldquo;Hot&amp;Cold&rdquo; China&#39;s"),
            "\u{201c}Hot&Cold\u{201d} China's"
        );
    }
}

/// Extracts the real, direct contact details from B2Brazil's "Contact
/// and location" block. Genuinely more specific than the founding-year
/// section's location (which only ever gives a country, e.g. "Brazil")
/// - this block gives a real city and state, e.g. "MACAE / RJ".
///
/// The phone number here is often deliberately masked by B2Brazil
/// itself (e.g. "+55 22********") until a paid tier unlocks it - a
/// masked, partial number has no real value for identity matching, so
/// it's deliberately treated the same as "not present" here, rather
/// than capturing a useless fragment.
fn extract_contact_and_location(
    document: &Html,
) -> (Option<String>, Option<String>, Option<String>) {
    let mut contact_name = None;
    let mut contact_phone = None;
    let mut location = None;

    if let Ok(li_sel) = Selector::parse("ul.section-content-more-info-list li") {
        for li in document.select(&li_sel) {
            let fragment = Html::parse_fragment(&li.html());
            let text = fragment
                .root_element()
                .text()
                .collect::<String>()
                .trim()
                .to_string();
            if text.is_empty() {
                continue;
            }
            let html_lower = li.html().to_lowercase();
            if html_lower.contains("user-icon") {
                contact_name = Some(text);
            } else if html_lower.contains("phone-") {
                if !text.contains('*') {
                    contact_phone = Some(text);
                }
            } else if html_lower.contains("map-marker") {
                location = Some(text);
            }
        }
    }
    (contact_name, contact_phone, location)
}
