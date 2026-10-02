use super::{B2bListingProfile, B2bScraper, B2bSupplierProfile};
use scraper::{ElementRef, Html, Selector};

pub struct KompassScraper;

fn text_of(el: &ElementRef) -> String {
    el.text().collect::<Vec<_>>().join(" ")
}

fn clean_optional_text(raw: &str) -> Option<String> {
    let trimmed: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// The real Kompass header h1 (`h1[itemprop=name].titleGeneral`) is
/// shared by both the company page and the product page, but on the
/// company page it also carries a trailing
/// `<span class="titleGMini">(activity, city)</span>` describing the
/// company's Kompass classification - not part of the actual name.
/// That span is removed by element, NOT by cutting at the first '(',
/// because real names contain brackets too, e.g.
/// "Chenyue(Jiangsu)Technology Co.,Ltd." (cutting at '(' gave "Chenyue").
fn parse_company_name(document: &Html) -> Option<String> {
    let selector = Selector::parse("h1[itemprop=name]").ok()?;
    let mini = Selector::parse(".titleGMini").ok()?;
    let h1 = document.select(&selector).next()?;
    let mut name = text_of(&h1);
    for span in h1.select(&mini) {
        name = name.replacen(&text_of(&span), "", 1);
    }
    clean_optional_text(&name)
}

/// Reads a value out of Kompass's real `<tr><th>Label</th><td>Value</td></tr>`
/// info tables (General / Legal) - used for both, since both share
/// this exact shape. Matches the label ignoring case and any trailing
/// whitespace so "No employees" doesn't also match "No employees
/// (address)".
fn find_table_value(document: &Html, table_selector: &str, label: &str) -> Option<String> {
    let table_sel = Selector::parse(table_selector).ok()?;
    let row_sel = Selector::parse("tr").ok()?;
    let th_sel = Selector::parse("th").ok()?;
    let td_sel = Selector::parse("td").ok()?;

    for table in document.select(&table_sel) {
        for row in table.select(&row_sel) {
            let Some(th) = row.select(&th_sel).next() else {
                continue;
            };
            let Some(td) = row.select(&td_sel).next() else {
                continue;
            };
            let row_label = text_of(&th).trim().to_string();
            if row_label.eq_ignore_ascii_case(label) {
                return clean_optional_text(&text_of(&td));
            }
        }
    }
    None
}

/// The real "Manufacturer / Distributor / Service / Importer /
/// Exporter" tags live as `.tag.tagOrange` spans inside the SAME
/// `.rowHead` div as the "Verified company" marker - but
/// `.tag.tagOrange` is reused elsewhere on the page (keywords,
/// classification tree), so this scopes to that one specific row by
/// walking up from `#isKompassYear` to its parent first.
fn parse_business_type_tags(document: &Html) -> Option<String> {
    let marker_sel = Selector::parse("#isKompassYear").ok()?;
    let tag_sel = Selector::parse(".tag.tagOrange").ok()?;

    let marker = document.select(&marker_sel).next()?;
    let row = marker.parent().and_then(ElementRef::wrap)?;

    let tags: Vec<String> = row
        .select(&tag_sel)
        .map(|el| text_of(&el).trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();

    if tags.is_empty() {
        None
    } else {
        Some(tags.join(", "))
    }
}

/// Locality + country (e.g. "Neuss, Germany") from the schema.org
/// address microdata - present identically on both the company page
/// and the product page header.
fn parse_location(document: &Html) -> Option<String> {
    let locality = Selector::parse("span[itemprop=addressLocality]")
        .ok()
        .and_then(|s| document.select(&s).next())
        .map(|el| text_of(&el).trim().to_string())
        .filter(|t| !t.is_empty());

    let country = Selector::parse("span[itemprop=addressCountry]")
        .ok()
        .and_then(|s| document.select(&s).next())
        .map(|el| text_of(&el).trim().to_string())
        .filter(|t| !t.is_empty());

    match (locality, country) {
        // Some companies type the country into the city field too, e.g.
        // "Shuyang County,Jiangsu Province.China" + "China". The repeat
        // is dropped so it reads "..., Jiangsu Province, China".
        (Some(l), Some(c)) if l.to_lowercase().ends_with(&c.to_lowercase()) => {
            let cut = l.len() - c.len();
            let head =
                l[..cut].trim_end_matches(|ch: char| ch == '.' || ch == ',' || ch.is_whitespace());
            if head.is_empty() {
                Some(c)
            } else {
                Some(format!("{head}, {c}"))
            }
        }
        (Some(l), Some(c)) => Some(format!("{l}, {c}")),
        (None, Some(c)) => Some(c),
        (Some(l), None) => Some(l),
        (None, None) => None,
    }
}

/// The real, ungated phone number - the input's id embeds the
/// company's Kompass ID (e.g.
/// `freePhone-contactCompanyForCompany-DE602025`), which differs per
/// company, so this matches on the id PREFIX rather than a fixed id.
fn parse_contact_phone(document: &Html) -> Option<String> {
    let selector = Selector::parse("input[id^='freePhone-contactCompanyForCompany-']").ok()?;
    document
        .select(&selector)
        .next()
        .and_then(|el| el.value().attr("value"))
        .and_then(clean_optional_text)
}

/// Membership tier (e.g. "Booster International", "Booster") - the
/// real text sits in the `tr.trAdhesion` row's anchor, not plain
/// table text, since the cell also contains a badge icon.
fn parse_membership_tier(document: &Html) -> Option<String> {
    let selector = Selector::parse("tr.trAdhesion td a.infoJuridicBooster").ok()?;
    document
        .select(&selector)
        .next()
        .map(|el| text_of(&el))
        .and_then(|t| clean_optional_text(&t))
}

/// The first executive's plain name (e.g. "Herr Yannick Koch") - the
/// closest real match to a single contact person Kompass offers.
fn parse_first_executive_name(document: &Html) -> Option<String> {
    let selector = Selector::parse(".executiveBlock .executiveName p strong").ok()?;
    document
        .select(&selector)
        .next()
        .map(|el| text_of(&el))
        .and_then(|t| clean_optional_text(&t))
}

/// The company website. The company page lists it as
/// `a#webSite_presentation_0`; product pages show the same link in a
/// "Company website" block (`.webSite a`). Both are read, so the website
/// is found even when only the product page could be fetched.
fn parse_website_url(document: &Html) -> Option<String> {
    [
        "a[id^='webSite_presentation_']",
        ".blockSectionProduct .webSite a[href]",
    ]
    .iter()
    .filter_map(|s| Selector::parse(s).ok())
    .find_map(|sel| {
        document
            .select(&sel)
            .next()
            .and_then(|el| el.value().attr("href"))
            .and_then(clean_optional_text)
    })
    .filter(|u| u.starts_with("http"))
}

fn parse_description(document: &Html) -> Option<String> {
    let selector = Selector::parse("#description").ok()?;
    document
        .select(&selector)
        .next()
        .map(|el| text_of(&el))
        .and_then(|t| clean_optional_text(&t))
}

fn parse_logo_url(document: &Html) -> Option<String> {
    let selector = Selector::parse("#companyLogo").ok()?;
    document
        .select(&selector)
        .next()
        .and_then(|el| el.value().attr("src"))
        .and_then(clean_optional_text)
}

/// Reads the flat Type/Model/Dimension/Brand/Certification/Origin
/// key-value pairs from a product's "Characteristics" list - present
/// on some product pages (e.g. Lisheng's) and absent on others (e.g.
/// Agilon's), and the actual set of keys varies by product category,
/// so this returns whatever pairs genuinely exist rather than
/// assuming a fixed set.
fn parse_characteristics(document: &Html) -> Vec<(String, String)> {
    let Ok(li_sel) = Selector::parse(".informationsProduct li") else {
        return Vec::new();
    };
    let Ok(title_sel) = Selector::parse(".spTitle") else {
        return Vec::new();
    };
    let Ok(value_sel) = Selector::parse(".spTexte") else {
        return Vec::new();
    };

    document
        .select(&li_sel)
        .filter_map(|li| {
            let title = li.select(&title_sel).next().map(|el| text_of(&el))?;
            let value = li.select(&value_sel).next().map(|el| text_of(&el))?;
            let title = clean_optional_text(&title)?;
            let value = clean_optional_text(&value)?;
            Some((title, value))
        })
        .collect()
}

/// The Kompass company ID from any Kompass company or product link,
/// e.g. "/c/accurate-industrial-products/in489756/" or
/// "/p/accurate-industrial-products/in489756/diamond-.../3141.../" ->
/// "in489756". It is the second path part after "/c/" or "/p/", and is
/// the same on every product page of that company.
fn company_id_from_url(url: &str) -> Option<String> {
    let path = url.split("kompass.com").last().unwrap_or(url);
    let mut parts = path
        .split(|c| c == '/' || c == '?' || c == '#')
        .filter(|p| !p.is_empty());
    let kind = parts.next()?;
    if kind != "c" && kind != "p" {
        return None;
    }
    let _slug = parts.next()?;
    let id = parts.next()?.trim().to_lowercase();
    // Real IDs are letters + digits (e.g. "in489756", "de602025").
    if id.len() >= 4
        && id.chars().all(|c| c.is_ascii_alphanumeric())
        && id.chars().any(|c| c.is_ascii_digit())
    {
        Some(id)
    } else {
        None
    }
}

impl B2bScraper for KompassScraper {
    fn matches_platform(&self, platform: &str) -> bool {
        platform == "kompass"
    }

    fn parse_supplier(&self, html: &str, profile_url: &str) -> B2bSupplierProfile {
        let document = Html::parse_document(html);

        let platform_verified_badge = Selector::parse("#isKompassYear .text")
            .ok()
            .and_then(|s| document.select(&s).next())
            .map(|el| text_of(&el).to_lowercase().contains("verified"))
            .unwrap_or(false);

        B2bSupplierProfile {
            company_name: parse_company_name(&document),
            logo_url: parse_logo_url(&document),
            year_established: find_table_value(
                &document,
                "table.tableInfoPlus",
                "Year established",
            ),
            country: parse_location(&document),
            platform_verified_badge,
            employee_count: find_table_value(&document, "table.tableInfoPlus", "No employees"),
            sales_revenue: None, // Kompass has no sales/revenue field anywhere
            export_percentage: None, // Kompass only gives "Export area" (region text), not a percentage
            profile_url: profile_url.to_string(),
            source_platform: "kompass".to_string(),
            contact_name: parse_first_executive_name(&document),
            contact_phone: parse_contact_phone(&document),
            badge_honorific: parse_membership_tier(&document)
                .or_else(|| parse_business_type_tags(&document)),
            company_description: parse_description(&document),
            website_url: parse_website_url(&document),
        }
    }

    /// The richer company profile ("About us") page is linked from
    /// the product page's nav as a relative path
    /// (`/c/{company-slug}/{kompass-id}/`) - resolved here to an
    /// absolute kompass.com URL.
    fn extract_company_profile_url(&self, listing_html: &str) -> Option<String> {
        let document = Html::parse_document(listing_html);
        let selector = Selector::parse("nav#productNav a[title='See the company']").ok()?;
        let href = document
            .select(&selector)
            .next()?
            .value()
            .attr("href")?
            .to_string();

        if href.starts_with("http") {
            Some(href)
        } else {
            Some(format!("https://www.kompass.com{href}"))
        }
    }

    /// One record per company: the Kompass company ID (e.g.
    /// "in489756"), read from the product page's "See the company"
    /// link, or from the page's own canonical link. The same for every
    /// product that company lists, so Safely history and fraud reports
    /// belong to the company, not to one product.
    fn company_key(&self, listing_html: &str) -> Option<String> {
        self.extract_company_profile_url(listing_html)
            .as_deref()
            .and_then(company_id_from_url)
            .or_else(|| {
                let document = Html::parse_document(listing_html);
                Selector::parse("link[rel='canonical']")
                    .ok()
                    .and_then(|sel| document.select(&sel).next())
                    .and_then(|el| el.value().attr("href"))
                    .and_then(company_id_from_url)
            })
    }

    /// Company page has richer data than the product page's own
    /// embedded header: full description, employee count, membership
    /// tier, year established, website and executive name are all
    /// re-read here and only fill in gaps left by parse_supplier
    /// (which already captured the real phone number and business
    /// type tags directly from the product page header).
    fn enrich_from_company_profile(
        &self,
        mut supplier: B2bSupplierProfile,
        profile_html: &str,
    ) -> B2bSupplierProfile {
        let document = Html::parse_document(profile_html);

        if supplier.company_description.is_none() {
            supplier.company_description = parse_description(&document);
        }
        if supplier.employee_count.is_none() {
            supplier.employee_count =
                find_table_value(&document, "table.tableInfoPlus", "No employees");
        }
        if supplier.year_established.is_none() {
            supplier.year_established =
                find_table_value(&document, "table.tableInfoPlus", "Year established");
        }
        if supplier.contact_name.is_none() {
            supplier.contact_name = parse_first_executive_name(&document);
        }
        if supplier.website_url.is_none() {
            supplier.website_url = parse_website_url(&document);
        }
        if supplier.logo_url.is_none() {
            supplier.logo_url = parse_logo_url(&document);
        }
        if supplier.badge_honorific.is_none() {
            supplier.badge_honorific =
                parse_membership_tier(&document).or_else(|| parse_business_type_tags(&document));
        }
        if !supplier.platform_verified_badge {
            supplier.platform_verified_badge = Selector::parse("#isKompassYear .text")
                .ok()
                .and_then(|s| document.select(&s).next())
                .map(|el| text_of(&el).to_lowercase().contains("verified"))
                .unwrap_or(false);
        }

        supplier
    }

    fn parse_listing(&self, html: &str, listing_url: &str) -> B2bListingProfile {
        let document = Html::parse_document(html);

        let title = Selector::parse(".productTitle h1")
            .ok()
            .and_then(|s| document.select(&s).next())
            .map(|el| text_of(&el))
            .and_then(|t| clean_optional_text(&t));

        let base_description = Selector::parse(".blockSectionProduct.description-text")
            .ok()
            .and_then(|s| document.select(&s).next())
            .map(|el| text_of(&el))
            .and_then(|t| clean_optional_text(&t));

        let characteristics = parse_characteristics(&document);
        let reference = characteristics
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Model"))
            .map(|(_, v)| v.clone());

        // No dedicated field exists on B2bListingProfile for the
        // free-form Type/Model/Dimension/Brand/Certification/Origin
        // pairs, so they're appended to the description as a
        // readable trailer rather than dropped - keeps every real
        // value available without inventing new struct fields.
        let description = if characteristics.is_empty() {
            base_description
        } else {
            let pairs = characteristics
                .iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect::<Vec<_>>()
                .join("; ");
            let trailer = format!("Characteristics: {pairs}");
            match base_description {
                Some(desc) => Some(format!("{desc} {trailer}")),
                None => Some(trailer),
            }
        };

        // Kompass loads product photos lazily: the real link is in
        // `data-src`, and `src` is often missing or a placeholder. Both
        // are read; placeholders (data: URIs) and repeats are skipped.
        let mut image_urls: Vec<String> = Vec::new();
        if let Ok(sel) = Selector::parse("#productPageCarousel li.itemPicture img") {
            for el in document.select(&sel) {
                let url = ["data-src", "src"]
                    .iter()
                    .filter_map(|a| el.value().attr(a))
                    .map(str::trim)
                    .find(|u| u.starts_with("http") || u.starts_with("//"));
                if let Some(url) = url {
                    let url = if url.starts_with("//") {
                        format!("https:{url}")
                    } else {
                        url.to_string()
                    };
                    if !image_urls.contains(&url) {
                        image_urls.push(url);
                    }
                }
            }
        }

        B2bListingProfile {
            title,
            description,
            image_urls,
            // Kompass has no price/trade-term fields anywhere on
            // either page shape - product pages show only the
            // literal text "Price on request", never a real number.
            unit_price: None,
            fob_price: None,
            minimum_order_quantity: None,
            payment_type: None,
            preferred_port: None,
            reference,
            production_capacity: None,
            delivery_timeframe: None,
            incoterms: None,
            packaging_details: None,
            listing_url: listing_url.to_string(),
            source_platform: "kompass".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product_page_html() -> &'static str {
        r#"
        <html><body>
        <div class="headerCompany">
            <div id="companyDivLogo"><img id="companyLogo" src="https://img.kompass.com/logo.jpg"></div>
            <h1 itemprop="name" class="titleGeneral">Agilon Cables India Private Limited</h1>
            <div class="rowHead">
                <div id="isKompassYear" class="isKompassYear"><span class="text">Verified company</span></div>
                <span class="bull">&bull;</span>
                <span class="tag tagOrange">Manufacturer</span>
                <span class="bull">&bull;</span>
                <span class="tag tagOrange">Exporter</span>
            </div>
            <div itemprop="address">
                <span itemprop="addressLocality">Noida</span>
                <span itemprop="postalCode">201301</span>
                <span itemprop="addressCountry">India</span>
            </div>
            <input id="freePhone-contactCompanyForCompany-IN483016" type="hidden" value="+91 9773793466">
        </div>
        <nav id="productNav">
            <a class="link itemLink" href="/c/agilon-cables-india-private-limited/in483016/" title="See the company">About us</a>
        </nav>
        <div class="productTitle"><h1>Multicore Shielded BMS Cable</h1></div>
        <div class="blockSectionProduct description-text">Flexible cable with HFFR insulation.</div>
        <div id="productPageCarousel">
            <ul>
                <li class="itemPicture active"><img src="https://img.kompass.com/1.jpg"></li>
            </ul>
        </div>
        </body></html>
        "#
    }

    fn product_page_html_with_characteristics() -> &'static str {
        r#"
        <html><body>
        <h1 itemprop="name" class="titleGeneral">Zhejiang Lisheng Spring Co.,Ltd</h1>
        <div class="productTitle"><h1>High-precision flat-end wave-shaped springs</h1></div>
        <div class="blockSectionProduct description-text">Crest-to-crest wave springs.</div>
        <div class="informationsProduct">
            <ul>
                <li><span class="spTitle">Type</span><span class="spTexte">wave spring</span></li>
                <li><span class="spTitle">Model</span><span class="spTexte">LS/LMS</span></li>
                <li><span class="spTitle">Origin</span><span class="spTexte">China</span></li>
            </ul>
        </div>
        </body></html>
        "#
    }

    fn company_page_html() -> &'static str {
        r#"
        <html><body>
        <h1 itemprop="name" class="titleGeneral">BEKO TECHNOLOGIES GmbH<span class="titleGMini">(Metallurgy and mechanical engineering consultants, Neuss)</span></h1>
        <div id="isKompassYear" class="isKompassYear"><span class="text">Verified company</span></div>
        <span class="tag tagOrange">Manufacturer</span>
        <span class="tag tagOrange">Distributor</span>
        <div itemprop="address">
            <span itemprop="addressLocality">Neuss</span>
            <span itemprop="addressCountry">Germany</span>
        </div>
        <div id="description">Modern production technology requires compressed air.</div>
        <table class="tableInfoPlus">
            <tbody>
                <tr class="trWebSite"><th>Discover more on our Website</th><td class="listWww">
                    <div class="webSite-item"><a id="webSite_presentation_0" href="https://www.beko-technologies.com/de/de/">https://www.beko-technologies.com/de/de/</a></div>
                </td></tr>
                <tr><th>Year established</th><td>1980</td></tr>
                <tr><th>No employees (address)</th><td>Not declared</td></tr>
                <tr><th>No employees</th><td>250-499 Employees</td></tr>
                <tr class="trAdhesion"><th>Membership</th><td><a href="https://www.solutions.kompass.com/contactus/booster/" class="infoJuridicBooster">Booster International</a></td></tr>
            </tbody>
        </table>
        <div class="executives">
            <div class="executiveBlock"><div class="executiveName"><p><strong>Herr Yannick  Koch</strong></p></div></div>
        </div>
        </body></html>
        "#
    }

    #[test]
    fn matches_platform_is_true_only_for_kompass() {
        let scraper = KompassScraper;
        assert!(scraper.matches_platform("kompass"));
        assert!(!scraper.matches_platform("b2bmap"));
    }

    #[test]
    fn parse_supplier_reads_the_real_fields_from_a_product_page_header() {
        let scraper = KompassScraper;
        let supplier = scraper.parse_supplier(
            product_page_html(),
            "https://www.kompass.com/p/agilon-cables-india-private-limited/in483016/x/",
        );

        assert_eq!(
            supplier.company_name.as_deref(),
            Some("Agilon Cables India Private Limited")
        );
        assert_eq!(supplier.country.as_deref(), Some("Noida, India"));
        assert!(supplier.platform_verified_badge);
        assert_eq!(
            supplier.badge_honorific.as_deref(),
            Some("Manufacturer, Exporter")
        );
        assert_eq!(supplier.contact_phone.as_deref(), Some("+91 9773793466"));
        assert_eq!(supplier.source_platform, "kompass");
    }

    #[test]
    fn parse_company_name_strips_the_titlegmini_activity_span() {
        let scraper = KompassScraper;
        let supplier = scraper.parse_supplier(
            company_page_html(),
            "https://www.kompass.com/c/beko/de602025/",
        );
        assert_eq!(
            supplier.company_name.as_deref(),
            Some("BEKO TECHNOLOGIES GmbH")
        );
    }

    #[test]
    fn parse_supplier_reads_membership_tier_year_and_employees_from_company_page_tables() {
        let scraper = KompassScraper;
        let supplier = scraper.parse_supplier(
            company_page_html(),
            "https://www.kompass.com/c/beko/de602025/",
        );

        assert_eq!(supplier.year_established.as_deref(), Some("1980"));
        assert_eq!(
            supplier.employee_count.as_deref(),
            Some("250-499 Employees")
        );
        assert_eq!(
            supplier.badge_honorific.as_deref(),
            Some("Booster International"),
            "membership tier should win over business-type tags when both are found"
        );
        assert_eq!(supplier.contact_name.as_deref(), Some("Herr Yannick Koch"));
        assert_eq!(
            supplier.website_url.as_deref(),
            Some("https://www.beko-technologies.com/de/de/")
        );
        assert!(
            supplier
                .company_description
                .as_deref()
                .unwrap()
                .contains("compressed air")
        );
    }

    #[test]
    fn extract_company_profile_url_resolves_the_relative_about_us_link() {
        let scraper = KompassScraper;
        let url = scraper.extract_company_profile_url(product_page_html());
        assert_eq!(
            url.as_deref(),
            Some("https://www.kompass.com/c/agilon-cables-india-private-limited/in483016/")
        );
    }

    #[test]
    fn extract_company_profile_url_is_none_when_the_nav_link_is_missing() {
        let scraper = KompassScraper;
        assert!(
            scraper
                .extract_company_profile_url("<html><body></body></html>")
                .is_none()
        );
    }

    #[test]
    fn enrich_from_company_profile_only_fills_gaps_left_by_the_product_page() {
        let scraper = KompassScraper;
        let supplier = B2bSupplierProfile {
            source_platform: "kompass".to_string(),
            contact_phone: Some("+91 9773793466".to_string()),
            ..Default::default()
        };

        let supplier = scraper.enrich_from_company_profile(supplier, company_page_html());

        assert_eq!(
            supplier.contact_phone.as_deref(),
            Some("+91 9773793466"),
            "phone already captured from the product page must never be overwritten"
        );
        assert_eq!(supplier.year_established.as_deref(), Some("1980"));
        assert_eq!(
            supplier.employee_count.as_deref(),
            Some("250-499 Employees")
        );
        assert_eq!(supplier.contact_name.as_deref(), Some("Herr Yannick Koch"));
    }

    #[test]
    fn parse_listing_reads_title_description_and_images() {
        let scraper = KompassScraper;
        let listing = scraper.parse_listing(
            product_page_html(),
            "https://www.kompass.com/p/agilon-cables-india-private-limited/in483016/x/y/",
        );

        assert_eq!(
            listing.title.as_deref(),
            Some("Multicore Shielded BMS Cable")
        );
        assert!(
            listing
                .description
                .as_deref()
                .unwrap()
                .contains("HFFR insulation")
        );
        assert_eq!(
            listing.image_urls,
            vec!["https://img.kompass.com/1.jpg".to_string()]
        );
        assert_eq!(listing.source_platform, "kompass");
    }

    #[test]
    fn parse_listing_leaves_every_nonexistent_kompass_trade_field_as_none() {
        let scraper = KompassScraper;
        let listing =
            scraper.parse_listing(product_page_html(), "https://www.kompass.com/p/x/y/z/");

        assert!(listing.unit_price.is_none());
        assert!(listing.fob_price.is_none());
        assert!(listing.minimum_order_quantity.is_none());
        assert!(listing.payment_type.is_none());
        assert!(listing.preferred_port.is_none());
        assert!(listing.production_capacity.is_none());
        assert!(listing.delivery_timeframe.is_none());
        assert!(listing.incoterms.is_none());
        assert!(listing.packaging_details.is_none());
    }

    #[test]
    fn parse_listing_folds_characteristics_into_description_and_lifts_model_into_reference() {
        let scraper = KompassScraper;
        let listing = scraper.parse_listing(
            product_page_html_with_characteristics(),
            "https://www.kompass.com/p/zhejiang-lisheng-spring-co-ltd/cnnkc00045342/x/y/",
        );

        assert_eq!(listing.reference.as_deref(), Some("LS/LMS"));
        let description = listing.description.as_deref().unwrap();
        assert!(description.contains("Crest-to-crest wave springs."));
        assert!(description.contains("Type: wave spring"));
        assert!(description.contains("Model: LS/LMS"));
        assert!(description.contains("Origin: China"));
    }

    #[test]
    fn parse_listing_reference_is_none_when_no_characteristics_table_exists() {
        let scraper = KompassScraper;
        let listing =
            scraper.parse_listing(product_page_html(), "https://www.kompass.com/p/x/y/z/");
        assert!(
            listing.reference.is_none(),
            "the Agilon-style product page has no Characteristics table at all"
        );
    }

    #[test]
    fn company_key_is_the_kompass_company_id() {
        assert_eq!(
            KompassScraper.company_key(product_page_html()).as_deref(),
            Some("in483016")
        );
        // Real links (Oct 2026): product and company page give the same ID.
        assert_eq!(
            company_id_from_url("https://www.kompass.com/p/accurate-industrial-products/in489756/diamond-dotted-paper-for-transformer-windings/3141f387-4da5-40ba-8012-09bc0d8d8ee3/").as_deref(),
            Some("in489756")
        );
        assert_eq!(
            company_id_from_url("https://www.kompass.com/c/accurate-industrial-products/in489756/")
                .as_deref(),
            Some("in489756")
        );
        // Broken or unrelated links give nothing.
        assert_eq!(
            company_id_from_url(
                "https://www.kompass.comfr.kompass.storefront.util.CanonicalUrlData@22a50c9b"
            ),
            None
        );
        assert_eq!(company_id_from_url("https://www.kompass.com/login"), None);
        assert_eq!(KompassScraper.company_key("<html></html>"), None);
    }

    #[test]
    fn company_key_falls_back_to_the_canonical_link() {
        let html = r#"<html><head><link rel="canonical" href="https://www.kompass.com/p/acme/de602025/widget/abc/"/></head><body></body></html>"#;
        assert_eq!(
            KompassScraper.company_key(html).as_deref(),
            Some("de602025")
        );
    }

    #[test]
    fn lazy_loaded_product_photos_are_read_from_data_src() {
        // Trimmed from the real KAY International product page (Oct 2026).
        let html = r#"<html><body><div id="productPageCarousel"><ul>
            <li class="itemPicture active"><picture><img data-src="https://img.kompass.com/sys-master-images/hcf/he0/10451305693214/mechanical-vacuum-bosster-png" class="lazyload" src="data:image/gif;base64,R0lGOD"></picture></li>
            <li class="itemPicture"><img data-src="https://img.kompass.com/sys-master-images/hcf/he0/10451305693214/mechanical-vacuum-bosster-png" class="lazyload"></li>
            <li class="itemPicture"><img src="//img.kompass.com/2.jpg"></li>
        </ul></div></body></html>"#;
        let l = KompassScraper.parse_listing(html, "u");
        assert_eq!(
            l.image_urls,
            vec![
                "https://img.kompass.com/sys-master-images/hcf/he0/10451305693214/mechanical-vacuum-bosster-png".to_string(),
                "https://img.kompass.com/2.jpg".to_string(),
            ]
        );
    }

    #[test]
    fn website_is_read_from_the_product_page_too() {
        // Trimmed from the real Jumbo Stillads product page (Oct 2026).
        let html = r#"<html><body><div class='blockSectionProduct'>
            <h2>Company website</h2>
            <div class="webSite"><a href="https://www.jumbo.as" rel="noopener" target="_blank"><span class="blockText">Jumbo Stillads A/S</span></a></div>
        </div></body></html>"#;
        let s = KompassScraper.parse_supplier(html, "u");
        assert_eq!(s.website_url.as_deref(), Some("https://www.jumbo.as"));
    }

    #[test]
    fn company_page_executive_employees_and_website_are_read() {
        // Trimmed from the real Jumbo Stillads company page (Oct 2026).
        let html = r#"<html><body>
        <table class="tableInfoPlus">
            <tr class="trWebSite"><th>Discover more on our Website</th><td class="listWww"><div class="webSite-item"><a id="webSite_presentation_0" href="https://www.jumbo.as" target="_blank">https://www.jumbo.as</a></div></td></tr>
            <tr><th>Year established</th><td></td></tr>
            <tr><th>No employees</th><td>47&nbsp;Employees</td></tr>
        </table>
        <div class="executiveBlock"><div class="executiveName"><p title="Karsten  Skov Hansen"><strong>Karsten  Skov Hansen</strong></p><p class="executiveFonction">CEO - Chief Executive Officer</p></div></div>
        </body></html>"#;
        let s = KompassScraper.enrich_from_company_profile(B2bSupplierProfile::default(), html);
        assert_eq!(s.contact_name.as_deref(), Some("Karsten Skov Hansen"));
        assert_eq!(s.employee_count.as_deref(), Some("47 Employees"));
        assert_eq!(s.website_url.as_deref(), Some("https://www.jumbo.as"));
        assert_eq!(s.year_established, None, "an empty year cell stays empty");
    }

    #[test]
    fn brackets_inside_the_real_company_name_are_kept() {
        // Trimmed from the real Chenyue company and product pages (Oct 2026).
        let company = r#"<html><body><h1 itemprop="name" class="titleGeneral">
            Chenyue&#x28;Jiangsu&#x29;Technology Co.,Ltd.<span class="titleGMini">
                (Industrial gearboxes<span class="virgule">,</span>Shuyang County,Jiangsu Province.China)
            </span></h1></body></html>"#;
        let product = r#"<html><body><h1 itemprop="name" class="titleGeneral">
            Chenyue&#x28;Jiangsu&#x29;Technology Co.,Ltd.</h1></body></html>"#;
        for html in [company, product] {
            let s = KompassScraper.parse_supplier(html, "u");
            assert_eq!(
                s.company_name.as_deref(),
                Some("Chenyue(Jiangsu)Technology Co.,Ltd.")
            );
        }
    }

    #[test]
    fn country_typed_into_the_city_is_not_repeated() {
        let html = r#"<html><body><div itemprop="address">
            <span itemprop="addressLocality">Shuyang County,Jiangsu Province.China</span>
            <span itemprop="addressCountry">China</span></div></body></html>"#;
        let s = KompassScraper.parse_supplier(html, "u");
        assert_eq!(
            s.country.as_deref(),
            Some("Shuyang County,Jiangsu Province, China")
        );
    }
}
