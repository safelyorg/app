use backend::services::scraper_client::{wrap_scraper_url, wrap_scraper_url_for_platform};
use serial_test::serial;

#[test]
#[serial]
fn wrap_scraper_url_for_platform_passes_the_target_through_unchanged_when_no_key_is_set() {
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    let result = wrap_scraper_url_for_platform("https://example.com/listing", "alibaba");
    assert_eq!(result, "https://example.com/listing");
}

#[test]
#[serial]
fn wrap_scraper_url_passes_the_target_through_unchanged_when_no_key_is_set() {
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    let result = wrap_scraper_url("https://example.com/listing");
    assert_eq!(result, "https://example.com/listing");
}

#[test]
#[serial]
fn wrap_scraper_url_for_platform_builds_the_real_scraperapi_url_with_the_key_when_set() {
    unsafe {
        std::env::set_var("SCRAPERAPI_KEY", "test-key-123");
    }
    let result = wrap_scraper_url_for_platform("https://example.com/listing", "unknown-platform");
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }

    assert!(result.starts_with("https://api.scraperapi.com/?"));
    assert!(result.contains("api_key=test-key-123"));
    assert!(result.contains("url=https%3A%2F%2Fexample.com%2Flisting"));
    assert!(result.contains("render=true"));
    assert!(result.contains("premium=true"));
    assert!(
        !result.contains("country_code="),
        "expected no country_code param for a platform with no real mapping, got: {}",
        result
    );
}

#[test]
#[serial]
fn wrap_scraper_url_for_platform_appends_the_real_country_code_for_exporthub() {
    unsafe {
        std::env::set_var("SCRAPERAPI_KEY", "test-key-123");
    }
    let result = wrap_scraper_url_for_platform("https://exporthub.com/x", "exporthub");
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    assert!(result.contains("country_code=us"));
}

#[test]
#[serial]
fn wrap_scraper_url_for_platform_appends_the_real_country_code_for_b2brazil() {
    unsafe {
        std::env::set_var("SCRAPERAPI_KEY", "test-key-123");
    }
    let result = wrap_scraper_url_for_platform("https://b2brazil.com/x", "b2brazil");
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    assert!(result.contains("country_code=us"));
}

#[test]
#[serial]
fn wrap_scraper_url_for_platform_appends_the_real_country_code_for_alibaba() {
    unsafe {
        std::env::set_var("SCRAPERAPI_KEY", "test-key-123");
    }
    let result = wrap_scraper_url_for_platform("https://alibaba.com/x", "alibaba");
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    assert!(result.contains("country_code=us"));
}

#[test]
#[serial]
fn wrap_scraper_url_for_platform_appends_the_real_country_code_for_thomasnet() {
    unsafe {
        std::env::set_var("SCRAPERAPI_KEY", "test-key-123");
    }
    let result = wrap_scraper_url_for_platform("https://thomasnet.com/x", "thomasnet");
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    assert!(result.contains("country_code=us"));
}

#[test]
#[serial]
fn wrap_scraper_url_for_platform_appends_the_real_country_code_for_olx() {
    unsafe {
        std::env::set_var("SCRAPERAPI_KEY", "test-key-123");
    }
    let result = wrap_scraper_url_for_platform("https://olx.com.pk/x", "olx");
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    assert!(result.contains("country_code=pk"));
}

#[test]
#[serial]
fn wrap_scraper_url_for_platform_appends_the_real_country_code_for_tradewheel() {
    unsafe {
        std::env::set_var("SCRAPERAPI_KEY", "test-key-123");
    }
    let result = wrap_scraper_url_for_platform("https://tradewheel.com/x", "tradewheel");
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    assert!(result.contains("country_code=us"));
}

#[test]
#[serial]
fn wrap_scraper_url_for_platform_omits_country_code_for_a_genuinely_unmapped_platform() {
    // Confirms there's still a real, honest fallback to "no country
    // targeting" for a platform that truly isn't in the match at all,
    // rather than every branch secretly defaulting to "us" no matter
    // what string comes in.
    unsafe {
        std::env::set_var("SCRAPERAPI_KEY", "test-key-123");
    }
    let result = wrap_scraper_url_for_platform("https://example.com/x", "some-future-platform");
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    assert!(!result.contains("country_code="));
}

#[test]
#[serial]
fn wrap_scraper_url_old_alias_never_appends_a_country_code_even_when_the_key_is_set() {
    // wrap_scraper_url() delegates to wrap_scraper_url_for_platform
    // with an empty platform string - confirms any not-yet-migrated
    // call site keeps working exactly as before, with no country
    // targeting applied at all.
    unsafe {
        std::env::set_var("SCRAPERAPI_KEY", "test-key-123");
    }
    let result = wrap_scraper_url("https://example.com/x");
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    assert!(!result.contains("country_code="));
    assert!(result.contains("api_key=test-key-123"));
}

#[test]
#[serial]
fn wrap_scraper_url_for_platform_percent_encodes_special_characters_in_the_target_url() {
    unsafe {
        std::env::set_var("SCRAPERAPI_KEY", "test-key-123");
    }
    let result = wrap_scraper_url_for_platform("https://example.com/x?q=a&b=c", "alibaba");
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    assert!(
        result.contains("url=https%3A%2F%2Fexample.com%2Fx%3Fq%3Da%26b%3Dc"),
        "expected the target URL's own query string to be safely percent-encoded so it can't corrupt ScraperAPI's own url= param, got: {}",
        result
    );
}

#[test]
#[serial]
fn wrap_scraper_url_for_platform_omits_render_for_thomasnet_to_avoid_its_own_anti_bot_detection() {
    // ThomasNet's content is server-rendered already - render=true spins
    // up a full headless browser for no benefit and was suspected of
    // triggering ThomasNet's own anti-bot detection (consistent 500s
    // with no other explanation).
    unsafe {
        std::env::set_var("SCRAPERAPI_KEY", "test-key-123");
    }
    let result = wrap_scraper_url_for_platform("https://thomasnet.com/x", "thomasnet");
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    assert!(
        !result.contains("render=true"),
        "expected render=true to be genuinely omitted for thomasnet, got: {}",
        result
    );
    assert!(result.contains("premium=true"));
    assert!(result.contains("country_code=us"));
}

#[test]
#[serial]
fn wrap_scraper_url_for_platform_still_includes_render_for_every_other_platform() {
    unsafe {
        std::env::set_var("SCRAPERAPI_KEY", "test-key-123");
    }
    let result = wrap_scraper_url_for_platform("https://alibaba.com/x", "alibaba");
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    assert!(
        result.contains("render=true"),
        "expected render=true to still be present for a platform other than thomasnet, got: {}",
        result
    );
}
