pub mod alibaba;
pub mod b2bmap;
pub mod b2brazil;
pub mod exporthub;
pub mod kompass;
pub mod thomasnet;
pub mod tradewheel;

use std::sync::atomic::{AtomicBool, Ordering};

use crate::services::scraper_client::{
    build_scraper_client, scraperapi_key_is_set, wrap_scraper_url_for_platform,
};

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

/// Text that only appears on a site's "are you a robot?" page, not on a
/// real page. Such a page loads with an OK status, so without this check
/// it was counted as a real company page.
/// - "punish-component", "sufei-punish": Alibaba's robot-check page.
/// - "Please enable JS and disable any ad blocker": Kompass's robot-check
///   page (DataDome).
/// To add another site, add a piece of text only its robot-check page has.
const BLOCK_PAGE_MARKERS: &[&str] = &[
    "punish-component",
    "sufei-punish",
    "Please enable JS and disable any ad blocker",
];

/// true when the page is a robot-check page instead of the real page.
pub fn looks_like_a_block_page(html: &str) -> bool {
    BLOCK_PAGE_MARKERS
        .iter()
        .any(|marker| html.contains(marker))
}

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
    /// true when the company's own page (or its extended page) could not
    /// be loaded, so founding year, employees, description etc. may be
    /// missing. The panel then shows a "Company details" note asking the
    /// user to scan again. false when the pages loaded, or when the
    /// platform has no company page.
    pub company_page_missing: bool,
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

/// Same as FETCH_RETRIES, but for the company's own pages (company
/// profile and extended profile), fetched after the listing page. A
/// failed company page used to be skipped silently; now the reason is
/// in the log and it is tried once more. Each extra try can add up to
/// about a minute on Alibaba when ScraperAPI is slow. Set to 0 to turn
/// the extra try off (the reason is still logged).
const PROFILE_FETCH_RETRIES: u32 = 1;

/// One ScraperAPI request, tried again on a server error. Returns the
/// page, or None (with the reason in the log).
async fn fetch_with_retry(
    client: &reqwest::Client,
    fetch_url: &str,
    page_url: &str,
) -> Option<String> {
    fetch_with_retries(client, fetch_url, page_url, FETCH_RETRIES).await
}

/// The real fetch loop. `page_url` is the page's own address and is the
/// only address written to the log - `fetch_url` contains the ScraperAPI
/// key and is never printed.
async fn fetch_with_retries(
    client: &reqwest::Client,
    fetch_url: &str,
    page_url: &str,
    retries: u32,
) -> Option<String> {
    let mut attempt = 0;
    loop {
        let retry = match client.get(fetch_url).send().await {
            Ok(response) if response.status().is_success() => {
                SCRAPERAPI_KEY_REJECTED.store(false, Ordering::Relaxed);
                return response.text().await.ok();
            }
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
                // ScraperAPI's answer to a wrong key: status 401 and
                // "...please make sure your API key is valid."
                if status.as_u16() == 401 && body.contains("API key") {
                    SCRAPERAPI_KEY_REJECTED.store(true, Ordering::Relaxed);
                    eprintln!(
                        "Safely: ScraperAPI says SCRAPERAPI_KEY is wrong - fix it in backend/.env and restart. B2B scans stop until then."
                    );
                }
                status.is_server_error()
            }
            Err(e) => {
                eprintln!("Safely: B2B fetch failed for {} - {}", page_url, e);
                true
            }
        };
        if !retry || attempt >= retries {
            return None;
        }
        attempt += 1;
        eprintln!(
            "Safely: trying {} again (attempt {} of {})",
            page_url,
            attempt + 1,
            retries + 1
        );
    }
}

/// Whether B2B scans may run when SCRAPERAPI_KEY is missing.
/// false: without the key, every B2B scan (all 7 platforms, including the
/// Alibaba browser copy) stops, and the panel shows the normal "Couldn't
/// analyze this listing" screen. A missing key is noticed right away
/// instead of hidden behind thinner or blocked results.
/// true: scans try to load pages directly, without ScraperAPI (some sites
/// block that), and Alibaba may use the browser copy.
/// When the key is set, this switch changes nothing.
const B2B_SCANS_WITHOUT_KEY: bool = false;

