pub mod alibaba;
pub mod b2bmap;
pub mod b2brazil;
pub mod exporthub;
pub mod kompass;
pub mod thomasnet;
pub mod tradewheel;

use crate::services::scraper_client::{build_scraper_client, wrap_scraper_url_for_platform};

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
    /// A stable ID for the COMPANY behind this listing, read from the
    /// listing page - the same for every product that company sells.
    /// Used as the seller's platform_id, so fraud reports and Safely
    /// history belong to the company, not to one product page.
    ///
    /// Default: None, meaning "this platform doesn't know yet". Safely
    /// then falls back to the old per-listing ID, so platforms that
    /// haven't implemented this keep working exactly as before.
    fn company_key(&self, _listing_html: &str) -> Option<String> {
        None
    }
}

pub fn get_scraper_for_platform(platform: &str) -> Option<Box<dyn B2bScraper>> {
    let scrapers: Vec<Box<dyn B2bScraper>> = vec![
        Box::new(b2brazil::B2brazilScraper),
        Box::new(alibaba::AlibabaScraper),
        Box::new(tradewheel::TradewheelScraper),
        Box::new(exporthub::ExporthubScraper),
        Box::new(thomasnet::ThomasnetScraper),
        Box::new(b2bmap::B2bmapScraper),
        Box::new(kompass::KompassScraper),
    ];
    scrapers.into_iter().find(|s| s.matches_platform(platform))
}

const MIN_PLAUSIBLE_HTML_BYTES: usize = 2000;

pub fn looks_like_a_real_page(html: &str) -> bool {
    html.len() >= MIN_PLAUSIBLE_HTML_BYTES
        && (html.to_lowercase().contains("<html") || html.to_lowercase().contains("<!doctype"))
}

/// Everything one B2B page fetch produces: the supplier, the listing,
/// and (if the platform supports it) the company's stable ID.
pub struct B2bPageResult {
    pub supplier: B2bSupplierProfile,
    pub listing: B2bListingProfile,
    pub company_key: Option<String>,
    /// The company's own page on the platform (not the product page).
    pub company_url: Option<String>,
}

/// Unchanged signature, kept so existing callers and tests keep
/// working. Same fetch as fetch_b2b_page, minus the company key.
pub async fn check_b2b_page(
    platform: &str,
    page_url: &str,
) -> Option<(B2bSupplierProfile, B2bListingProfile)> {
    fetch_b2b_page(platform, page_url)
        .await
        .map(|r| (r.supplier, r.listing))
}

/// The listing link without tracking extras. Alibaba links carry
/// "?spm=...&priceId=..." from search pages; the page is the same
/// without them, and the plain link is what was tested to work through
/// ScraperAPI. Other platforms' links are left exactly as they are.
pub fn clean_listing_url(platform: &str, page_url: &str) -> String {
    if platform == "alibaba" {
        let end = page_url.find(['?', '#']).unwrap_or(page_url.len());
        page_url[..end].to_string()
    } else {
        page_url.to_string()
    }
}

/// How many times a failed ScraperAPI request is tried again. With
/// ultra_premium + render, Alibaba pages take close to a minute and
/// ScraperAPI sometimes gives up ("500 Request failed. You will not be
/// charged for this request") and succeeds on the next try. Only server
/// errors (5xx) and network errors are retried. Set to 0 to turn off.
const FETCH_RETRIES: u32 = 1;

/// One ScraperAPI request, tried again on a server error. Returns the
/// page, or None (with the reason in the log).
async fn fetch_with_retry(
    client: &reqwest::Client,
    fetch_url: &str,
    page_url: &str,
) -> Option<String> {
    let mut attempt = 0;
    loop {
        let retry = match client.get(fetch_url).send().await {
            Ok(response) if response.status().is_success() => return response.text().await.ok(),
            Ok(response) => {
                let status = response.status();
                // ScraperAPI explains the failure in the response body.
                let body = response.text().await.unwrap_or_default();
                let reason: String = body.chars().take(300).collect();
                eprintln!(
                    "Safely: B2B fetch failed for {} - status {} - {}",
                    page_url,
                    status,
                    reason.trim()
                );
                status.is_server_error()
            }
            Err(e) => {
                eprintln!("Safely: B2B fetch failed for {} - {}", page_url, e);
                true
            }
        };
        if !retry || attempt >= FETCH_RETRIES {
            return None;
        }
        attempt += 1;
        eprintln!(
            "Safely: trying {} again (attempt {} of {})",
            page_url,
            attempt + 1,
            FETCH_RETRIES + 1
        );
    }
}

pub async fn fetch_b2b_page(platform: &str, page_url: &str) -> Option<B2bPageResult> {
    let scraper = get_scraper_for_platform(platform)?;
    let client = build_scraper_client();
    let fetch_url = wrap_scraper_url_for_platform(&clean_listing_url(platform, page_url), platform);

    let html = fetch_with_retry(&client, &fetch_url, page_url).await?;

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
    let company_key = scraper.company_key(&html);
    let company_url = scraper.extract_company_profile_url(&html);

    if supplier.company_name.is_none() && listing.title.is_none() {
        eprintln!(
            "Safely: DEPENDENCY DOWN: B2B fetch for {} - real HTML but no matching content. Total length: {} bytes. First 3000 chars:\n{}",
            page_url,
            html.len(),
            &html[..html.len().min(3000)]
        );
        return None;
    }

    enrich_from_profile_pages(scraper.as_ref(), platform, &html, &mut supplier).await;

    Some(B2bPageResult {
        supplier,
        listing,
        company_key,
        company_url,
    })
}

