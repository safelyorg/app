pub mod olx;

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
    let client = reqwest::Client::new();
    let response = client.get(profile_url).send().await.ok()?;

    if !response.status().is_success() {
        eprintln!(
            "Safely: store page fetch failed for {} - status {}",
            profile_url,
            response.status()
        );
        return None;
    }

    let html = response.text().await.ok()?;
    Some(scraper.parse(&html, expected_seller_name))
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
    let client = reqwest::Client::new();
    let response = client.get(listing_url).send().await.ok()?;

    if !response.status().is_success() {
        eprintln!(
            "Safely: listing page fetch failed for {} - status {}",
            listing_url,
            response.status()
        );
        return None;
    }

    let html = response.text().await.ok()?;
    Some(scraper.parse(&html))
}

pub fn requires_client_side_scraping(platform: &str) -> bool {
    !matches!(platform, "olx")
}
