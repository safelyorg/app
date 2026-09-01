pub mod olx;

pub struct StorePageResult {
    pub website: Option<String>,
    pub seller_name_confirmed: bool,
}

pub trait StorePageScraper: Send + Sync {
    fn matches_platform(&self, platform: &str) -> bool;
    fn parse(&self, html: &str, expected_seller_name: &str) -> StorePageResult;
}

pub fn get_scraper_for_platform(platform: &str) -> Option<Box<dyn StorePageScraper>> {
    let scrapers: Vec<Box<dyn StorePageScraper>> = vec![Box::new(olx::OlxStoreScraper)];
    scrapers.into_iter().find(|s| s.matches_platform(platform))
}

/// Fetches a seller's real, live store/profile page and parses it
/// using whichever scraper matches this platform. Never panics or
/// propagates an error upward - a failed fetch or missing scraper
/// just means no extra data this time, never a broken analysis.
pub async fn check_store_page(
    platform: &str,
    profile_url: &str,
    expected_seller_name: &str,
) -> Option<StorePageResult> {
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