/// Set when ScraperAPI rejects the key (a wrong key). Treated the same as
/// a missing key: the scan stops, and the Alibaba browser copy is not
/// used. Cleared again as soon as ScraperAPI accepts a request.
static SCRAPERAPI_KEY_REJECTED: AtomicBool = AtomicBool::new(false);

/// true when scans may run: the key is set and ScraperAPI has not
/// rejected it (or B2B_SCANS_WITHOUT_KEY is switched on).
fn scraperapi_key_usable() -> bool {
    B2B_SCANS_WITHOUT_KEY
        || (scraperapi_key_is_set() && !SCRAPERAPI_KEY_REJECTED.load(Ordering::Relaxed))
}

pub async fn fetch_b2b_page(platform: &str, page_url: &str) -> Option<B2bPageResult> {
    let scraper = get_scraper_for_platform(platform)?;
    if !B2B_SCANS_WITHOUT_KEY && !scraperapi_key_is_set() {
        eprintln!(
            "Safely: SCRAPERAPI_KEY is not set - scan stopped for {}",
            page_url
        );
        return None;
    }
    if !scraperapi_key_usable() {
        eprintln!(
            "Safely: SCRAPERAPI_KEY is wrong - scan stopped for {}",
            page_url
        );
        return None;
    }
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

    let company_page_missing =
        !enrich_from_profile_pages(scraper.as_ref(), platform, page_url, &html, &mut supplier)
            .await;

    Some(B2bPageResult {
        supplier,
        listing,
        company_key,
        company_url,
        company_page_missing,
    })
}

/// How many more times a company page is asked for when the site
/// answers with its robot-check page instead of the real page. Every
/// ScraperAPI request goes out through a different connection (IP), so
/// the next try is often let through. Only the company's own pages are
/// tried again - the listing page is fetched exactly as before. Each
/// extra try costs ScraperAPI credits and, on Alibaba, up to about a
/// minute. Set to 0 to turn the extra try off.
const BLOCK_PAGE_RETRIES: u32 = 1;

/// What came back when a company page was fetched.
enum CompanyPage {
    /// The real page.
    Loaded(String),
    /// Only the site's robot-check page, even after BLOCK_PAGE_RETRIES.
    Blocked,
    /// Something that isn't a web page (size in bytes).
    NotAPage(usize),
    /// The request failed (the reason is already in the log).
    Failed,
}

/// Fetches one company page, asking again (up to BLOCK_PAGE_RETRIES
/// times) when the site shows its robot-check page.
async fn fetch_company_page(
    client: &reqwest::Client,
    fetch_url: &str,
    page_url: &str,
) -> CompanyPage {
    let mut blocked = 0;
    loop {
        match fetch_with_retries(client, fetch_url, page_url, PROFILE_FETCH_RETRIES).await {
            None => return CompanyPage::Failed,
            Some(html) if looks_like_a_block_page(&html) => {
                if blocked >= BLOCK_PAGE_RETRIES {
                    return CompanyPage::Blocked;
                }
                blocked += 1;
                eprintln!(
                    "Safely: company page {} was a robot-check page - asking again through a different connection (attempt {} of {})",
                    page_url,
                    blocked + 1,
                    BLOCK_PAGE_RETRIES + 1
                );
            }
            Some(html) if looks_like_a_real_page(&html) => return CompanyPage::Loaded(html),
            Some(html) => return CompanyPage::NotAPage(html.len()),
        }
    }
}

