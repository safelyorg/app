pub mod alibaba;
pub mod b2bmap;
pub mod b2brazil;
pub mod exporthub;
pub mod globalsources;
pub mod kompass;
pub mod madeinchina;
pub mod thomasnet;
pub mod tradewheel;
pub mod wholesalecentral;

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

/// What the platform shows about a supplier beyond the normal profile:
/// numbers the platform keeps (Alibaba: orders, rating, checks) and
/// what the company lists about itself (B2Brazil: other products,
/// business types, certificates, the year it joined). Only Alibaba and
/// B2Brazil (and Wholesale Central: years on the platform and its
/// categories) fill this in; every other platform returns None (see
/// B2bScraper::supplier_record), so nothing changes for them.
/// Every field is optional: a missing value is simply not shown.
#[derive(Debug, Default, Clone)]
pub struct SupplierRecord {
    /// The platform's name as buyers know it, e.g. "Alibaba".
    pub platform: String,
    /// The platform checked the company itself (Alibaba: an on-site
    /// check or a third-party assessment). Not the same as the
    /// "Verified" badge, which is kept in platform_verified_badge.
    pub checked_by_platform: bool,
    /// A paid membership level, e.g. "Gold Supplier". Paid, so never
    /// treated as a check on the company.
    pub member_label: Option<String>,
    /// How long the account has been on the platform (not how old
    /// the company is).
    pub years_on_platform: Option<u32>,
    /// Orders paid through the platform in the last 6 months.
    pub orders_6_months: Option<u64>,
    /// Their value as the platform shows it, e.g. "260,000+" (US$).
    pub order_value_6_months: Option<String>,
    /// Average buyer rating out of 5.
    pub rating: Option<f64>,
    pub review_count: Option<u64>,
    /// e.g. "100.0%".
    pub on_time_rate: Option<String>,
    /// Share of buyers who order again, e.g. "21%".
    pub reorder_rate: Option<String>,
    /// The year the supplier joined the platform (B2Brazil: "Since
    /// 2024"). Not the year the company was founded.
    pub joined_platform_year: Option<i32>,
    /// Products the company itself lists on the platform: its other
    /// products and its keywords. Used to spot products that are common
    /// bait in fake commodity deals.
    pub products_offered: Vec<String>,
    /// The business types the company picked, e.g. "Importer / Trading
    /// Company".
    pub business_types: Vec<String>,
    /// Links to the certificate images the company shows.
    pub certificate_images: Vec<String>,
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
    /// The platform's own record of this supplier (orders, rating,
    /// checks), read from the listing page. Default: None - platforms
    /// that don't implement it keep working exactly as before.
    fn supplier_record(&self, _listing_html: &str) -> Option<SupplierRecord> {
        None
    }
    /// Adds what only the company's own page shows about the listing
    /// (Global Sources trade-show pages: the price and minimum order).
    /// Default: unchanged.
    fn enrich_listing_from_company_profile(
        &self,
        listing: B2bListingProfile,
        _profile_html: &str,
    ) -> B2bListingProfile {
        listing
    }
    /// Adds what the company's own page shows (certificates, products)
    /// to the record from supplier_record. Default: unchanged.
    fn enrich_record_from_company_profile(
        &self,
        record: Option<SupplierRecord>,
        _profile_html: &str,
    ) -> Option<SupplierRecord> {
        record
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
        Box::new(wholesalecentral::WholesalecentralScraper),
        Box::new(madeinchina::MadeinchinaScraper),
        Box::new(globalsources::GlobalsourcesScraper),
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
/// - "AwsWafIntegration": the AWS WAF robot-check page (Wholesale
///   Central).
/// To add another site, add a piece of text only its robot-check page has.
const BLOCK_PAGE_MARKERS: &[&str] = &[
    "punish-component",
    "sufei-punish",
    "Please enable JS and disable any ad blocker",
    "AwsWafIntegration",
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
    /// The platform's own record of the supplier (see SupplierRecord).
    /// None on platforms that don't show one.
    pub record: Option<SupplierRecord>,
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
    let mut listing = scraper.parse_listing(&html, page_url);
    let company_key = scraper.company_key(&html);
    let company_url = scraper.extract_company_profile_url(&html);
    let mut record = scraper.supplier_record(&html);

    if supplier.company_name.is_none() && listing.title.is_none() {
        eprintln!(
            "Safely: DEPENDENCY DOWN: B2B fetch for {} - real HTML but no matching content. Total length: {} bytes. First 3000 chars:\n{}",
            page_url,
            html.len(),
            &html[..html.len().min(3000)]
        );
        return None;
    }

    let company_page_missing = !enrich_from_profile_pages(
        scraper.as_ref(),
        platform,
        page_url,
        &html,
        &mut supplier,
        &mut listing,
        &mut record,
    )
    .await;

    Some(B2bPageResult {
        supplier,
        listing,
        company_key,
        company_url,
        company_page_missing,
        record,
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
/// supplier, and certificates and products to the record. Optional: if
/// a fetch fails, both keep what the listing page already gave.
/// Returns true when every company page that exists was loaded (or the
/// platform has none), false when one of them could not be loaded.
async fn enrich_from_profile_pages(
    scraper: &dyn B2bScraper,
    platform: &str,
    listing_url: &str,
    listing_html: &str,
    supplier: &mut B2bSupplierProfile,
    listing: &mut B2bListingProfile,
    record: &mut Option<SupplierRecord>,
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
                let current = std::mem::take(listing);
                *listing = scraper.enrich_listing_from_company_profile(current, &profile_html);
                *record = scraper.enrich_record_from_company_profile(record.take(), &profile_html);
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
    } else if !LISTING_IS_THE_COMPANY_PAGE.contains(&platform) {
        // Not counted as a failure: some platforms and listings have no
        // company page at all. Logged so the reason is visible - except
        // on platforms where the scanned page already is the company
        // page, where it would print on every scan.
        eprintln!(
            "Safely: no company page link found on {} - company details come from the listing page only",
            listing_url
        );
    }
    all_loaded
}

/// Platforms whose scanned page is the company's own profile page
/// (ThomasNet), so there is never a separate company page to fetch.
/// Wholesale Central and Made-in-China are listed because their
/// supplier pages are scanned directly (their product pages always link
/// to the supplier page, so the note would only ever print for supplier
/// pages).
const LISTING_IS_THE_COMPANY_PAGE: &[&str] = &["thomasnet", "wholesalecentral", "madeinchina"];

fn take_and_replace(
    supplier: &mut B2bSupplierProfile,
    f: impl FnOnce(B2bSupplierProfile) -> B2bSupplierProfile,
) {
    let current = std::mem::take(supplier);
    *supplier = f(current);
}

/// Platforms where the page the buyer's browser sent (page_html) is
/// also the LAST backup: read once more if the browser-first read (see
/// BROWSER_FIRST_PLATFORMS) and ScraperAPI both failed. Alibaba is in
/// both lists.
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
    let mut listing = scraper.parse_listing(html, page_url);
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
    let mut record = scraper.supplier_record(html);

    let company_page_missing = !enrich_from_profile_pages(
        scraper.as_ref(),
        platform,
        page_url,
        html,
        &mut supplier,
        &mut listing,
        &mut record,
    )
    .await;

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
        record,
    })
}

/// Platforms where the buyer's own browser is the FIRST source, and
/// ScraperAPI only the backup. Wholesale Central sits behind an AWS WAF
/// robot check: through ScraperAPI a page takes about a minute and is
/// often blocked or comes back empty, while the buyer's browser has
/// already passed the check. The extension sends the page the buyer is
/// on (page_html) and, on a product page, the supplier's page loaded in
/// the same browser (company_page_html). The buyer's own session is
/// used - never a Safely account. Must match BROWSER_FIRST_PLATFORMS in
/// the extension's api.ts. To go back to ScraperAPI first, remove the
/// platform from both lists.
/// Made-in-China: its pages are plain HTML the buyer's browser already
/// has, so reading them there is faster than ScraperAPI and costs no
/// credits. A product page on the main site (www.made-in-china.com)
/// cannot load the supplier's page from the browser (another address),
/// so that one page is fetched through ScraperAPI.
/// Global Sources: the same plain HTML. Its supplier pages are on
/// another address ({shop}.manufacturer.globalsources.com), so the
/// extension loads them through its background script.
/// Alibaba: through ScraperAPI (ultra premium + render) the listing took
/// close to a minute, and Alibaba often answered "Product Not Available"
/// to ScraperAPI's US address (tested 2026-10-29 on the live server). With
/// the company page after it, a scan went over the 100 seconds nginx and
/// Cloudflare wait, and the extension showed "Reload". The extension
/// already sends the page the buyer is on, so it is read first now. The
/// company page (on {shop}.en.alibaba.com) is loaded by the extension's
/// background script; if it is missing or wrong, that one page is
/// fetched through ScraperAPI.
/// Kompass: its robot check (DataDome) blocks ScraperAPI often (it needs
/// ultra premium, the most expensive mode) and blocks any page loaded
/// again without cookies (tested 2026-10-29). The extension copies the
/// product page from the open tab, which already passed the check, and
/// loads the company page with the buyer's cookies.
/// ExportHub: ScraperAPI needs ultra premium there (2026-10-01). Its
/// pages load fine in the buyer's browser, even again without cookies
/// (tested 2026-10-29). The extension copies the product page from the
/// tab and loads both company pages (the company page and its
/// "profile.html" tab, sent as extended_page_html).
/// ThomasNet: the profile page is also the company page, so it is the
/// only page. It loads fine in the buyer's browser, even again without
/// cookies (tested 2026-10-29); the extension sends it with its data
/// script (__NEXT_DATA__), which is where most details are.
/// B2Brazil: its pages load fine in the buyer's browser, even again
/// without cookies (tested 2026-10-29). The extension copies the product
/// page from the tab and loads the company page (/hotsite/{company}).
/// TradeWheel: the same (tested 2026-10-29). The product page is copied
/// from the tab and the company page (/co/{company}/) is loaded from the
/// same site.
/// B2BMap: the same (tested 2026-10-29). The product page is copied from
/// the tab and the company page (b2bmap.com/{company}) is loaded from
/// the same site. With it, every platform reads the browser first and
/// uses ScraperAPI only as the backup.
pub const BROWSER_FIRST_PLATFORMS: &[&str] = &[
    "wholesalecentral",
    "madeinchina",
    "globalsources",
    "alibaba",
    "kompass",
    "exporthub",
    "thomasnet",
    "b2brazil",
    "tradewheel",
    "b2bmap",
];

/// true when `html` is a real page and not the site's robot check.
fn usable_browser_page(html: &str) -> bool {
    looks_like_a_real_page(html) && !looks_like_a_block_page(html)
}

/// true when the company page the browser sent belongs to the same
/// company as the listing (same company key). A page that gives no key
/// is accepted only when the listing gives none either.
fn company_page_matches(scraper: &dyn B2bScraper, listing_html: &str, company_html: &str) -> bool {
    match (
        scraper.company_key(listing_html),
        scraper.company_key(company_html),
    ) {
        (Some(listing), Some(company)) => listing == company,
        (None, None) => true,
        _ => false,
    }
}

/// Reads a B2B page from the buyer's browser FIRST (see
/// BROWSER_FIRST_PLATFORMS). Returns None when the platform is not
/// listed or the page is missing or unreadable - analyze.rs then
/// fetches it through ScraperAPI as before. When the company page from
/// the browser is missing or wrong, only that page is fetched through
/// ScraperAPI. The same for the second company page (ExportHub's
/// profile.html, extended_page_html). The pages are never saved or written to the log (they
/// can contain the buyer's own account details); only sizes are logged.
pub async fn b2b_page_from_browser_first(
    platform: &str,
    page_url: &str,
    page_html: Option<&str>,
    company_page_html: Option<&str>,
    extended_page_html: Option<&str>,
) -> Option<B2bPageResult> {
    if !BROWSER_FIRST_PLATFORMS.contains(&platform) {
        return None;
    }
    if !scraperapi_key_usable() {
        // Same rule as every other B2B scan: no usable key, no scan.
        return None;
    }
    let scraper = get_scraper_for_platform(platform)?;
    let Some(html) = page_html else {
        eprintln!(
            "Safely: no {} page from the browser - fetching it through ScraperAPI",
            platform
        );
        return None;
    };
    if !usable_browser_page(html) {
        eprintln!(
            "Safely: the {} page from the browser could not be used ({} bytes) - fetching it through ScraperAPI",
            platform,
            html.len()
        );
        return None;
    }

    let mut supplier = scraper.parse_supplier(html, page_url);
    let mut listing = scraper.parse_listing(html, page_url);
    if supplier.company_name.is_none() && listing.title.is_none() {
        eprintln!(
            "Safely: nothing readable in the {} page from the browser ({} bytes) - fetching it through ScraperAPI",
            platform,
            html.len()
        );
        return None;
    }
    let company_key = scraper.company_key(html);
    let company_url = scraper.extract_company_profile_url(html);
    let mut record = scraper.supplier_record(html);

    let company_page_missing = if company_url.is_none() {
        // The page is the company's own page: nothing more to load.
        false
    } else {
        match company_page_html {
            Some(company_html)
                if usable_browser_page(company_html)
                    && company_page_matches(scraper.as_ref(), html, company_html) =>
            {
                take_and_replace(&mut supplier, |s| {
                    scraper.enrich_from_company_profile(s, company_html)
                });
                listing = scraper.enrich_listing_from_company_profile(listing, company_html);
                record = scraper.enrich_record_from_company_profile(record.take(), company_html);
                // The second company page (only ExportHub has one).
                match company_url
                    .as_deref()
                    .and_then(|url| scraper.build_extended_profile_url(url))
                {
                    None => false,
                    Some(extended_url) => match extended_page_html {
                        Some(extended_html) if usable_browser_page(extended_html) => {
                            take_and_replace(&mut supplier, |s| {
                                scraper.enrich_from_extended_profile(s, extended_html)
                            });
                            false
                        }
                        other => {
                            eprintln!(
                                "Safely: the {} second company page from the browser could not be used ({}) - fetching it through ScraperAPI",
                                platform,
                                match other {
                                    None => "not sent".to_string(),
                                    Some(e) => format!("{} bytes", e.len()),
                                }
                            );
                            !enrich_from_extended_page(
                                scraper.as_ref(),
                                platform,
                                &extended_url,
                                &mut supplier,
                            )
                            .await
                        }
                    },
                }
            }
            other => {
                eprintln!(
                    "Safely: the {} company page from the browser could not be used ({}) - fetching it through ScraperAPI",
                    platform,
                    match other {
                        None => "not sent".to_string(),
                        Some(c) => format!("{} bytes", c.len()),
                    }
                );
                !enrich_from_profile_pages(
                    scraper.as_ref(),
                    platform,
                    page_url,
                    html,
                    &mut supplier,
                    &mut listing,
                    &mut record,
                )
                .await
            }
        }
    };

    // Nothing is logged when it works - only when the browser copy could
    // not be used and ScraperAPI is used instead (see above).
    Some(B2bPageResult {
        supplier,
        listing,
        company_key,
        company_url,
        company_page_missing,
        record,
    })
}

/// Fetches only the second company page (ExportHub's "profile.html")
/// through ScraperAPI, when the browser's copy could not be used.
/// Returns true when it loaded.
async fn enrich_from_extended_page(
    scraper: &dyn B2bScraper,
    platform: &str,
    extended_url: &str,
    supplier: &mut B2bSupplierProfile,
) -> bool {
    let client = build_scraper_client();
    let fetch_url = wrap_scraper_url_for_platform(extended_url, platform);
    match fetch_company_page(&client, &fetch_url, extended_url).await {
        CompanyPage::Loaded(extended_html) => {
            take_and_replace(supplier, |s| {
                scraper.enrich_from_extended_profile(s, &extended_html)
            });
            true
        }
        CompanyPage::Blocked => {
            eprintln!(
                "Safely: extended company page {} was a robot-check page, not the real page - those details skipped",
                extended_url
            );
            false
        }
        CompanyPage::NotAPage(len) => {
            eprintln!(
                "Safely: DEPENDENCY DOWN: B2B extended-profile fetch for {} returned {} bytes that don't look like a real page - skipping",
                extended_url, len
            );
            false
        }
        CompanyPage::Failed => false,
    }
}

#[cfg(test)]
mod browser_page_tests {
    use super::*;

