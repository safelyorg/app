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

/// Real, per-platform country targeting - each B2B platform is
/// actually based in (or primarily serves) a different real region,
/// so one hardcoded country_code would be wrong for the others.
/// Returns None for platforms where no specific country genuinely
/// helps (ScraperAPI then picks its own default automatically).
fn country_code_for_platform(platform: &str) -> Option<&'static str> {
    match platform {
        "exporthub" => Some("us"),
        "b2brazil" => Some("us"),
        "alibaba" => Some("us"),
        "olx" => Some("pk"),
        _ => None,
    }
}

pub fn wrap_scraper_url(target_url: &str) -> String {
    wrap_scraper_url_for_platform(target_url, "")
}

/// The real, platform-aware version - prefer this at every call site
/// going forward. wrap_scraper_url() above is kept only so any
/// not-yet-updated call site keeps compiling; it applies no country
/// targeting at all.
pub fn wrap_scraper_url_for_platform(target_url: &str, platform: &str) -> String {
    if let Ok(api_key) = var("SCRAPERAPI_KEY") {
        let encoded_url = encode(target_url);
        let mut url = format!(
            "https://api.scraperapi.com/?api_key={}&url={}&render=true&premium=true",
            api_key, encoded_url
        );
        if let Some(country) = country_code_for_platform(platform) {
            url.push_str("&country_code=");
            url.push_str(country);
        }
        url
    } else {
        target_url.to_string()
    }
}
