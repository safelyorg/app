use reqwest::Client;
use std::env::var;
use urlencoding::encode;

/// The one, real, shared HTTP client every scraper fetches through -
/// genuinely plain now, since ScraperAPI's real, actual integration
/// method wraps the TARGET URL itself, rather than configuring the
/// client as a proxy.
pub fn build_scraper_client() -> Client {
    let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
    Client::builder()
        .user_agent(user_agent)
        .build()
        .unwrap_or_else(|_| Client::new())
}

/// Wraps a real, actual target URL inside ScraperAPI's own, real
/// endpoint - if a real, live API key is configured, every scraper
/// fetch goes through ScraperAPI's genuine, own unblocking service
/// instead of fetching the target directly. Falls back to fetching
/// the target directly if no key is configured, so this stays
/// entirely optional, never breaking OLX/B2Brazil if unset.
pub fn wrap_scraper_url(target_url: &str) -> String {
    if let Ok(api_key) = var("SCRAPERAPI_KEY") {
        let encoded_url = encode(target_url);
        format!(
            "https://api.scraperapi.com/?api_key={}&url={}&render=true",
            api_key, encoded_url
        )
    } else {
        target_url.to_string()
    }
}
