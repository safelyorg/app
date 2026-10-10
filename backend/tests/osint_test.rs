use backend::services::osint::{
    PlatformCheckResult, SCAM_MENTIONS_FOUND, SocialCandidateLink, build_location_fallback_queries,
    build_osint_query_matrix, social_presence_signal,
};
use std::collections::HashSet;

// Build Osint Query Matrix Tests
#[test]
fn build_osint_query_matrix_returns_empty_when_theres_genuinely_no_company_name() {
    let queries = build_osint_query_matrix(None, None, None);
    assert!(queries.is_empty());

    let queries = build_osint_query_matrix(Some("   "), None, None);
    assert!(
        queries.is_empty(),
        "expected whitespace-only company name to be treated as genuinely missing"
    );
}

#[test]
fn build_osint_query_matrix_never_embeds_location_in_the_query_text_anymore() {
    // Tier 1 is deliberately name-only now - location moved to the
    // fallback tier (build_location_fallback_queries).
    let queries = build_osint_query_matrix(Some("Deva Inc"), None, None);
    assert!(!queries.is_empty());
    for (_, query_text, _) in &queries {
        assert!(
            !query_text.to_lowercase().contains("tustin"),
            "expected no location text to leak into a Tier 1 query, got: {}",
            query_text
        );
    }
}

#[test]
fn build_osint_query_matrix_dedupes_a_two_word_company_name_into_one_real_variant() {
    // "Reviews" is pushed exactly once per variant (unlike the
    // per-platform entries, which are multiplied by both the 6
    // platforms and the 5 scam-word groups) - so it's a clean signal
    // for how many variants dedup actually produced.
    let queries = build_osint_query_matrix(Some("Deva Inc"), None, None);
    let review_queries: Vec<_> = queries
        .iter()
        .filter(|(platform, _, _)| platform == "Reviews")
        .collect();
    assert_eq!(
        review_queries.len(),
        1,
        "expected exactly one Reviews query for a genuinely two-word name, not a duplicate"
    );
}

#[test]
fn build_osint_query_matrix_produces_two_real_variants_for_a_three_word_company_name() {
    let queries = build_osint_query_matrix(Some("Deva Industrial Supply"), None, None);
    let review_variants: Vec<_> = queries
        .iter()
        .filter(|(platform, _, _)| platform == "Reviews")
        .map(|(_, _, variant)| variant.clone())
        .collect();
    assert_eq!(
        review_variants.len(),
        2,
        "expected both the first-two-words variant and the full-name variant"
    );
    assert!(review_variants.contains(&"Deva Industrial".to_string()));
    assert!(review_variants.contains(&"Deva Industrial Supply".to_string()));
}

#[test]
fn build_osint_query_matrix_fans_out_every_platform_query_across_all_scam_word_groups() {
    // 1 base query + 5 scam-word-group queries per (variant, platform).
    let queries = build_osint_query_matrix(Some("Deva Inc"), None, None);
    let facebook_queries: Vec<_> = queries
        .iter()
        .filter(|(platform, _, _)| platform == "Facebook")
        .collect();
    assert_eq!(
        facebook_queries.len(),
        6,
        "expected 1 base query + 5 scam-word-group queries for a single variant on one platform"
    );
}

#[test]
fn build_osint_query_matrix_total_count_matches_the_real_formula_for_one_variant() {
    // 6 platforms * (1 base + 5 scam-word groups) + 1 Reviews query.
    let queries = build_osint_query_matrix(Some("Deva Inc"), None, None);
    assert_eq!(queries.len(), 37);
}

#[test]
fn build_osint_query_matrix_includes_the_real_site_filter_for_every_platform() {
    let queries = build_osint_query_matrix(Some("Acme Co"), None, None);
    let facebook = queries
        .iter()
        .find(|(platform, _, _)| platform == "Facebook")
        .expect("expected a genuine Facebook entry");
    assert!(facebook.1.contains("site:facebook.com"));
    assert!(facebook.1.contains("\"Acme Co\""));
}