    /// A small Wholesale Central page: the data rows inside a
    /// self.__next_f.push([1,"..."]) script.
    fn wc_page(rows: &str) -> String {
        let pushed = serde_json::to_string(&serde_json::json!([1, rows])).unwrap();
        format!(
            "<!DOCTYPE html><html><body>{}<script>self.__next_f.push({pushed})</script></body></html>",
            "x".repeat(2500)
        )
    }

    fn wc_product(supplier_id: u32) -> String {
        wc_page(&format!(
            "b:[\"$\",\"div\",null,{{\"product\":{{\"id\":1,\"name\":\"Earphones\",\"supplierName\":\"iPhoenix\",\"supplierId\":{supplier_id},\"subdirectory\":\"iphoenix\",\"productVariants\":[]}}}}]\n"
        ))
    }

    fn wc_supplier(id: u32) -> String {
        wc_page(&format!(
            "2e:{{\"id\":{id},\"supplierBusinessName\":\"iPhoenix\",\"contactInfo\":[{{\"salesNumber\":[\"626-279-6899\"]}}],\"wholesaleLink\":\"https://shopiphoenix.com\"}}\n"
        ))
    }

    #[tokio::test]
    async fn browser_first_is_only_for_listed_platforms() {
        let page = wc_product(63409);
        assert!(
            b2b_page_from_browser_first("someotherplatform", "u", Some(&page), None, None)
                .await
                .is_none(),
            "a platform that is not listed keeps ScraperAPI first"
        );
        assert!(
            BROWSER_FIRST_PLATFORMS.contains(&"alibaba"),
            "Alibaba is read from the browser first"
        );
        assert!(
            BROWSER_FIRST_PLATFORMS.contains(&"kompass"),
            "Kompass is read from the browser first"
        );
        assert!(
            BROWSER_FIRST_PLATFORMS.contains(&"exporthub"),
            "ExportHub is read from the browser first"
        );
        assert!(
            BROWSER_FIRST_PLATFORMS.contains(&"thomasnet"),
            "ThomasNet is read from the browser first"
        );
        assert!(
            BROWSER_FIRST_PLATFORMS.contains(&"b2brazil"),
            "B2Brazil is read from the browser first"
        );
        assert!(
            BROWSER_FIRST_PLATFORMS.contains(&"tradewheel"),
            "TradeWheel is read from the browser first"
        );
        assert!(
            BROWSER_FIRST_PLATFORMS.contains(&"b2bmap"),
            "B2BMap is read from the browser first"
        );
        assert!(
            b2b_page_from_browser_first("wholesalecentral", "u", None, None, None)
                .await
                .is_none(),
            "no page sent: fetched through ScraperAPI"
        );
    }