/// Fetches the company's own profile pages (through ScraperAPI) and adds
/// what they show - founding year, employees, description - to the
/// supplier. Optional: if a fetch fails, the supplier keeps what the
/// listing page already gave.
/// Returns true when every company page that exists was loaded (or the
/// platform has none), false when one of them could not be loaded.
async fn enrich_from_profile_pages(
    scraper: &dyn B2bScraper,
    platform: &str,
    listing_url: &str,
    listing_html: &str,
    supplier: &mut B2bSupplierProfile,
) -> bool {
    let mut all_loaded = true;
    let client = build_scraper_client();
    if let Some(profile_url) = scraper.extract_company_profile_url(listing_html) {
        let profile_fetch_url = wrap_scraper_url_for_platform(&profile_url, platform);
        // A failure is logged inside fetch_with_retries (status + reason),
        // and the listing-page data is kept.
        match fetch_company_page(&client, &profile_fetch_url, &profile_url).await {
            CompanyPage::Loaded(profile_html) => {
                take_and_replace(supplier, |s| {
                    scraper.enrich_from_company_profile(s, &profile_html)
                });
            }
            CompanyPage::Blocked => {
                all_loaded = false;
                eprintln!(
                    "Safely: company page {} was a robot-check page, not the real page - company details skipped",
                    profile_url
                );
            }
            CompanyPage::NotAPage(len) => {
                all_loaded = false;
                eprintln!(
                    "Safely: DEPENDENCY DOWN: B2B enrichment fetch for {} returned {} bytes that don't look like a real page - skipping enrichment, keeping listing-page data only",
                    profile_url, len
                );
            }
            CompanyPage::Failed => all_loaded = false,
        }

        if let Some(extended_url) = scraper.build_extended_profile_url(&profile_url) {
            let extended_fetch_url = wrap_scraper_url_for_platform(&extended_url, platform);
            match fetch_company_page(&client, &extended_fetch_url, &extended_url).await {
                CompanyPage::Loaded(extended_html) => {
                    take_and_replace(supplier, |s| {
                        scraper.enrich_from_extended_profile(s, &extended_html)
                    });
                }
                CompanyPage::Blocked => {
                    all_loaded = false;
                    eprintln!(
                        "Safely: extended company page {} was a robot-check page, not the real page - those details skipped",
                        extended_url
                    );
                }
                CompanyPage::NotAPage(len) => {
                    all_loaded = false;
                    eprintln!(
                        "Safely: DEPENDENCY DOWN: B2B extended-profile fetch for {} returned {} bytes that don't look like a real page - skipping",
                        extended_url, len
                    );
                }
                CompanyPage::Failed => all_loaded = false,
            }
        }
    } else {
        // Not counted as a failure: some platforms and listings have no
        // company page at all. Logged so the reason is visible.
        eprintln!(
            "Safely: no company page link found on {} - company details come from the listing page only",
            listing_url
        );
    }
    all_loaded
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
    if !scraperapi_key_usable() {
        // Key missing or wrong - the reason was already logged.
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

    let company_page_missing =
        !enrich_from_profile_pages(scraper.as_ref(), platform, page_url, html, &mut supplier).await;

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
        company_page_missing,
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

    /// A tiny local web server that answers each request with the next
    /// page in `pages`.
    fn serve(pages: Vec<String>) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for (stream, body) in listener.incoming().zip(pages) {
                let mut stream = stream.unwrap();
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        format!("http://{}/company_profile.html", addr)
    }

    fn real_page() -> String {
        format!(
            "<!DOCTYPE html><html><body>{}</body></html>",
            "x".repeat(3000)
        )
    }

    fn robot_page() -> String {
        format!(
            "<!DOCTYPE html><html><body><div id=\"sufei-punish\"></div>{}</body></html>",
            "x".repeat(3000)
        )
    }

    #[tokio::test]
    async fn robot_check_company_page_is_asked_for_again() {
        let url = serve(vec![robot_page(), real_page()]);
        let client = build_scraper_client();
        assert!(matches!(
            fetch_company_page(&client, &url, &url).await,
            CompanyPage::Loaded(_)
        ));
    }

    #[tokio::test]
    async fn robot_check_twice_gives_up() {
        let url = serve(vec![robot_page(), robot_page(), real_page()]);
        let client = build_scraper_client();
        assert!(matches!(
            fetch_company_page(&client, &url, &url).await,
            CompanyPage::Blocked
        ));
    }
}
