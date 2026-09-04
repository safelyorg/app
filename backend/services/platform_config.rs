use once_cell::sync::Lazy;
use std::collections::HashMap;
use serde_json::from_str;

static PLATFORM_DOMAINS: Lazy<HashMap<String, String>> = Lazy::new(|| {
    let raw = include_str!("../config/platform_domains.json");
    from_str(raw).expect("platform_domains.json must be valid JSON")
});

pub fn get_all_platform_domains() -> HashMap<String, String> {
    PLATFORM_DOMAINS.clone()
}
