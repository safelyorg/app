use crate::services::b2b_scrapers::{
    B2bListingProfile, B2bScraper, B2bSupplierProfile, SupplierRecord,
};
use chrono::{Datelike, Utc};
use scraper::{ElementRef, Html, Selector};

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
        // 1. The "Home" link in the company's hotsite menu.
        if let Some(href) = select_attr(&document, "a.nav-home", "href") {
            return Some(absolute_b2brazil_url(&href));
        }
        // 2. Some product pages have no "Home" link. The company page is
        //    still the /hotsite/{company}/ part of any hotsite link or of
        //    the page's own address, so it is rebuilt from that. Without
        //    this, the seller silently became one record per product.
        let fallbacks = [
            select_attr(&document, r#"link[rel="canonical"]"#, "href"),
            select_attr(&document, r#"meta[property="og:url"]"#, "content"),
            select_attr(&document, r#"a[href*="/hotsite/"]"#, "href"),
        ];
        fallbacks
            .into_iter()
            .flatten()
            .find_map(|href| hotsite_home_url(&absolute_b2brazil_url(&href)))
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

    /// What the company lists about itself on the product page: the
    /// year it joined B2Brazil ("Since 2024"), its keywords, business
    /// types and its other products. None when the page shows none.
    fn supplier_record(&self, listing_html: &str) -> Option<SupplierRecord> {
        let document = Html::parse_document(listing_html);
        let slug = self.company_key(listing_html);
        let mut record = new_record();
        add_company_lists(&mut record, &document, slug.as_deref());
        has_company_lists(&record).then_some(record)
    }

    /// The company page also shows the certificates and the company's
    /// products, so they are added to what the product page gave.
    fn enrich_record_from_company_profile(
        &self,
        record: Option<SupplierRecord>,
        profile_html: &str,
    ) -> Option<SupplierRecord> {
        let document = Html::parse_document(profile_html);
        let slug = self.company_key(profile_html);
        let mut record = record.unwrap_or_else(new_record);
        add_company_lists(&mut record, &document, slug.as_deref());
        if let Ok(sel) = Selector::parse(".section-content-certificates-item img") {
            for img in document.select(&sel) {
                let value = img.value();
                let src = value
                    .attr("data-src")
                    .or_else(|| value.attr("src"))
                    .filter(|src| !src.contains("loading-"));
                if let Some(src) = src {
                    push_unique(&mut record.certificate_images, src.trim());
                }
            }
        }
        has_company_lists(&record).then_some(record)
    }
}

fn new_record() -> SupplierRecord {
    SupplierRecord {
        platform: "B2Brazil".to_string(),
        ..Default::default()
    }
}

fn has_company_lists(record: &SupplierRecord) -> bool {
    record.joined_platform_year.is_some()
        || !record.products_offered.is_empty()
        || !record.business_types.is_empty()
        || !record.certificate_images.is_empty()
}

/// Reads the lists the product page and the company page both show:
/// "Since 2024" (the year the company joined B2Brazil), the business
/// types, the keywords and the company's own products. Only products
/// linking to this company's hotsite are taken - other companies'
/// products are never mixed in.
fn add_company_lists(record: &mut SupplierRecord, document: &Html, slug: Option<&str>) {
    if record.joined_platform_year.is_none() {
        record.joined_platform_year = joined_year(document);
        record.years_on_platform = record
            .joined_platform_year
            .and_then(|y| u32::try_from(Utc::now().year() - y).ok());
    }
    for kind in list_after_heading(document, "Business type") {
        push_unique(&mut record.business_types, &kind);
    }
    for keyword in list_after_heading(document, "Keywords") {
        push_unique(&mut record.products_offered, &keyword);
    }
    let Some(slug) = slug else {
        return;
    };
    let own_link = format!("/hotsite/{}/", slug);
    if let (Ok(link_sel), Ok(name_sel)) = (
        Selector::parse("a.section-products-content-item"),
        Selector::parse("h3"),
    ) {
        for link in document.select(&link_sel) {
            let href = link.value().attr("href").unwrap_or("");
            if !href.contains(&own_link) {
                continue;
            }
            if let Some(name) = link.select(&name_sel).next() {
                let name = name.text().collect::<String>();
                push_unique(&mut record.products_offered, name.trim());
            }
        }
    }
}

/// "Since 2024" in the page header -> 2024.
fn joined_year(document: &Html) -> Option<i32> {
    let sel = Selector::parse(".actions-item h4").ok()?;
    document.select(&sel).find_map(|el| {
        let text = el.text().collect::<String>();
        let year = text.trim().strip_prefix("Since ")?.trim();
        (year.len() == 4)
            .then(|| year.parse::<i32>().ok())
            .flatten()
    })
}

/// The items of the list that follows a heading, e.g. the keywords
/// under <h5>Keywords</h5>.
fn list_after_heading(document: &Html, heading: &str) -> Vec<String> {
    let (Ok(h5), Ok(li)) = (Selector::parse("h5"), Selector::parse("li")) else {
        return Vec::new();
    };
    let Some(list) = document
        .select(&h5)
        .find(|el| {
            el.text()
                .collect::<String>()
                .trim()
                .eq_ignore_ascii_case(heading)
        })
        .and_then(|el| el.next_siblings().find_map(ElementRef::wrap))
    else {
        return Vec::new();
    };
    list.select(&li)
        .map(|item| item.text().collect::<String>().trim().to_string())
        .filter(|text| !text.is_empty())
        .collect()
}

fn push_unique(list: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if !value.is_empty() && !list.iter().any(|v| v.eq_ignore_ascii_case(value)) {
        list.push(value.to_string());
    }
}

fn absolute_b2brazil_url(href: &str) -> String {
    let href = href.trim();
    if href.starts_with("http") {
        href.to_string()
    } else {
        format!("https://b2brazil.com{}", href)
    }
}

/// "https://b2brazil.com/hotsite/siltimodapraia/some-product" ->
/// "https://b2brazil.com/hotsite/siltimodapraia" (keeps the site, so
/// b2colombia and the other sister sites work too).
fn hotsite_home_url(url: &str) -> Option<String> {
    let origin = url.split("/hotsite/").next()?;
    let slug = hotsite_slug(url)?;
    Some(format!("{}/hotsite/{}", origin, slug))
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
    remove_typed_line_breaks(&decode_entities_only(text))
}

/// Some sellers' text contains "<br />" typed in as visible text (B2Brazil
/// shows it literally on the page). The real line break is already there
/// next to it, so the leftover tag text is removed and blank lines are
/// kept to at most one.
fn remove_typed_line_breaks(text: &str) -> String {
    let mut out = text.to_string();
    for tag in ["<br />", "<br/>", "<br>", "<BR />", "<BR/>", "<BR>"] {
        out = out.replace(tag, "\n");
    }
    let mut lines: Vec<&str> = Vec::new();
    for line in out.lines().map(str::trim_end) {
        if line.trim().is_empty() && lines.last().is_some_and(|l| l.trim().is_empty()) {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n").trim().to_string()
}

fn decode_entities_only(text: &str) -> String {
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
    fn company_link_is_rebuilt_when_the_home_link_is_missing() {
        let html = r#"<html><head><link rel="canonical" href="https://b2brazil.com/hotsite/siltimodapraia/-set-top-and-legging-fashion-babi-"></head><body></body></html>"#;
        assert_eq!(
            B2brazilScraper.extract_company_profile_url(html).as_deref(),
            Some("https://b2brazil.com/hotsite/siltimodapraia")
        );
        assert_eq!(
            B2brazilScraper.company_key(html).as_deref(),
            Some("siltimodapraia")
        );

        let link_only = r#"<a href="/hotsite/siltimodapraia/products">Products</a>"#;
        assert_eq!(
            B2brazilScraper
                .extract_company_profile_url(link_only)
                .as_deref(),
            Some("https://b2brazil.com/hotsite/siltimodapraia")
        );

        let sister_site = r#"<meta property="og:url" content="https://en.b2colombia.com/hotsite/yanbianstatexurong2/hair-styling-wand">"#;
        assert_eq!(
            B2brazilScraper
                .extract_company_profile_url(sister_site)
                .as_deref(),
            Some("https://en.b2colombia.com/hotsite/yanbianstatexurong2")
        );

        assert_eq!(
            B2brazilScraper.extract_company_profile_url("<a href=\"/plans\">x</a>"),
            None
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

    /// Trimmed from the real DAAKIIYA product and company pages.
    const DAAKIIYA_PRODUCT_PAGE: &str = r#"<html><head>
        <link rel="canonical" href="https://b2brazil.com/hotsite/daakiiya/bethel-nut"></head><body>
        <div class="actions-item"><h4>Since 2024</h4></div>
        <div class="actions-item"><h4>Brazil</h4></div>
        <nav><a href="/hotsite/daakiiya" class="nav-home">Company Information</a></nav>
        <ul class="section-content-more-info-keywords"><li>betel nut</li><li>chewable nut</li></ul>
        <div class="uk-width-1-1 uk-margin-remove-top">
            <h5>Business type</h5>
            <ul class="section-content-more-info-keywords">
                <li>Importer / Trading Company</li>
                <li>Buying Office</li>
                <li>Representative / Agent</li>
            </ul>
        </div>
        <div class="uk-width-1-1 uk-margin-remove-top">
            <h5>Keywords</h5>
            <ul class="section-content-more-info-keywords "><li>SUGAR</li><li>CHICKEN PAWS</li></ul>
        </div>
        <ul>
            <li class="box-product-item"><a href="/hotsite/daakiiya/icumsa-45-sugar" class="section-products-content-item">
                <h3 class="section-products-content-title">ICUMSA 45 Sugar</h3></a></li>
            <li class="box-product-item"><a href="/hotsite/daakiiya/urea-46-fertilizer" class="section-products-content-item">
                <h3 class="section-products-content-title">Urea 46 Fertilizer</h3></a></li>
            <li class="box-product-item"><a href="/hotsite/othercompany/olive-oil" class="section-products-content-item">
                <h3 class="section-products-content-title">Olive Oil</h3></a></li>
        </ul>
    </body></html>"#;

    const DAAKIIYA_COMPANY_PAGE: &str = r#"<html><head>
        <link rel="canonical" href="https://b2brazil.com/hotsite/daakiiya"></head><body>
        <div class="actions-item"><h4>Since 2024</h4></div>
        <a href="/hotsite/daakiiya/cod-fish" class="section-products-content-item">
            <div class="section-products-content-item-img"><img src="x.webp"></div>
            <h3>Cod Fish</h3></a>
        <div class="section-content-certificates-item">
            <div class="section-content-certificates-img">
                <img class="lazyload" data-src="https://cdn.b2brazil.com/certs/395_sgssystemcertiso90012000-13bb15.jpg.webp" src="//cdn.b2brazil.com/assets/images/loading-aH4uwG80c9b336.svg">
            </div>
        </div>
        <div class="section-content-certificates-item">
            <div class="section-content-certificates-img">
                <img class="lazyload" data-src="https://cdn.b2brazil.com/certs/png-transparent-halal-logo-e3ed98.png.webp" src="//cdn.b2brazil.com/assets/images/loading-aH4uwG80c9b336.svg">
            </div>
        </div>
    </body></html>"#;

    #[test]
    fn record_reads_the_company_lists_from_the_product_page() {
        let r = B2brazilScraper
            .supplier_record(DAAKIIYA_PRODUCT_PAGE)
            .expect("record");
        assert_eq!(r.platform, "B2Brazil");
        assert_eq!(r.joined_platform_year, Some(2024));
        assert!(r.years_on_platform.is_some());
        assert_eq!(
            r.business_types,
            vec![
                "Importer / Trading Company",
                "Buying Office",
                "Representative / Agent"
            ]
        );
        // Keywords and this company's own products; never the product
        // description keywords ("betel nut") or another company's product.
        assert_eq!(
            r.products_offered,
            vec![
                "SUGAR",
                "CHICKEN PAWS",
                "ICUMSA 45 Sugar",
                "Urea 46 Fertilizer"
            ]
        );
        assert!(r.certificate_images.is_empty());
        assert!(!r.checked_by_platform);
        assert!(r.member_label.is_none());
    }

    #[test]
    fn company_page_adds_certificates_and_products() {
        let r = B2brazilScraper.supplier_record(DAAKIIYA_PRODUCT_PAGE);
        let r = B2brazilScraper
            .enrich_record_from_company_profile(r, DAAKIIYA_COMPANY_PAGE)
            .expect("record");
        assert!(r.products_offered.contains(&"Cod Fish".to_string()));
        assert_eq!(
            r.certificate_images,
            vec![
                "https://cdn.b2brazil.com/certs/395_sgssystemcertiso90012000-13bb15.jpg.webp",
                "https://cdn.b2brazil.com/certs/png-transparent-halal-logo-e3ed98.png.webp"
            ]
        );
        // The company page alone (product page record missing) still works.
        let alone = B2brazilScraper
            .enrich_record_from_company_profile(None, DAAKIIYA_COMPANY_PAGE)
            .expect("record");
        assert_eq!(alone.certificate_images.len(), 2);
        assert_eq!(alone.joined_platform_year, Some(2024));
    }

    #[test]
    fn a_page_without_company_lists_has_no_record() {
        let plain = r#"<div id="header-info-company"><h1>Plain Co</h1></div>"#;
        assert!(B2brazilScraper.supplier_record(plain).is_none());
        assert!(B2brazilScraper.supplier_record("").is_none());
    }

    #[test]
    fn leftover_entities_are_decoded() {
        assert_eq!(
            decode_leftover_entities("&ldquo;Hot&amp;Cold&rdquo; China&#39;s"),
            "\u{201c}Hot&Cold\u{201d} China's"
        );
    }

    #[test]
    fn typed_line_break_tags_are_removed() {
        let text = "First paragraph.<br />\n<br />\n\nSecond paragraph.";
        assert_eq!(
            decode_leftover_entities(text),
            "First paragraph.\n\nSecond paragraph."
        );
        assert_eq!(
            decode_leftover_entities("No tags &amp; fine"),
            "No tags & fine"
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