    #[test]
    fn a_company_page_from_another_supplier_is_not_used() {
        let scraper = get_scraper_for_platform("wholesalecentral").unwrap();
        let product = wc_product(63409);
        assert!(company_page_matches(
            scraper.as_ref(),
            &product,
            &wc_supplier(63409)
        ));
        assert!(!company_page_matches(
            scraper.as_ref(),
            &product,
            &wc_supplier(40952)
        ));
    }

    #[test]
    fn kompass_robot_check_page_is_never_read_as_a_page() {
        // What Kompass sends to a page loaded without cookies (2026-10-29).
        let robot = format!(
            "<html lang=\"en\"><head><title>kompass.com</title></head><body><p id=\"cmsg\">Please enable JS and disable any ad blocker</p><script src=\"https://ct.captcha-delivery.com/i.js\"></script>{}</body></html>",
            "x".repeat(2500)
        );
        assert!(!usable_browser_page(&robot));
    }

    #[test]
    fn the_robot_check_page_is_never_read_as_a_page() {
        let robot = format!(
            "<!DOCTYPE html><html><body><script>AwsWafIntegration.getToken()</script>{}</body></html>",
            "x".repeat(3000)
        );
        assert!(!usable_browser_page(&robot));
        assert!(usable_browser_page(&wc_supplier(1)));
    }

