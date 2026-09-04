use backend::services::platform_config::get_all_platform_domains;

#[test]
fn returns_the_real_known_platforms_with_correct_domains() {
    let domains = get_all_platform_domains();
    assert_eq!(domains.get("olx"), Some(&"olx.com.pk".to_string()));
    assert_eq!(domains.get("b2brazil"), Some(&"b2brazil.com".to_string()));
}

#[test]
fn returns_exactly_the_platforms_currently_configured_no_more_no_less() {
    let domains = get_all_platform_domains();
    assert_eq!(
        domains.len(),
        2,
        "expected exactly the platforms currently in platform_domains.json - \
         update this test if a new platform is genuinely added"
    );
}

#[test]
fn does_not_contain_a_genuinely_unconfigured_platform() {
    let domains = get_all_platform_domains();
    assert_eq!(domains.get("alibaba"), None);
    assert_eq!(domains.get("some_random_platform"), None);
}

#[test]
fn repeated_calls_return_consistent_data() {
    let first_call = get_all_platform_domains();
    let second_call = get_all_platform_domains();
    assert_eq!(first_call, second_call);
}

#[test]
fn returns_a_genuine_clone_not_a_shared_mutable_reference() {
    let mut first_call = get_all_platform_domains();
    first_call.insert("fake_platform".to_string(), "fake.com".to_string());

    let second_call = get_all_platform_domains();
    assert_eq!(
        second_call.get("fake_platform"),
        None,
        "expected the static, real data to remain genuinely unaffected by mutating a returned clone"
    );
    assert_eq!(second_call.len(), 2);
}

#[test]
fn domain_values_contain_no_protocol_or_trailing_slash() {
    // A real, honest sanity check - these values get compared
    // directly against extracted hostnames elsewhere, so a stray
    // "https://" or trailing "/" would silently break every match.
    let domains = get_all_platform_domains();
    for (platform, domain) in domains.iter() {
        assert!(
            !domain.starts_with("http"),
            "expected {}'s domain to be a bare hostname, not a full URL: {}",
            platform,
            domain
        );
        assert!(
            !domain.ends_with('/'),
            "expected {}'s domain to have no trailing slash: {}",
            platform,
            domain
        );
    }
}
