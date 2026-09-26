use crate::models::analysis::Signal;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::HashSet, env::var, sync::Arc};
use tokio::{sync::Semaphore, task::JoinSet};

#[derive(Debug)]
pub struct SellerIdentifiers {
    pub name: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub website: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug)]
pub struct OsintMatch {
    pub matched_identifiers: Vec<String>,
    pub confidence: String, // "strong", "weak", "none"
}

#[derive(Debug, Deserialize)]
pub struct SerperOrganicResult {
    pub title: String,
    pub link: String,
    pub snippet: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SerperSearchResponse {
    #[serde(default)]
    pub organic: Vec<SerperOrganicResult>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SocialCandidateLink {
    pub platform: String,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct PlatformCheckResult {
    pub platform: String,
    pub variant_searched: String,
    pub found: bool,
    pub candidates: Vec<SocialCandidateLink>,
}

/// The real, complete scam-word list, split into small, real groups
/// - each group becomes its OWN, separate Google search, so every
/// single word genuinely gets searched for, rather than cramming
/// everything into one query where Google might only weight the
/// first few terms.
const SCAM_WORD_SEARCH_GROUPS: &[&str] = &[
    "golpe OR scam OR fraude",
    "cuidado OR reclamação OR estelionato",
    "picareta OR enganou OR \"não recomendo\"",
    "\"não entregou\" OR sumiu OR processo",
    "polícia OR ripoff OR beware OR avoid OR scammed",
];

pub fn score_identifier_match(seller: &SellerIdentifiers, found_text: &str) -> OsintMatch {
    let lower_text = found_text.to_lowercase();
    let mut matched = Vec::new();

    if let Some(name) = &seller.name {
        let lower_name = name.to_lowercase();
        let words: Vec<&str> = lower_name.split_whitespace().collect();
        let core_name = if words.len() >= 2 {
            format!("{} {}", words[0], words[1])
        } else {
            lower_name.clone()
        };
        if !lower_name.trim().is_empty()
            && (lower_text.contains(&lower_name) || lower_text.contains(&core_name))
        {
            matched.push("name".to_string());
        }
    }

    if let Some(phone) = &seller.phone {
        let digits_only: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
        if !digits_only.is_empty()
            && lower_text
                .replace(|c: char| !c.is_ascii_digit(), "")
                .contains(&digits_only)
        {
            matched.push("phone".to_string());
        }
    }

    if let Some(email) = &seller.email {
        if !email.trim().is_empty() && lower_text.contains(&email.to_lowercase()) {
            matched.push("email".to_string());
        }
    }

    if let Some(website) = &seller.website {
        if !website.trim().is_empty() && lower_text.contains(&website.to_lowercase()) {
            matched.push("website".to_string());
        }
    }

    if let Some(location) = &seller.location {
        let clean_loc = location
            .split(['/', '|'])
            .next()
            .map(|s| s.trim().to_lowercase())
            .unwrap_or_default();
        if !clean_loc.is_empty() && lower_text.contains(&clean_loc) {
            matched.push("location".to_string());
        }
    }

    const SCAM_WORDS: &[&str] = &[
        "golpe",
        "scam",
        "fraude",
        "cuidado",
        "reclamação",
        "estelionato",
        "picareta",
        "enganou",
        "não recomendo",
        "não entregou",
        "sumiu",
        "processo",
        "polícia",
        "ripoff",
        "beware",
        "avoid",
        "scammed",
    ];
    let contains_scam_language = SCAM_WORDS.iter().any(|w| lower_text.contains(w));
    if contains_scam_language {
        matched.push("scam_language".to_string());
    }

    let confidence = if matched.len() >= 2 {
        "strong"
    } else if matched.len() == 1 {
        "weak"
    } else {
        "none"
    };

    OsintMatch {
        matched_identifiers: matched,
        confidence: confidence.to_string(),
    }
}

/// Runs one, real, live search query against Serper's actual API,
/// with an optional country code (e.g. "br" for Brazil) to genuinely
/// match local, real results - without this, Serper may return
/// generic, less relevant results for the seller's actual region.
pub async fn run_serper_search(
    query: &str,
    country_code: Option<&str>,
) -> Option<SerperSearchResponse> {
    let api_key = var("SERPER_API_KEY").ok()?;
    let client = Client::new();
    let mut body = json!({ "q": query });
    if let Some(gl) = country_code {
        body["gl"] = json!(gl);
    }
    let response = client
        .post("https://google.serper.dev/search")
        .header("X-API-KEY", api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<SerperSearchResponse>().await.ok()
}

/// Confirms whether a real, found URL belongs to one of Safely's OWN,
/// already-scraped platforms - genuinely NOT independent OSINT
/// evidence, since a company's page existing on the same platform
/// it's already listed on proves nothing new. Reuses the same, real
/// config/platform_domains.json already powering the extension's
/// Domain check signal - one, single, shared source of truth.
fn is_own_platform_domain(url: &str) -> bool {
    crate::services::platform_config::get_all_platform_domains()
        .values()
        .any(|domain| url.contains(domain))
}

/// Real, shared name-variant builder - first-two-words and full-name,
/// deduplicated - used by both the primary and the location-fallback
/// query builders below, so the two tiers always search the exact
/// same set of name variants.
fn build_name_variants(company_name: &str) -> Vec<String> {
    let words: Vec<&str> = company_name.split_whitespace().collect();
    let mut variants: Vec<String> = Vec::new();
    if words.len() >= 2 {
        variants.push(format!("{} {}", words[0], words[1]));
    }
    variants.push(company_name.to_string());
    variants.dedup();
    variants
}

/// TIER 1 - the real, name-ONLY matrix. Deliberately never combines
/// the company name with the location as a mandatory AND term inside
/// one query - a real company's own Facebook/LinkedIn page almost
/// never has its city+state indexed verbatim next to its name, so
/// forcing that combination silently throws away genuine matches
/// (confirmed on Deva Inc / ThomasNet: 0 of 40 checks found anything,
/// purely because every query required "Deva Inc" AND "Tustin, CA"
/// together). This tier runs for every listing, on every platform,
/// always - location is handled separately, as a fallback, below.
pub fn build_osint_query_matrix(
    company_name: Option<&str>,
    contact_name: Option<&str>,
    phone: Option<&str>,
) -> Vec<(String, String, String)> {
    let mut queries = Vec::new();
    let Some(name) = company_name.filter(|n| !n.trim().is_empty()) else {
        return queries;
    };

    let variants = build_name_variants(name);

    let platforms: Vec<(&str, &str)> = vec![
        ("Facebook", "site:facebook.com"),
        ("LinkedIn", "site:linkedin.com"),
        ("TikTok", "site:tiktok.com"),
        ("Instagram", "site:instagram.com"),
        ("Reddit", "site:reddit.com"),
        ("Trustpilot", "site:trustpilot.com"),
    ];

    for variant in &variants {
        for (platform_label, site_filter) in &platforms {
            queries.push((
                platform_label.to_string(),
                format!("{} \"{}\"", site_filter, variant),
                variant.clone(),
            ));
            let phone_part = phone.map(|p| format!(" \"{}\"", p)).unwrap_or_default();
            for word_group in SCAM_WORD_SEARCH_GROUPS {
                queries.push((
                    platform_label.to_string(),
                    format!(
                        "{} \"{}\"{} ({})",
                        site_filter, variant, phone_part, word_group
                    ),
                    variant.clone(),
                ));
            }
        }
        queries.push((
            "Reviews".to_string(),
            format!(
                "\"{}\" reviews OR reclameaqui OR reclamação OR avaliação",
                variant
            ),
            variant.clone(),
        ));
    }

    // A real, strong, dedicated search for the individual contact
    // person, when their name and a genuine, unmasked phone number
    // are both available - the strongest possible combination. Kept
    // as-is: a phone number is a hard identifier, not a soft one
    // like a city name, so it's fine for this pairing to stay
    // mandatory.
    if let (Some(contact), Some(real_phone)) = (contact_name, phone) {
        if !contact.trim().is_empty() && !real_phone.trim().is_empty() {
            for (platform_label, site_filter) in &[
                ("Facebook", "site:facebook.com"),
                ("LinkedIn", "site:linkedin.com"),
            ] {
                queries.push((
                    format!("Contact ({})", platform_label),
                    format!("{} \"{}\" \"{}\"", site_filter, contact, real_phone),
                    contact.to_string(),
                ));
            }
            queries.push((
                "Contact (Web)".to_string(),
                format!("\"{}\" \"{}\"", contact, real_phone),
                contact.to_string(),
            ));
        }
    }

    queries
}

/// TIER 2 - the real location-FALLBACK matrix. Only ever built for
/// the specific (platform, variant) pairs Tier 1 genuinely came up
/// empty on - never run blind, since re-searching everything with
/// location attached would just reintroduce the same problem this
/// whole split exists to fix. Same platform list, same name variants
/// - the only difference is the location is now appended, used here
/// as a genuine disambiguation refinement, not a gate.
pub fn build_location_fallback_queries(
    company_name: &str,
    raw_location: &str,
    needed: &HashSet<(String, String)>,
) -> Vec<(String, String, String)> {
    let mut queries = Vec::new();

    let clean_location = raw_location
        .split(['/', '|'])
        .next()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let Some(location) = clean_location else {
        return queries;
    };

    let variants = build_name_variants(company_name);

    let platforms: Vec<(&str, &str)> = vec![
        ("Facebook", "site:facebook.com"),
        ("LinkedIn", "site:linkedin.com"),
        ("TikTok", "site:tiktok.com"),
        ("Instagram", "site:instagram.com"),
        ("Reddit", "site:reddit.com"),
        ("Trustpilot", "site:trustpilot.com"),
    ];

    for variant in &variants {
        for (platform_label, site_filter) in &platforms {
            let key = (platform_label.to_string(), variant.clone());
            if !needed.contains(&key) {
                continue;
            }
            queries.push((
                platform_label.to_string(),
                format!("{} \"{}\" \"{}\"", site_filter, variant, location),
                variant.clone(),
            ));
        }
    }

    queries
}

/// Runs one batch of (platform, query, variant) jobs against Serper,
/// in parallel (bounded by a shared semaphore), and reports back a
/// PlatformCheckResult per job plus whether the underlying Serper
/// call itself succeeded - shared by both Tier 1 and Tier 2 so the
/// two tiers behave identically and failures are counted the same
/// way in both.
async fn run_query_batch(
    queries: Vec<(String, String, String)>,
    country_code: &str,
) -> (Vec<PlatformCheckResult>, u32) {
    let semaphore = Arc::new(Semaphore::new(10));
    let mut join_set = JoinSet::new();
    for (platform_label, query, variant) in queries {
        let permit_holder = semaphore.clone();
        let country_code = country_code.to_string();
        join_set.spawn(async move {
            let _permit = permit_holder.acquire().await.ok();
            let real_results = run_serper_search(&query, Some(&country_code)).await;
            let search_succeeded = real_results.is_some();
            let mut real_candidates = Vec::new();
            if let Some(response) = real_results {
                for r in response.organic.iter().take(5) {
                    if is_own_platform_domain(&r.link) {
                        continue;
                    }
                    let haystack = format!("{} {}", r.title, r.snippet.as_deref().unwrap_or(""))
                        .to_lowercase();
                    let variant_lower = variant.to_lowercase();
                    if !haystack.contains(&variant_lower) {
                        continue;
                    }
                    real_candidates.push(SocialCandidateLink {
                        platform: platform_label.clone(),
                        title: r.title.clone(),
                        url: r.link.clone(),
                    });
                }
            }
            (
                PlatformCheckResult {
                    platform: platform_label,
                    variant_searched: variant,
                    found: !real_candidates.is_empty(),
                    candidates: real_candidates,
                },
                search_succeeded,
            )
        });
    }

    let mut results = Vec::new();
    let mut failures = 0u32;
    while let Some(joined) = join_set.join_next().await {
        if let Ok((result, search_succeeded)) = joined {
            if !search_succeeded {
                failures += 1;
            }
            results.push(result);
        }
    }
    (results, failures)
}

/// Runs the REAL, complete, two-tier matrix - every name variant
/// against every platform, name-only first, with location genuinely
/// held back and only spent on the specific platform+variant pairs
/// that came up empty - and reports back EVERY result, found or not,
/// so the final signal can honestly show exactly what was checked.
pub async fn build_social_presence_matrix(
    company_name: Option<&str>,
    contact_name: Option<&str>,
    location: Option<&str>,
    phone: Option<&str>,
    platform: &str,
) -> Result<(Signal, Vec<PlatformCheckResult>), String> {
    let primary_queries = build_osint_query_matrix(company_name, contact_name, phone);
    if primary_queries.is_empty() {
        return Ok((
            Signal {
                label: "Social presence check".to_string(),
                sub: "No company name was available to search with.".to_string(),
                value: "Not checked".to_string(),
                signal_type: "info".to_string(),
                category: "external_intelligence".to_string(),
                check_type: "existence".to_string(),
            },
            Vec::new(),
        ));
    }

    // Real, per-platform region for OSINT searches - a company
    // listed on a US-focused directory like ThomasNet should be
    // searched with US-region results, not Brazil's. Applies to
    // BOTH tiers, for every platform, not just ThomasNet.
    let country_code = match platform {
        "b2brazil" => "br",
        _ => "us",
    };

    let total_primary = primary_queries.len();
    let (mut results, mut real_search_failures) =
        run_query_batch(primary_queries, country_code).await;
    let mut total_queries = total_primary;

    // TIER 2 - only for the platform+variant pairs Tier 1 genuinely
    // found nothing on, and only when we actually have a location to
    // try. This is the real fallback: location refines an already-
    // empty result, it never gates the first attempt.
    let clean_location = location
        .and_then(|l| l.split(['/', '|']).next())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    if let (Some(name), Some(loc)) = (
        company_name.filter(|n| !n.trim().is_empty()),
        clean_location,
    ) {
        let needed: HashSet<(String, String)> = results
            .iter()
            .filter(|r| !r.found)
            .map(|r| (r.platform.clone(), r.variant_searched.clone()))
            .collect();

        if !needed.is_empty() {
            let fallback_queries = build_location_fallback_queries(name, loc, &needed);
            if !fallback_queries.is_empty() {
                total_queries += fallback_queries.len();
                let (fallback_results, fallback_failures) =
                    run_query_batch(fallback_queries, country_code).await;
                real_search_failures += fallback_failures;

                // Merge: a Tier 2 hit fills in the matching Tier 1
                // row (same platform + same variant) instead of
                // adding a duplicate row.
                for fallback in fallback_results {
                    if !fallback.found {
                        continue;
                    }
                    if let Some(existing) = results.iter_mut().find(|r| {
                        r.platform == fallback.platform
                            && r.variant_searched == fallback.variant_searched
                    }) {
                        existing.found = true;
                        existing.candidates = fallback.candidates;
                    }
                }
            }
        }
    }

    let failure_rate = real_search_failures as f64 / total_queries.max(1) as f64;
    if failure_rate > 0.5 {
        eprintln!(
            "Safely: DEPENDENCY DOWN: Serper real failure rate {:.0}% ({}/{}) - likely SERPER_API_KEY exhausted or invalid",
            failure_rate * 100.0,
            real_search_failures,
            total_queries
        );
        return Err(
            "This check could not be completed right now - Serper appears to be down".to_string(),
        );
    }

    let found_count = results.iter().filter(|r| r.found).count();
    let signal = Signal {
        label: "Social presence check".to_string(),
        sub: format!(
            "{} of {} platform checks found a real, candidate result.",
            found_count,
            results.len()
        ),
        value: if found_count > 0 {
            "Candidates found".to_string()
        } else {
            "No presence found".to_string()
        },
        signal_type: if found_count > 0 {
            "info".to_string()
        } else {
            "caution".to_string()
        },
        category: "external_intelligence".to_string(),
        check_type: "existence".to_string(),
    };

    Ok((signal, results))
}

/// Fetches ONE, real, specific candidate link's actual page content,
/// and checks it against the seller's real, known identifiers - the
/// genuine, deep, on-demand verification step, only ever run for a
/// single link the user has deliberately chosen to check. Mirrors
/// check_b2b_page's real, existing fetch pattern - no separate proxy
/// service exists in this codebase.
pub async fn verify_social_link(
    url: &str,
    seller: &SellerIdentifiers,
) -> Result<OsintMatch, String> {
    let client = Client::new();
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Could not reach the real, live page: {}", e))?;
    if !response.status().is_success() {
        return Err(format!(
            "The real, live page returned an unsuccessful status: {}",
            response.status()
        ));
    }
    let page_text = response
        .text()
        .await
        .map_err(|e| format!("Could not read the real, live page content: {}", e))?;

    Ok(score_identifier_match(seller, &page_text))
}
