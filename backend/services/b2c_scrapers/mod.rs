pub mod olx;

use crate::services::scraper_client::{build_scraper_client, wrap_scraper_url};

// ─────────────────────────────────────────────────────────
// JOB 1: Store page verification (Tier 2) - visits a seller's
// SEPARATE store/profile page to confirm their claimed identity.
// ─────────────────────────────────────────────────────────

pub struct B2cProfileResult {
    pub website: Option<String>,
    pub seller_name_confirmed: bool,
}

pub trait B2cScraper: Send + Sync {
    fn matches_platform(&self, platform: &str) -> bool;
    fn parse(&self, html: &str, expected_seller_name: &str) -> B2cProfileResult;
}

pub fn get_scraper_for_platform(platform: &str) -> Option<Box<dyn B2cScraper>> {
    let scrapers: Vec<Box<dyn B2cScraper>> = vec![Box::new(olx::OlxScraper)];
    scrapers.into_iter().find(|s| s.matches_platform(platform))
}

pub async fn check_store_page(
    platform: &str,
    profile_url: &str,
    expected_seller_name: &str,
) -> Option<B2cProfileResult> {
    let scraper = get_scraper_for_platform(platform)?;
    let client = build_scraper_client();
    let fetch_url = wrap_scraper_url(profile_url);

    // Same, real 3-attempt retry as check_b2b_page - transient
    // network/DNS/ScraperAPI failures are genuinely possible on any
    // platform routed through the shared scraper client, not just
    // ones that specifically need CAPTCHA bypassing.
    let mut last_status = None;
    for attempt in 1..=3 {
        let response = match client.get(&fetch_url).send().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "Safely: store page fetch attempt {} network error for {} - {:?}",
                    attempt, profile_url, e
                );
                continue;
            }
        };

        if response.status().is_success() {
            let html = response.text().await.ok()?;
            return Some(scraper.parse(&html, expected_seller_name));
        }

        last_status = Some(response.status());
        eprintln!(
            "Safely: store page fetch attempt {} failed for {} - status {}",
            attempt,
            profile_url,
            response.status()
        );
    }

    eprintln!(
        "Safely: store page fetch genuinely failed after 3 attempts for {} - last status {:?}",
        profile_url, last_status
    );
    None
}

// ─────────────────────────────────────────────────────────
// JOB 2: Main listing page scraping - fetches and parses the
// PRIMARY listing page itself (title, price, description), as a
// genuine server-side alternative to the extension's client-side
// scrapeOLX(). Separate trait, separate data shape, same module.
// ─────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct ListingPageData {
    pub title: Option<String>,
    pub price: Option<i64>,
    pub description: Option<String>,
    pub seller_name: Option<String>,
    pub location: Option<String>,
    pub platform_id: Option<String>,
    pub seller_profile_url: Option<String>,
    pub last_active: Option<String>,
    pub seller_verified: bool,
    pub seller_rating: Option<f64>,
    pub seller_total_products: Option<i32>,
    pub seller_join_date: Option<String>,
    pub image_urls: Vec<String>,
    pub seller_website: Option<String>,
    pub seller_logo_url: Option<String>,
}

pub trait ListingScraper: Send + Sync {
    fn matches_platform(&self, platform: &str) -> bool;
    fn parse(&self, html: &str) -> ListingPageData;
}

pub fn get_listing_scraper_for_platform(platform: &str) -> Option<Box<dyn ListingScraper>> {
    let scrapers: Vec<Box<dyn ListingScraper>> = vec![Box::new(olx::OlxListingScraper)];
    scrapers.into_iter().find(|s| s.matches_platform(platform))
}

pub async fn check_listing_page(platform: &str, listing_url: &str) -> Option<ListingPageData> {
    let scraper = get_listing_scraper_for_platform(platform)?;
    let client = build_scraper_client();
    let fetch_url = wrap_scraper_url(listing_url);

    let mut last_status = None;
    for attempt in 1..=3 {
        let response = match client.get(&fetch_url).send().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "Safely: listing page fetch attempt {} network error for {} - {:?}",
                    attempt, listing_url, e
                );
                continue;
            }
        };

        if response.status().is_success() {
            let html = response.text().await.ok()?;
            return Some(scraper.parse(&html));
        }

        last_status = Some(response.status());
        eprintln!(
            "Safely: listing page fetch attempt {} failed for {} - status {}",
            attempt,
            listing_url,
            response.status()
        );
    }

    eprintln!(
        "Safely: listing page fetch genuinely failed after 3 attempts for {} - last status {:?}",
        listing_url, last_status
    );
    None
}

pub fn requires_client_side_scraping(platform: &str) -> bool {
    !matches!(platform, "olx")
}
