pub mod olx;

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

/// Fetches a seller's real, live store/profile page and parses it
/// using whichever scraper matches this platform. Never panics or
/// propagates an error upward - a failed fetch or missing scraper
/// just means no extra data this time, never a broken analysis.
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