    #[tokio::test]
    async fn product_and_supplier_pages_from_the_browser_make_one_result() {
        unsafe {
            std::env::set_var("SCRAPERAPI_KEY", "k");
        }
        let product = wc_product(63409);
        let supplier_page = wc_supplier(63409);
        let page = b2b_page_from_browser_first(
            "wholesalecentral",
            "https://www.wholesalecentral.com/products/1",
            Some(&product),
            Some(&supplier_page),
            None,
        )
        .await
        .expect("read from the browser");
        assert_eq!(page.listing.title.as_deref(), Some("Earphones"));
        assert_eq!(page.supplier.contact_phone.as_deref(), Some("626-279-6899"));
        assert_eq!(
            page.supplier.website_url.as_deref(),
            Some("https://shopiphoenix.com")
        );
        assert_eq!(page.company_key.as_deref(), Some("63409"));
        assert!(!page.company_page_missing);

        // A supplier page needs nothing more.
        let page = b2b_page_from_browser_first(
            "wholesalecentral",
            "https://www.wholesalecentral.com/suppliers/iphoenix",
            Some(&supplier_page),
            None,
            None,
        )
        .await
        .expect("read from the browser");
        assert_eq!(page.listing.title, None);
        assert!(!page.company_page_missing);
    }

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