// Build Location Fallback Queries Tests
#[test]
fn build_location_fallback_queries_returns_empty_when_location_is_genuinely_missing() {
    let needed = HashSet::from([("Facebook".to_string(), "Acme Co".to_string())]);
    let queries = build_location_fallback_queries("Acme Co", "", &needed);
    assert!(queries.is_empty());

    let queries = build_location_fallback_queries("Acme Co", "   ", &needed);
    assert!(
        queries.is_empty(),
        "expected whitespace-only location to be treated as genuinely missing"
    );
}

#[test]
fn build_location_fallback_queries_only_builds_for_the_pairs_genuinely_marked_as_needed() {
    let mut needed = HashSet::new();
    needed.insert(("Facebook".to_string(), "Acme Co".to_string()));

    let queries = build_location_fallback_queries("Acme Co", "Tustin, CA", &needed);

    assert_eq!(
        queries.len(),
        1,
        "expected exactly one fallback query, for only the one platform+variant marked as needed"
    );
    assert_eq!(queries[0].0, "Facebook");
}

#[test]
fn build_location_fallback_queries_appends_the_real_location_to_the_query_text() {
    let mut needed = HashSet::new();
    needed.insert(("LinkedIn".to_string(), "Acme Co".to_string()));

    let queries = build_location_fallback_queries("Acme Co", "Tustin, CA/Orange County", &needed);

    assert_eq!(queries.len(), 1);
    let (_, query_text, _) = &queries[0];
    assert!(query_text.contains("\"Acme Co\""));
    assert!(
        query_text.contains("\"Tustin, CA\""),
        "expected only the part before the first '/' to be used, got: {}",
        query_text
    );
    assert!(
        !query_text.contains("Orange County"),
        "expected everything after the first '/' to be genuinely dropped, got: {}",
        query_text
    );
}

#[test]
fn build_location_fallback_queries_ignores_pairs_that_werent_asked_for() {
    // Confirms this is a real, honest filter - not silently returning
    // every platform+variant combination regardless of `needed`.
    let needed = HashSet::from([("Reddit".to_string(), "Acme Co".to_string())]);
    let queries = build_location_fallback_queries("Acme Co", "Tustin, CA", &needed);

    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0].0, "Reddit");
    assert!(
        !queries
            .iter()
            .any(|(platform, _, _)| platform == "Facebook"),
        "expected Facebook to be genuinely skipped, since it wasn't in `needed`"
    );
}

// Social Presence Signal Tests
fn result(platform: &str, found: bool, scam_mention: bool) -> PlatformCheckResult {
    PlatformCheckResult {
        platform: platform.to_string(),
        variant_searched: "Acme Co".to_string(),
        found,
        candidates: if found {
            vec![SocialCandidateLink {
                platform: platform.to_string(),
                title: "Acme Co".to_string(),
                url: "https://example.com".to_string(),
            }]
        } else {
            Vec::new()
        },
        scam_mention,
    }
}

#[test]
fn scam_mentions_are_a_warning_not_proof_the_company_exists() {
    // Before: any hit, including "Acme Co golpe", counted as "found
    // online" and showed as information only.
    let signal = social_presence_signal(&[
        result("Facebook", true, false),
        result("Reddit", true, true),
    ]);
    assert_eq!(signal.value, SCAM_MENTIONS_FOUND);
    assert_eq!(signal.signal_type, "caution");
    assert!(signal.sub.contains("1 search(es)"));
}

#[test]
fn being_found_online_without_scam_words_is_information_only() {
    let signal = social_presence_signal(&[
        result("Facebook", true, false),
        result("LinkedIn", false, false),
        result("Reddit", false, true),
    ]);
    assert_eq!(signal.value, "Candidates found");
    assert_eq!(signal.signal_type, "info");
    assert_eq!(
        signal.sub,
        "1 of 2 platform checks found a real, candidate result."
    );
}

#[test]
fn no_trace_online_is_a_warning() {
    let signal = social_presence_signal(&[
        result("Facebook", false, false),
        result("Reddit", false, true),
    ]);
    assert_eq!(signal.value, "No presence found");
    assert_eq!(signal.signal_type, "caution");
}
