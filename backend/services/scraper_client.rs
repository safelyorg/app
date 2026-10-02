use reqwest::Client;
use std::env::var;
use std::sync::Once;
use urlencoding::encode;

/// Makes sure the "key is missing" warning is printed only once per run,
/// not once per page.
static MISSING_KEY_WARNING: Once = Once::new();

/// Prints a clear warning (once) when SCRAPERAPI_KEY is not set. Pages
/// still load, but directly from this computer instead of through
/// ScraperAPI - so no IP rotation, and sites may block the scan. The key
/// itself is never printed.
fn warn_missing_scraperapi_key() {
    MISSING_KEY_WARNING.call_once(|| {
        eprintln!(
            "Safely: WARNING: SCRAPERAPI_KEY is not set - pages are being fetched directly, not through ScraperAPI. Add SCRAPERAPI_KEY to backend/.env and restart."
        );
    });
}

/// Called once when the server starts (main.rs), so a missing key is
/// noticed right away instead of on the first scan. Prints nothing when
/// the key is set. The key itself is never printed.
pub fn warn_if_scraperapi_key_missing() {
    if !scraperapi_key_is_set() {
        warn_missing_scraperapi_key();
    }
}

/// true when SCRAPERAPI_KEY is set and not empty.
pub fn scraperapi_key_is_set() -> bool {
    var("SCRAPERAPI_KEY")
        .map(|key| !key.trim().is_empty())
        .unwrap_or(false)
}

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
pub fn country_code_for_platform(platform: &str) -> Option<&'static str> {
    match platform {
        "exporthub" => Some("us"),
        "b2brazil" => Some("us"),
        "alibaba" => Some("us"),
        "tradewheel" => Some("us"),
        "b2bmap" => Some("us"),
        "thomasnet" => Some("us"),
        "kompass" => Some("us"),
        "olx" => Some("pk"),
        _ => None,
    }
}

/// Platforms whose pages are fetched WITHOUT render=true (no browser
/// running the page's JavaScript):
/// - thomasnet, kompass: serve already-rendered HTML, and render=true
///   risked triggering their anti-bot detection.
/// - exporthub: everything the scraper reads is in the plain page
///   (lazy images keep their real address in data-src, which the
///   scraper already reads). Rendering made ExportHub pages slow enough
///   that ScraperAPI gave up with "500" while ExportHub was slow.
///   Plain fetches are also cheaper in ScraperAPI credits.
/// To turn rendering back on for a platform, remove it from this list.
const NO_RENDER_PLATFORMS: &[&str] = &["thomasnet", "kompass", "exporthub"];

/// Platforms that ScraperAPI treats as "protected domains" and only
/// fetches with ultra_premium=true. ScraperAPI's own error says so when
/// it is needed: "Protected domains may require adding premium=true OR
/// ultra_premium=true". Ultra premium costs more credits per page, so
/// only platforms that actually need it are listed.
/// - kompass: always needed it.
/// - exporthub: started needing it on 2026-10-01 (every fetch failed
///   with that message on plain premium).
/// - alibaba: started needing it on 2026-10-01 afternoon (every listing
///   failed with "500 Request failed" on plain premium, and loaded with
///   ultra premium).
/// To stop using it for a platform, remove it from this list.
const ULTRA_PREMIUM_PLATFORMS: &[&str] = &["kompass", "exporthub", "alibaba"];

pub fn wrap_scraper_url(target_url: &str) -> String {
    wrap_scraper_url_for_platform(target_url, "")
}

/// The real, platform-aware version - prefer this at every call site
/// going forward. wrap_scraper_url() above is kept only so any
/// not-yet-updated call site keeps compiling; it applies no country
/// targeting at all.
///
/// Each platform's required ScraperAPI tier is decided upfront here,
/// not discovered live - Kompass is known to need ultra_premium=true
/// (ScraperAPI's own error message for it explicitly asks for it:
/// "Protected domains may require adding premium=true OR
/// ultra_premium=true"), and render=true is skipped for the platforms
/// in NO_RENDER_PLATFORMS. Every other platform stays on plain
/// premium=true + render=true.
pub fn wrap_scraper_url_for_platform(target_url: &str, platform: &str) -> String {
    let api_key = var("SCRAPERAPI_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty());
    if let Some(api_key) = api_key {
        let encoded_url = encode(target_url);
        let needs_render = !NO_RENDER_PLATFORMS.contains(&platform);
        let needs_ultra_premium = ULTRA_PREMIUM_PLATFORMS.contains(&platform);

        let mut url = format!(
            "https://api.scraperapi.com/?api_key={}&url={}&premium=true",
            api_key, encoded_url
        );
        if needs_ultra_premium {
            url.push_str("&ultra_premium=true");
        }
        if needs_render {
            url.push_str("&render=true");
        }
        if let Some(country) = country_code_for_platform(platform) {
            url.push_str("&country_code=");
            url.push_str(country);
        }
        url
    } else {
        warn_missing_scraperapi_key();
        target_url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_is_skipped_only_for_the_listed_platforms() {
        unsafe {
            std::env::set_var("SCRAPERAPI_KEY", "k");
        }
        let eh = wrap_scraper_url_for_platform("https://www.exporthub.com/x", "exporthub");
        assert!(
            !eh.contains("render=true"),
            "ExportHub is fetched without rendering"
        );
        assert!(eh.contains("premium=true") && eh.contains("country_code=us"));
        assert!(
            eh.contains("ultra_premium=true"),
            "ExportHub is now a protected domain"
        );

        let ali = wrap_scraper_url_for_platform("https://www.alibaba.com/x", "alibaba");
        assert!(ali.contains("render=true"), "Alibaba still renders");
        assert!(
            ali.contains("ultra_premium=true"),
            "Alibaba now needs ultra premium"
        );

        let kompass = wrap_scraper_url_for_platform("https://www.kompass.com/x", "kompass");
        assert!(!kompass.contains("render=true") && kompass.contains("ultra_premium=true"));
    }
}
