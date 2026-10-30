use serde::Deserialize;
use std::env::var;

#[derive(Debug, Deserialize)]
struct WhoisRegistrar {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WhoisJsonResponse {
    registered: bool,
    created: Option<String>,
    registrar: Option<WhoisRegistrar>,
}

#[derive(Debug)]
pub struct WhoisResult {
    pub registered: bool,
    pub created: Option<String>,
    pub registrar: Option<String>,
}

/// Checks a real domain's registration details via WhoisJSON. This is
/// Layer 3's first genuine "Active collection" source - a real,
/// external, slow-ish API call, unlike the instant, free checks built
/// so far.
///
/// It never panics or propagates an error upward if the lookup fails
/// for any reason - a missing key, a network issue, an unregistered
/// domain - since a failed WHOIS check should never block or crash a
/// real analysis; it just means less information is available.
pub async fn check_domain_whois(domain: &str) -> Option<WhoisResult> {
    let api_key = match var("WHOIS_API_KEY") {
        Ok(k) => k,
        Err(_) => {
            eprintln!("Safely: WHOIS_API_KEY is not set in the environment at all");
            return None;
        }
    };
    let client = reqwest::Client::new();

    let url = format!("https://whoisjson.com/api/v1/whois?domain={}", domain);

    let response = client
        .get(&url)
        .header("Authorization", format!("TOKEN={}", api_key))
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        eprintln!(
            "Safely: WHOIS lookup failed for domain {} - status {}",
            domain,
            response.status()
        );
        return None;
    }

    let raw_body = response.text().await.ok()?;
    let parsed: WhoisJsonResponse = match serde_json::from_str(&raw_body) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "Safely: WHOIS response for {} failed to parse: {} - raw body: {}",
                domain, e, raw_body
            );
            return None;
        }
    };

    Some(WhoisResult {
        registered: parsed.registered,
        created: parsed.created,
        registrar: parsed.registrar.and_then(|r| r.name),
    })
}