/// Fetches the company's own profile pages (through ScraperAPI) and adds
/// what they show - founding year, employees, description - to the
/// supplier. Optional: if a fetch fails, the supplier keeps what the
/// listing page already gave.
async fn enrich_from_profile_pages(
    scraper: &dyn B2bScraper,
    platform: &str,
    listing_html: &str,
    supplier: &mut B2bSupplierProfile,
) {
    let client = build_scraper_client();
    if let Some(profile_url) = scraper.extract_company_profile_url(listing_html) {
        let profile_fetch_url = wrap_scraper_url_for_platform(&profile_url, platform);
        if let Ok(profile_response) = client.get(&profile_fetch_url).send().await {
            if profile_response.status().is_success() {
                if let Ok(profile_html) = profile_response.text().await {
                    if looks_like_a_real_page(&profile_html) {
                        take_and_replace(supplier, |s| {
                            scraper.enrich_from_company_profile(s, &profile_html)
                        });
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
            let extended_fetch_url = wrap_scraper_url_for_platform(&extended_url, platform);
            if let Ok(extended_response) = client.get(&extended_fetch_url).send().await {
                if extended_response.status().is_success() {
                    if let Ok(extended_html) = extended_response.text().await {
                        if looks_like_a_real_page(&extended_html) {
                            take_and_replace(supplier, |s| {
                                scraper.enrich_from_extended_profile(s, &extended_html)
                            });
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
}

fn take_and_replace(
    supplier: &mut B2bSupplierProfile,
    f: impl FnOnce(B2bSupplierProfile) -> B2bSupplierProfile,
) {
    let current = std::mem::take(supplier);
    *supplier = f(current);
}

/// Platforms where the page the buyer's browser sent (page_html) is the
/// BACKUP: the listing is fetched through ScraperAPI first, and the
/// browser's page is read only if ScraperAPI fails. Useful for Alibaba,
/// where ScraperAPI is slow (close to a minute with ultra premium +
/// render) and some listings (e.g. medical items) are hidden from US
/// visitors.
/// To stop using the backup for a platform, remove it from this list
/// (and from BROWSER_PAGE_PLATFORMS in the extension's api.ts).
pub const BROWSER_PAGE_PLATFORMS: &[&str] = &["alibaba"];

/// Reads a B2B listing from the page the buyer's browser sent. Returns
/// None if the page is missing or unreadable. The page itself is never
/// saved or written to the log (it can contain the buyer's own account
/// details from the site header); only its size is logged.
pub async fn b2b_page_from_browser(
    platform: &str,
    page_url: &str,
    page_html: Option<&str>,
) -> Option<B2bPageResult> {
    if !BROWSER_PAGE_PLATFORMS.contains(&platform) {
        return None;
    }
    let html = page_html?;
    let scraper = get_scraper_for_platform(platform)?;
    if !looks_like_a_real_page(html) {
        eprintln!(
            "Safely: {} page from the browser is too small to read ({} bytes)",
            platform,
            html.len()
        );
        return None;
    }

    let mut supplier = scraper.parse_supplier(html, page_url);
    let listing = scraper.parse_listing(html, page_url);
    if supplier.company_name.is_none() && listing.title.is_none() {
        eprintln!(
            "Safely: nothing readable in the {} page from the browser ({} bytes)",
            platform,
            html.len()
        );
        return None;
    }
    let company_key = scraper.company_key(html);
    let company_url = scraper.extract_company_profile_url(html);

    enrich_from_profile_pages(scraper.as_ref(), platform, html, &mut supplier).await;

    eprintln!(
        "Safely: ScraperAPI could not get the {} listing - read it from the browser page instead ({} bytes)",
        platform,
        html.len()
    );
    Some(B2bPageResult {
        supplier,
        listing,
        company_key,
        company_url,
    })
}

#[cfg(test)]
mod browser_page_tests {
    use super::*;

    #[test]
    fn alibaba_links_lose_tracking_extras_only() {
        assert_eq!(
            clean_listing_url(
                "alibaba",
                "https://www.alibaba.com/product-detail/x_1601806837928.html?spm=a2700.x&priceId=8a"
            ),
            "https://www.alibaba.com/product-detail/x_1601806837928.html"
        );
        assert_eq!(
            clean_listing_url("alibaba", "https://www.alibaba.com/product-detail/x_1.html"),
            "https://www.alibaba.com/product-detail/x_1.html"
        );
        assert_eq!(
            clean_listing_url("tradewheel", "https://www.tradewheel.com/p/x/?a=1"),
            "https://www.tradewheel.com/p/x/?a=1"
        );
    }

    #[tokio::test]
    async fn browser_page_is_used_only_for_listed_platforms_and_real_pages() {
        // Not a listed platform: always fetched as before.
        assert!(
            b2b_page_from_browser("exporthub", "u", Some("<html></html>"))
                .await
                .is_none()
        );
        // No page sent: fall back to fetching.
        assert!(b2b_page_from_browser("alibaba", "u", None).await.is_none());
        // Too small to be a real page: fall back to fetching.
        assert!(
            b2b_page_from_browser("alibaba", "u", Some("<html></html>"))
                .await
                .is_none()
        );
    }
}
