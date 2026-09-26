use super::{B2bListingProfile, B2bScraper, B2bSupplierProfile};
use scraper::{ElementRef, Html, Selector};

pub struct ThomasnetScraper;

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

        let contact_name =
            Selector::parse("[data-sentry-component='BusinessDetailsSectionColumn'] p.mar-0")
                .ok()
                .and_then(|s| document.select(&s).next())
                .map(|el| text_of(&el))
                .and_then(|t| clean_optional_text(&t));

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

        B2bSupplierProfile {
            company_name,
            logo_url: None,
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

        B2bListingProfile {
            title,
            description,
            image_urls: Vec::new(),
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
