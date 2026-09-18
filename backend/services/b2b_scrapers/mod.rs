pub mod alibaba;
pub mod b2brazil;
pub mod exporthub;
pub mod tradewheel;

use crate::services::scraper_client::{build_scraper_client, wrap_scraper_url};

#[derive(Debug, Default)]
pub struct B2bSupplierProfile {
    pub company_name: Option<String>,
    pub logo_url: Option<String>,
    pub year_established: Option<String>,
    pub country: Option<String>,
    pub platform_verified_badge: bool,
    pub employee_count: Option<String>,
    pub sales_revenue: Option<String>,
    pub export_percentage: Option<String>,
    pub profile_url: String,
    pub source_platform: String,
    pub contact_name: Option<String>,
    pub contact_phone: Option<String>,
    pub badge_honorific: Option<String>,
    pub company_description: Option<String>,
    pub website_url: Option<String>,
}

#[derive(Debug, Default)]
pub struct B2bListingProfile {
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_urls: Vec<String>,
    pub unit_price: Option<String>,
    pub fob_price: Option<String>,
    pub minimum_order_quantity: Option<String>,
    pub payment_type: Option<String>,
    pub preferred_port: Option<String>,
    pub reference: Option<String>,
    pub production_capacity: Option<String>,
    pub delivery_timeframe: Option<String>,
    pub incoterms: Option<String>,
    pub packaging_details: Option<String>,
    pub listing_url: String,
    pub source_platform: String,
}

pub trait B2bScraper: Send + Sync {
    fn matches_platform(&self, platform: &str) -> bool;
    fn parse_supplier(&self, html: &str, profile_url: &str) -> B2bSupplierProfile;
    fn parse_listing(&self, html: &str, listing_url: &str) -> B2bListingProfile;
    fn extract_company_profile_url(&self, _listing_html: &str) -> Option<String> {
        None
    }
    fn enrich_from_company_profile(
        &self,
        supplier: B2bSupplierProfile,
        _profile_html: &str,
    ) -> B2bSupplierProfile {
        supplier
    }
    fn build_extended_profile_url(&self, _profile_url: &str) -> Option<String> {
        None
    }
    fn enrich_from_extended_profile(
        &self,
        supplier: B2bSupplierProfile,
        _extended_html: &str,
    ) -> B2bSupplierProfile {
        supplier
    }
}

pub fn get_scraper_for_platform(platform: &str) -> Option<Box<dyn B2bScraper>> {
    let scrapers: Vec<Box<dyn B2bScraper>> = vec![
        Box::new(b2brazil::B2brazilScraper),
        Box::new(alibaba::AlibabaScraper),
        Box::new(tradewheel::TradewheelScraper),
        Box::new(exporthub::ExporthubScraper),
    ];
    scrapers.into_iter().find(|s| s.matches_platform(platform))
}

const MIN_PLAUSIBLE_HTML_BYTES: usize = 2000;

pub fn looks_like_a_real_page(html: &str) -> bool {
    html.len() >= MIN_PLAUSIBLE_HTML_BYTES
        && (html.to_lowercase().contains("<html") || html.to_lowercase().contains("<!doctype"))
}

pub async fn check_b2b_page(
    platform: &str,
    page_url: &str,
) -> Option<(B2bSupplierProfile, B2bListingProfile)> {
    let scraper = get_scraper_for_platform(platform)?;
    let client = build_scraper_client();
    let fetch_url = wrap_scraper_url(page_url);

    let response = client.get(&fetch_url).send().await.ok()?;

    if !response.status().is_success() {
        eprintln!(
            "Safely: B2B fetch failed for {} - status {}",
            page_url,
            response.status()
        );
        return None;
    }

    let html = response.text().await.ok()?;

    if !looks_like_a_real_page(&html) {
        eprintln!(
            "Safely: DEPENDENCY DOWN: B2B fetch for {} returned {} bytes that don't look like a real page - likely ScraperAPI credits exhausted. First 300 chars: {}",
            page_url,
            html.len(),
            &html[..html.len().min(300)]
        );
        return None;
    }

    let mut supplier = scraper.parse_supplier(&html, page_url);
    let listing = scraper.parse_listing(&html, page_url);

    if let Some(profile_url) = scraper.extract_company_profile_url(&html) {
        let profile_fetch_url = wrap_scraper_url(&profile_url);
        if let Ok(profile_response) = client.get(&profile_fetch_url).send().await {
            if profile_response.status().is_success() {
                if let Ok(profile_html) = profile_response.text().await {
                    if looks_like_a_real_page(&profile_html) {
                        supplier = scraper.enrich_from_company_profile(supplier, &profile_html);
                    } else {
                        eprintln!(
                            "Safely: DEPENDENCY DOWN: B2B enrichment fetch for {} returned {} bytes that don't look like a real page - skipping enrichment, keeping listing-page data only",
                            profile_url,
                            profile_html.len()
                        );
                    }
                }
            }
        }

        if let Some(extended_url) = scraper.build_extended_profile_url(&profile_url) {
            let extended_fetch_url = wrap_scraper_url(&extended_url);
            if let Ok(extended_response) = client.get(&extended_fetch_url).send().await {
                if extended_response.status().is_success() {
                    if let Ok(extended_html) = extended_response.text().await {
                        if looks_like_a_real_page(&extended_html) {
                            supplier =
                                scraper.enrich_from_extended_profile(supplier, &extended_html);
                        } else {
                            eprintln!(
                                "Safely: DEPENDENCY DOWN: B2B extended-profile fetch for {} returned {} bytes that don't look like a real page - skipping",
                                extended_url,
                                extended_html.len()
                            );
                        }
                    }
                }
            }
        }
    }

    Some((supplier, listing))
}
