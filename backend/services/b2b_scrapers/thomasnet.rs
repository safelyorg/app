use super::{B2bListingProfile, B2bScraper, B2bSupplierProfile};
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;

pub struct ThomasnetScraper;

/// ThomasNet pages are built with Next.js, which puts the whole
/// company record into one `<script id="__NEXT_DATA__">` JSON block.
/// Returns `props.pageProps.data` (the company) if present. Used only
/// to fill fields the visible page didn't give.
fn next_company_data(document: &Html) -> Option<Value> {
    let sel = Selector::parse("script#__NEXT_DATA__").ok()?;
    let raw = document.select(&sel).next()?.text().collect::<String>();
    let data: Value = serde_json::from_str(&raw).ok()?;
    data.pointer("/props/pageProps/data").cloned()
}

fn json_text(value: &Value, key: &str) -> Option<String> {
    match value.get(key)? {
        Value::String(s) => clean_optional_text(s),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// "Livermore, CA, USA" from the company's address record.
fn json_location(company: &Value) -> Option<String> {
    let address = company.get("address")?;
    let parts: Vec<String> = ["city", "state", "country"]
        .iter()
        .filter_map(|k| json_text(address, k))
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

/// The company part of a ThomasNet profile link, e.g.
/// ".../company/t-k-machine-30682072/profile?heading=1" ->
/// "t-k-machine-30682072". The same for every category page of that
/// company, so it is used as the company's record key.
fn company_slug_from_url(url: &str) -> Option<String> {
    let after = url.split("/company/").nth(1)?;
    let slug = after
        .split(|c| c == '/' || c == '?' || c == '#')
        .next()?
        .trim();
    if slug.is_empty() {
        None
    } else {
        Some(slug.to_lowercase())
    }
}

/// Turns any of ThomasNet's three real "empty" shapes (a genuine
/// value, the literal text "Not available", or a fully empty /
/// whitespace element) into one consistent None - confirmed
/// necessary across several real profile pages, where all three
/// patterns showed up for otherwise-equivalent fields.
fn clean_optional_text(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("not available") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn text_of(el: &ElementRef) -> String {
    el.text().collect::<Vec<_>>().join(" ")
}

/// Reads a "Business Details" grid field by its exact label text
/// (e.g. "Year Founded", "No of Employees") - the shared shape every
/// one of these fields uses. The real value lives in the label's own
/// parent's first <ul>.
fn parse_labeled_field(document: &Html, label: &str) -> Option<String> {
    let label_selector = Selector::parse("div.txt-label").ok()?;
    let ul_selector = Selector::parse("ul").ok()?;

    for label_el in document.select(&label_selector) {
        let label_text = text_of(&label_el);
        if label_text.trim().trim_end_matches(':') != label {
            continue;
        }
        if let Some(parent) = label_el.parent().and_then(ElementRef::wrap) {
            if let Some(ul) = parent.select(&ul_selector).next() {
                return clean_optional_text(&text_of(&ul));
            }
        }
    }
    None
}

/// Like parse_labeled_field, but returns only the FIRST <li> under the
/// label, so a list (e.g. several key personnel) gives one entry, not
/// all of them run together. Extra spaces are squeezed out.
fn first_labeled_item(document: &Html, label: &str) -> Option<String> {
    let label_selector = Selector::parse("div.txt-label").ok()?;
    let li_selector = Selector::parse("ul li").ok()?;

    for label_el in document.select(&label_selector) {
        if text_of(&label_el).trim().trim_end_matches(':') != label {
            continue;
        }
        let parent = label_el.parent().and_then(ElementRef::wrap)?;
        let li = parent.select(&li_selector).next()?;
        let text = text_of(&li)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        return clean_optional_text(&text);
    }
    None
}

/// Confirms the real claim status by checking which of the three
/// mutually exclusive markers is present: a genuine verified badge,
/// a plain "Claimed" label, or an explicit "Unclaimed" one. This is
/// stored in badge_honorific, since the shared B2bSupplierProfile
/// has no dedicated three-way field, and platform_verified_badge is
/// set true only for the strongest of the three.
fn parse_verification_status(document: &Html) -> Option<String> {
    let h3_selector = Selector::parse("h3").ok()?;
    for el in document.select(&h3_selector) {
        if text_of(&el).contains("Thomas Verified Supplier") {
            return Some("Thomas Verified Supplier".to_string());
        }
    }

    if let Ok(selector) = Selector::parse("[data-sentry-component='Unclaimed']") {
        if document.select(&selector).next().is_some() {
            return Some("Unclaimed".to_string());
        }
    }

    if let Ok(selector) = Selector::parse("span.txt-label") {
        for el in document.select(&selector) {
            if text_of(&el).trim().eq_ignore_ascii_case("claimed") {
                return Some("Claimed".to_string());
            }
        }
    }

    None
}

impl B2bScraper for ThomasnetScraper {
    fn matches_platform(&self, platform: &str) -> bool {
        platform == "thomasnet"
    }

    fn parse_supplier(&self, html: &str, profile_url: &str) -> B2bSupplierProfile {
        let document = Html::parse_document(html);

        let company_name = Selector::parse("h1")
            .ok()
            .and_then(|s| document.select(&s).next())
            .map(|el| text_of(&el))
            .and_then(|t| clean_optional_text(&t));

        let verification_status = parse_verification_status(&document);
        let platform_verified_badge =
            verification_status.as_deref() == Some("Thomas Verified Supplier");

        let year_established = parse_labeled_field(&document, "Year Founded");
        let employee_count = parse_labeled_field(&document, "No of Employees");
        let sales_revenue = parse_labeled_field(&document, "Annual Sales");

        let location = Selector::parse("[data-sentry-component='SupplierLocations'] a")
            .ok()
            .and_then(|s| document.select(&s).next())
            .map(|el| text_of(&el))
            .and_then(|t| clean_optional_text(&t));

        // "Company Description by Thomasnet" is always present when
        // a company has any real description at all; a second,
        // self-authored one only shows up once a company has
        // claimed and filled in its own profile. Prefer the
        // self-authored one when it exists, since it's the more
        // specific, first-party source.
        let h3_selector = Selector::parse("h3").unwrap();
        let p_selector = Selector::parse("p").unwrap();

        let mut description_thomasnet = None;
        let mut description_self_authored = None;

        for h3 in document.select(&h3_selector) {
            let heading_text = text_of(&h3);
            if !heading_text.contains("Company Description by") {
                continue;
            }
            let Some(parent) = h3.parent().and_then(ElementRef::wrap) else {
                continue;
            };
            let Some(p) = parent.select(&p_selector).next() else {
                continue;
            };
            let value = clean_optional_text(&text_of(&p));
            if heading_text.contains("Thomasnet") {
                description_thomasnet = value;
            } else {
                description_self_authored = value;
            }
        }
        let company_description = description_self_authored.or(description_thomasnet);

        // The contact person is the first entry under "Key Personnel".
        // (The old selector took the first <p> in the business details
        // column, which is the "Primary Company Type", e.g. "Custom
        // Manufacturer" - not a person.) "Not available" becomes None.
        // Each entry is "Name, Role" (e.g. "Steve Savignac, Manager -
        // Quad Metalworks"); only the name is kept, same as on the
        // other platforms. The page's data record has the name on its
        // own, so it is used first; the visible text split at the first
        // comma is the fallback.
        let contact_name = next_company_data(&document)
            .as_ref()
            .and_then(|c| c.get("personnel"))
            .and_then(|p| p.as_array())
            .and_then(|list| list.iter().find_map(|person| json_text(person, "name")))
            .or_else(|| {
                first_labeled_item(&document, "Key Personnel")
                    .and_then(|entry| clean_optional_text(entry.split(',').next().unwrap_or("")))
            });

        let contact_phone = Selector::parse("a[href^='tel:']")
            .ok()
            .and_then(|s| document.select(&s).next())
            .map(|el| text_of(&el))
            .and_then(|t| clean_optional_text(&t));

        let website_url = Selector::parse("div.txt-label")
            .ok()
            .and_then(|label_sel| {
                document
                    .select(&label_sel)
                    .find(|el| text_of(el).trim() == "Website")
            })
            .and_then(|label_el| label_el.parent())
            .and_then(ElementRef::wrap)
            .and_then(|parent| {
                let a_sel = Selector::parse("ul a[href]").ok()?;
                parent
                    .select(&a_sel)
                    .next()
                    .and_then(|a| a.value().attr("href"))
            })
            .map(|s| s.to_string());

        // The page's own data record fills anything the visible page
        // didn't give. The visible page always wins.
        let company = next_company_data(&document);
        let from_data = |key: &str| company.as_ref().and_then(|c| json_text(c, key));
        let company_name = company_name.or_else(|| from_data("name"));
        let year_established = year_established.or_else(|| from_data("yearFounded"));
        let employee_count = employee_count.or_else(|| from_data("numberEmployees"));
        let sales_revenue = sales_revenue.or_else(|| from_data("annualSales"));
        let contact_phone = contact_phone.or_else(|| from_data("primaryPhone"));
        let website_url = website_url.or_else(|| from_data("website"));
        let company_description = company_description
            .or_else(|| from_data("descriptionByCompany"))
            .or_else(|| from_data("description"));
        let logo_url = from_data("logoUrl");
        // "Livermore, CA, USA" (with the country) is preferred over the
        // visible "Livermore, CA 94551", which never names the country.
        let location = company.as_ref().and_then(json_location).or(location);

        B2bSupplierProfile {
            company_name,
            logo_url,
            year_established,
            country: location,
            platform_verified_badge,
            employee_count,
            sales_revenue,
            export_percentage: None, // Not applicable - ThomasNet is US/North America focused
            profile_url: profile_url.to_string(),
            source_platform: "thomasnet".to_string(),
            contact_name,
            contact_phone,
            badge_honorific: verification_status,
            company_description,
            website_url,
        }
    }

    fn parse_listing(&self, html: &str, listing_url: &str) -> B2bListingProfile {
        let document = Html::parse_document(html);

        // The category-specific "Details" tab (e.g. "Fasteners: Hook
        // & Loop Details") is already present directly on the
        // profile page - no second fetch needed. Title is that tab's
        // label with " Details" trimmed off; description is its
        // tabpanel's text.
        let tab_selector = Selector::parse("#businessDescDetailsTab").ok();
        let title = tab_selector
            .as_ref()
            .and_then(|s| document.select(s).next())
            .map(|el| {
                text_of(&el)
                    .trim()
                    .trim_end_matches("Details")
                    .trim()
                    .to_string()
            })
            .and_then(|t| clean_optional_text(&t));

        let description = Selector::parse("[aria-labelledby='businessDescDetailsTab'] p")
            .ok()
            .and_then(|s| document.select(&s).next())
            .map(|el| text_of(&el))
            .and_then(|t| clean_optional_text(&t));

        // Fill gaps from the page's data record: the category this page
        // is about ("heading"), and the company's own photos.
        let company = next_company_data(&document);
        let heading = company.as_ref().and_then(|c| c.get("heading"));
        let title = title.or_else(|| heading.and_then(|h| json_text(h, "name")));
        let description = description.or_else(|| heading.and_then(|h| json_text(h, "description")));
        let image_urls: Vec<String> = company
            .as_ref()
            .and_then(|c| c.get("additionalInformation"))
            .and_then(|a| a.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter(|i| i.get("type").and_then(|t| t.as_str()) == Some("IMAGE"))
                    .filter_map(|i| json_text(i, "url"))
                    .collect()
            })
            .unwrap_or_default();

        B2bListingProfile {
            title,
            description,
            image_urls,
            // None of the price/MOQ/Incoterms fields apply on
            // ThomasNet - it's a capability/service directory, not a
            // priced-item marketplace, confirmed across every real
            // page reviewed.
            unit_price: None,
            fob_price: None,
            minimum_order_quantity: None,
            payment_type: None,
            preferred_port: None,
            reference: None,
            production_capacity: None,
            delivery_timeframe: None,
            incoterms: None,
            packaging_details: None,
            listing_url: listing_url.to_string(),
            source_platform: "thomasnet".to_string(),
        }
    }

    /// One record per company: "t-k-machine-30682072", taken from the
    /// page's canonical link (or the same link in the page's data
    /// record). The same for
    /// every category page of the company, so its Safely history and
    /// fraud reports are shared, not split per category.
    fn company_key(&self, listing_html: &str) -> Option<String> {
        let document = Html::parse_document(listing_html);
        Selector::parse("link[rel='canonical']")
            .ok()
            .and_then(|s| document.select(&s).next())
            .and_then(|el| el.value().attr("href"))
            .and_then(company_slug_from_url)
            .or_else(|| {
                let sel = Selector::parse("script#__NEXT_DATA__").ok()?;
                let raw = document.select(&sel).next()?.text().collect::<String>();
                let data: Value = serde_json::from_str(&raw).ok()?;
                data.pointer("/props/pageProps/canonicalUrl")?
                    .as_str()
                    .and_then(company_slug_from_url)
            })
    }
}

/// Confirms whether a given URL path is a ThomasNet company PROFILE
/// page - the primary target this scraper analyzes. Products/
/// services and catalog pages are real, separate ThomasNet pages,
/// but are not treated as their own "listing page" for extension-
/// activation purposes.
pub fn is_thomasnet_profile_url(path: &str) -> bool {
    path.contains("/company/") && path.contains("/profile")
}

pub fn matches_thomasnet_hostname(hostname: &str) -> bool {
    hostname == "www.thomasnet.com" || hostname == "thomasnet.com"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_platform_is_true_only_for_thomasnet() {
        let scraper = ThomasnetScraper;
        assert!(scraper.matches_platform("thomasnet"));
        assert!(!scraper.matches_platform("alibaba"));
    }

    #[test]
    fn is_thomasnet_profile_url_requires_both_segments() {
        assert!(is_thomasnet_profile_url("/company/acme-corp/profile"));
        assert!(!is_thomasnet_profile_url("/company/acme-corp/products"));
        assert!(!is_thomasnet_profile_url("/profile"));
    }

    #[test]
    fn matches_thomasnet_hostname_accepts_both_www_and_bare_forms() {
        assert!(matches_thomasnet_hostname("www.thomasnet.com"));
        assert!(matches_thomasnet_hostname("thomasnet.com"));
        assert!(!matches_thomasnet_hostname("thomasnet.co.uk"));
    }

    fn claimed_verified_html() -> &'static str {
        r#"
        <html><body>
        <h1>Acme Fasteners Inc</h1>
        <h3>Thomas Verified Supplier</h3>
        <div><div class="txt-label">Year Founded:</div><ul><li>2005</li></ul></div>
        <div><div class="txt-label">No of Employees:</div><ul><li>50-99</li></ul></div>
        <div><div class="txt-label">Annual Sales:</div><ul><li>Not available</li></ul></div>
        <div data-sentry-component="SupplierLocations"><a>Tustin, CA</a></div>
        <div><h3>Company Description by Thomasnet</h3><p>A ThomasNet-authored blurb.</p></div>
        <div><h3>Company Description by Acme Fasteners Inc</h3><p>Our own, self-authored description.</p></div>
        <div data-sentry-component="BusinessDetailsSectionColumn">
          <div><div class="txt-label">Primary Company Type</div><ul><li><p class="mar-0">Custom Manufacturer</p></li></ul></div>
          <div><div class="txt-label">Key Personnel</div><ul><li><p class="mar-0">Jane Doe, Sales Manager</p></li><li><p class="mar-0">John Roe, Accounts</p></li></ul></div>
        </div>
        <a href="tel:+15551234567">(555) 123-4567</a>
        <div><div class="txt-label">Website</div><ul><li><a href="https://acmefasteners.com">acmefasteners.com</a></li></ul></div>
        </body></html>
        "#
    }

    #[test]
    fn parse_supplier_extracts_the_real_fields_from_a_claimed_verified_profile() {
        let scraper = ThomasnetScraper;
        let profile = scraper.parse_supplier(
            claimed_verified_html(),
            "https://www.thomasnet.com/company/acme/profile",
        );

        assert_eq!(profile.company_name.as_deref(), Some("Acme Fasteners Inc"));
        assert!(
            profile.platform_verified_badge,
            "expected the Thomas Verified badge to be recognized"
        );
        assert_eq!(
            profile.badge_honorific.as_deref(),
            Some("Thomas Verified Supplier")
        );
        assert_eq!(profile.year_established.as_deref(), Some("2005"));
        assert_eq!(profile.employee_count.as_deref(), Some("50-99"));
        assert_eq!(
            profile.sales_revenue, None,
            "expected the literal 'Not available' text to become a real None, not a string"
        );
        assert_eq!(profile.country.as_deref(), Some("Tustin, CA"));
        assert_eq!(profile.contact_name.as_deref(), Some("Jane Doe"));
        assert!(profile.contact_phone.as_deref().unwrap().contains("555"));
        assert_eq!(
            profile.website_url.as_deref(),
            Some("https://acmefasteners.com")
        );
        assert_eq!(
            profile.export_percentage, None,
            "ThomasNet is US-focused - never populated"
        );
    }

    #[test]
    fn parse_supplier_prefers_the_self_authored_description_over_thomasnets_own() {
        let scraper = ThomasnetScraper;
        let profile = scraper.parse_supplier(
            claimed_verified_html(),
            "https://www.thomasnet.com/company/acme/profile",
        );
        assert_eq!(
            profile.company_description.as_deref(),
            Some("Our own, self-authored description."),
            "expected the self-authored description to win when both exist"
        );
    }

    #[test]
    fn parse_supplier_falls_back_to_thomasnets_description_when_no_self_authored_one_exists() {
        let html = r#"
        <html><body>
        <h1>Beta Supply Co</h1>
        <div><h3>Company Description by Thomasnet</h3><p>Only the Thomasnet-authored blurb exists here.</p></div>
        </body></html>
        "#;
        let scraper = ThomasnetScraper;
        let profile =
            scraper.parse_supplier(html, "https://www.thomasnet.com/company/beta/profile");
        assert_eq!(
            profile.company_description.as_deref(),
            Some("Only the Thomasnet-authored blurb exists here.")
        );
    }

    #[test]
    fn parse_supplier_recognizes_an_unclaimed_profile() {
        let html = r#"
        <html><body>
        <h1>Gamma Industrial</h1>
        <div data-sentry-component="Unclaimed"></div>
        </body></html>
        "#;
        let scraper = ThomasnetScraper;
        let profile =
            scraper.parse_supplier(html, "https://www.thomasnet.com/company/gamma/profile");
        assert_eq!(profile.badge_honorific.as_deref(), Some("Unclaimed"));
        assert!(!profile.platform_verified_badge);
    }

    #[test]
    fn parse_supplier_recognizes_a_claimed_but_unverified_profile() {
        let html = r#"
        <html><body>
        <h1>Delta Machining</h1>
        <span class="txt-label">Claimed</span>
        </body></html>
        "#;
        let scraper = ThomasnetScraper;
        let profile =
            scraper.parse_supplier(html, "https://www.thomasnet.com/company/delta/profile");
        assert_eq!(profile.badge_honorific.as_deref(), Some("Claimed"));
        assert!(
            !profile.platform_verified_badge,
            "expected 'Claimed' alone to NOT count as the stronger Verified badge"
        );
    }

    #[test]
    fn parse_listing_extracts_the_title_and_description_from_the_details_tab() {
        let html = r#"
        <html><body>
        <div id="businessDescDetailsTab">Fasteners: Hook &amp; Loop Details</div>
        <div aria-labelledby="businessDescDetailsTab"><p>Custom hook-and-loop fastener manufacturing.</p></div>
        </body></html>
        "#;
        let scraper = ThomasnetScraper;
        let listing = scraper.parse_listing(html, "https://www.thomasnet.com/company/acme/profile");

        assert_eq!(listing.title.as_deref(), Some("Fasteners: Hook & Loop"));
        assert_eq!(
            listing.description.as_deref(),
            Some("Custom hook-and-loop fastener manufacturing.")
        );
    }

    #[test]
    fn parse_listing_never_populates_price_or_moq_fields_since_thomasnet_has_none() {
        let scraper = ThomasnetScraper;
        let listing = scraper.parse_listing(
            "<html><body></body></html>",
            "https://www.thomasnet.com/company/x/profile",
        );

        assert_eq!(listing.unit_price, None);
        assert_eq!(listing.fob_price, None);
        assert_eq!(listing.minimum_order_quantity, None);
        assert_eq!(listing.incoterms, None);
    }

    // Trimmed from the real T & K Machine profile page (Oct 2026).
    const TK_PAGE: &str = r#"<html><head><link rel="canonical" href="https://www.thomasnet.com/company/t-k-machine-30682072/profile"/></head><body>
    <h1>T &amp; K Machine</h1>
    <div data-sentry-component="BusinessDetailsSectionColumn">
      <div><div class="txt-label">Primary Company Type</div><ul><li class="mar-l-2 mar-t-1"><p class="mar-0">Custom Manufacturer</p></li></ul></div>
      <div><div class="txt-label">Key Personnel</div><ul><li class="mar-l-2 mar-t-1">Not available</li></ul></div>
    </div>
    <div data-sentry-component="SupplierLocations"><a>Livermore, CA 94551</a></div>
    <script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"canonicalUrl":"https://www.thomasnet.com/company/t-k-machine-30682072/profile","data":{"tgramsId":30682072,"name":"T & K Machine","logoUrl":"https://cdn.thomasnet.com/ccp/30682072/185135.jpg","website":"https://tk-machine.com/","primaryPhone":"(866) 996-3676","annualSales":"$1 - 4.9 Mil","numberEmployees":"1-9","yearFounded":1994,"descriptionByCompany":"T&K Machine is a precision machine shop.","address":{"city":"Livermore","state":"CA","zip":"94551","country":"USA"},"heading":{"name":"Prototypes","description":"Custom manufacturer of prototypes."},"additionalInformation":[{"title":"Facility","url":"https://cdn.thomasnet.com/ccp/30682072/208047.JPG","type":"IMAGE"},{"title":"Brochure","url":"https://cdn.thomasnet.com/ccp/30682072/x.pdf","type":"PDF"}]}}}}</script>
    </body></html>"#;

    #[test]
    fn company_type_is_not_used_as_contact_name() {
        let s = ThomasnetScraper.parse_supplier(TK_PAGE, "u");
        assert_eq!(
            s.contact_name, None,
            "'Custom Manufacturer' is not a person"
        );
    }

    #[test]
    fn first_key_person_is_the_contact_name() {
        let s = ThomasnetScraper.parse_supplier(claimed_verified_html(), "u");
        assert_eq!(s.contact_name.as_deref(), Some("Jane Doe"));
    }

    #[test]
    fn page_data_fills_missing_fields_and_logo() {
        let s = ThomasnetScraper.parse_supplier(TK_PAGE, "u");
        assert_eq!(s.company_name.as_deref(), Some("T & K Machine"));
        assert_eq!(
            s.logo_url.as_deref(),
            Some("https://cdn.thomasnet.com/ccp/30682072/185135.jpg")
        );
        assert_eq!(s.year_established.as_deref(), Some("1994"));
        assert_eq!(s.employee_count.as_deref(), Some("1-9"));
        assert_eq!(s.sales_revenue.as_deref(), Some("$1 - 4.9 Mil"));
        assert_eq!(s.contact_phone.as_deref(), Some("(866) 996-3676"));
        assert_eq!(s.website_url.as_deref(), Some("https://tk-machine.com/"));
        assert_eq!(
            s.company_description.as_deref(),
            Some("T&K Machine is a precision machine shop.")
        );
        assert_eq!(s.country.as_deref(), Some("Livermore, CA, USA"));
    }

    #[test]
    fn visible_page_wins_over_page_data() {
        // No page data at all: the visible values are used unchanged.
        let s = ThomasnetScraper.parse_supplier(claimed_verified_html(), "u");
        assert_eq!(s.country.as_deref(), Some("Tustin, CA"));
        assert_eq!(s.year_established.as_deref(), Some("2005"));
        assert_eq!(s.logo_url, None);
    }

    #[test]
    fn listing_gets_photos_but_not_documents() {
        let l = ThomasnetScraper.parse_listing(TK_PAGE, "u");
        assert_eq!(l.title.as_deref(), Some("Prototypes"));
        assert_eq!(
            l.description.as_deref(),
            Some("Custom manufacturer of prototypes.")
        );
        assert_eq!(
            l.image_urls,
            vec!["https://cdn.thomasnet.com/ccp/30682072/208047.JPG"]
        );
    }

    #[test]
    fn company_key_is_the_company_slug() {
        assert_eq!(
            ThomasnetScraper.company_key(TK_PAGE).as_deref(),
            Some("t-k-machine-30682072")
        );
        assert_eq!(
            company_slug_from_url(
                "https://www.thomasnet.com/company/t-k-machine-30682072/profile?coverage_area=NA&heading=63750202"
            )
            .as_deref(),
            Some("t-k-machine-30682072")
        );
        assert_eq!(ThomasnetScraper.company_key("<html></html>"), None);
    }

    #[test]
    fn contact_name_comes_from_page_data_without_the_role() {
        // Trimmed from the real QuadMetalworks profile page (Oct 2026).
        let html = r#"<html><body>
        <div><div class="txt-label">Key Personnel</div><ul><li><p class="mar-0">Steve Savignac, Manager - Quad Metalworks</p></li><li><p class="mar-0">Ross Byrum, Sales / Customer Service</p></li></ul></div>
        <script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"data":{"personnel":[{"name":"Steve Savignac","title":"Manager - Quad Metalworks"},{"name":"Ross Byrum","title":"Sales / Customer Service"}]}}}}</script>
        </body></html>"#;
        let s = ThomasnetScraper.parse_supplier(html, "u");
        assert_eq!(s.contact_name.as_deref(), Some("Steve Savignac"));
    }
}
