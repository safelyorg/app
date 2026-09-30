mod common;

use backend::services::b2b_scrapers::{
    B2bScraper, B2bSupplierProfile, alibaba::AlibabaScraper, b2brazil::B2brazilScraper,
    check_b2b_page, exporthub::ExporthubScraper, fetch_b2b_page, looks_like_a_real_page,
    tradewheel::TradewheelScraper,
};
use serial_test::serial;
use std::env::remove_var;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

// NOTE: This test hits B2Brazil's real, live site through ScraperAPI -
// occasional 500 errors are real, external flakiness (ScraperAPI's own
// retry cycle failing), not a code bug. If this fails, just re-run it
// alone a few minutes later before assuming something's broken.
// #[tokio::test]
// #[serial]
// async fn check_b2b_page_fetches_and_parses_a_real_live_b2brazil_page() {
//     let real_url = "https://b2brazil.com/hotsite/akuratconsultor";

//     let result = check_b2b_page("b2brazil", real_url).await;

//     assert!(
//         result.is_some(),
//         "expected a real, successful fetch and parse against B2Brazil's live site"
//     );

//     let (supplier, listing) = result.expect("expected to get the suplier and listing");

//     assert_eq!(
//         supplier.company_name,
//         Some("Akurat Consultoria Empresarial".to_string())
//     );
//     assert_eq!(supplier.source_platform, "b2brazil");
//     assert_eq!(listing.source_platform, "b2brazil");
// }

#[tokio::test]
#[serial]
async fn check_b2b_page_returns_none_for_a_genuinely_unrecognized_platform() {
    let result = check_b2b_page("some_platform_that_does_not_exist", "https://example.com").await;
    assert!(result.is_none());
}

#[tokio::test]
#[serial]
async fn check_b2b_page_returns_none_for_a_genuinely_broken_url() {
    let result = check_b2b_page(
        "b2brazil",
        "https://this-domain-genuinely-does-not-exist-xyz123.com",
    )
    .await;
    assert!(result.is_none());
}

// Build B2B Analysis Path Tests
// #[tokio::test]
// #[serial]
// async fn build_b2b_analysis_path_produces_a_real_complete_result_from_the_live_site() {
//     let pool = test_pool().await;
//     let request = AnalyzeRequest {
//         platform: "b2brazil".to_string(),
//         seller_id: None,
//         listing_url: "https://b2brazil.com/hotsite/akuratconsultor".to_string(),
//         listing_id: None,
//         title: None,
//         price: None,
//         description: None,
//         category: None,
//         image_urls: None,
//         posted_date: None,
//         platform_id: None,
//         seller_name: None,
//         seller_handle: None,
//         seller_phone: None,
//         seller_profile_url: None,
//         seller_join_date: None,
//         seller_location: None,
//         seller_last_active: None,
//         seller_website: None,
//         seller_verified: None,
//         seller_rating: None,
//         seller_total_products: None,
//         domain_check_status: None,
//         domain_check_real_name: None,
//         domain_check_real_domain: None,
//         domain_check_current_domain: None,
//         domain_check_current_html: None,
//         domain_check_real_html: None,
//         language: None,
//     };

//     let result = build_b2b_analysis_path(&pool, &request, 0, Uuid::new_v4()).await;

//     assert!(
//         result.is_ok(),
//         "expected the full, real B2B analysis path to succeed against the live site"
//     );

//     let (signals, risk_score, notes, supplier, _listing, _social_candidates) = result.unwrap();

//     assert_eq!(signals.len(), 14);
//     assert!(risk_score >= 0 && risk_score <= 100);
//     assert!(!notes.is_empty());
//     assert_eq!(
//         supplier.company_name,
//         Some("Akurat Consultoria Empresarial".to_string())
//     );
// }

#[tokio::test]
#[serial]
async fn fetch_b2b_page_fails_gracefully_for_a_genuinely_broken_url() {
    let result = fetch_b2b_page(
        "b2brazil",
        "https://this-domain-genuinely-does-not-exist-xyz789.com",
    )
    .await;
    assert!(
        result.is_none(),
        "expected the fetch to fail gracefully for a genuinely broken URL, but it succeeded"
    );
}

// #[tokio::test]
// #[serial]
// async fn build_b2b_analysis_path_returns_real_social_candidates() {
//     let pool = test_pool().await;
//     let request = AnalyzeRequest {
//         platform: "b2brazil".to_string(),
//         seller_id: None,
//         listing_url: "https://b2brazil.com/hotsite/akuratconsultor".to_string(),
//         listing_id: None,
//         title: None,
//         price: None,
//         description: None,
//         category: None,
//         image_urls: None,
//         posted_date: None,
//         platform_id: None,
//         seller_name: None,
//         seller_handle: None,
//         seller_phone: None,
//         seller_profile_url: None,
//         seller_join_date: None,
//         seller_location: None,
//         seller_last_active: None,
//         seller_website: None,
//         seller_verified: None,
//         seller_rating: None,
//         seller_total_products: None,
//         domain_check_status: None,
//         domain_check_real_name: None,
//         domain_check_real_domain: None,
//         domain_check_current_domain: None,
//         domain_check_current_html: None,
//         domain_check_real_html: None,
//         language: None,
//     };

//     let result = build_b2b_analysis_path(&pool, &request, 0, Uuid::new_v4())
//         .await
//         .expect("expected the full, real B2B analysis path to succeed");

//     let (_signals, _risk_score, _notes, _supplier, _listing, social_candidates) = result;

//     assert!(
//         !social_candidates.is_empty(),
//         "expected the real, live OSINT matrix to return at least some platform check results"
//     );
//     assert!(
//         social_candidates.iter().any(|r| r.platform == "Facebook"),
//         "expected Facebook to genuinely be among the checked platforms"
//     );
// }

#[test]
#[serial]
fn alibaba_parse_supplier_reads_a_badged_listing_correctly() {
    let html = r#"
        <html><body>
        <div data-testid="three-column-mini-company-card">
            <a class="id-underline">Guangzhou Reliable Supplies Ltd.</a>
            <div data-testid="three-column-mini-company-card-verify-icon">
                <img src="https://img.alibaba.com/verify.png" />
            </div>
            <div class="id-mt-1">
                <span>Shenzhen, CN</span>
                <span>8 yrs</span>
                <span>Trusted service provider</span>
            </div>
        </div>
        </body></html>
    "#;

    let supplier = AlibabaScraper.parse_supplier(html, "https://alibaba.com/product-detail/y.html");

    assert_eq!(supplier.country, Some("Shenzhen, CN".to_string()));
    assert_eq!(
        supplier.badge_honorific,
        Some("Trusted service provider".to_string())
    );
    assert!(
        supplier.platform_verified_badge,
        "expected a real verify badge when the verify-icon element is genuinely present"
    );

    assert_eq!(supplier.year_established, None);
}

#[test]
#[serial]
fn alibaba_parse_supplier_reads_an_unbadged_listing_correctly() {
    // Real Alibaba pages give the company logo alt="<Company name> logo";
    // that alt text is how the scraper tells the logo apart from the
    // verified-badge image.
    let html = r#"
        <html><body>
        <div data-testid="three-column-mini-company-card">
            <a class="id-underline">Shenzhen Trustco Electronics Co., Ltd.</a>
            <img src="https://img.alibaba.com/logo.png" alt="Shenzhen Trustco Electronics Co., Ltd. logo" />
            <div class="id-mt-1">
                <span>CN</span>
                <span>10 yrs</span>
            </div>
        </div>
        </body></html>
    "#;

    let supplier = AlibabaScraper.parse_supplier(html, "https://alibaba.com/product-detail/x.html");

    assert_eq!(
        supplier.company_name,
        Some("Shenzhen Trustco Electronics Co., Ltd.".to_string())
    );
    assert_eq!(supplier.country, Some("CN".to_string()));
    assert_eq!(supplier.badge_honorific, None);
    assert!(
        !supplier.platform_verified_badge,
        "expected no verify badge when the verify-icon element is genuinely absent"
    );
    assert_eq!(
        supplier.logo_url,
        Some("https://img.alibaba.com/logo.png".to_string())
    );
    assert_eq!(supplier.source_platform, "alibaba");

    // "10 yrs" is years on Alibaba, not company age - no founding year.
    assert_eq!(supplier.year_established, None);
}

#[test]
#[serial]
fn alibaba_parse_supplier_prefers_the_real_overview_year_over_the_years_estimate() {
    let html = r#"
        <html><body>
        <div data-testid="three-column-mini-company-card">
            <a class="id-underline">Estimate Overridden Co.</a>
            <div class="id-mt-1">
                <span>CN</span>
                <span>10 yrs</span>
            </div>
        </div>
        <button class="id-cursor-default">
            <div>Year founded</div>
            <div title="2015"></div>
        </button>
        </body></html>
    "#;

    let supplier = AlibabaScraper.parse_supplier(html, "https://alibaba.com/product-detail/z.html");

    assert_eq!(
        supplier.year_established,
        Some("2015".to_string()),
        "expected the real, stated overview year to win over the years-span estimate"
    );
}

#[test]
#[serial]
fn alibaba_extract_overview_field_reads_sales_revenue_from_the_overview_panel() {
    let html = r#"
        <html><body>
        <div data-testid="three-column-mini-company-card">
            <a class="id-underline">Revenue Co.</a>
        </div>
        <button class="id-cursor-default">
            <div>Online revenue</div>
            <div title="Above US$100 million"></div>
        </button>
        </body></html>
    "#;

    let supplier =
        AlibabaScraper.parse_supplier(html, "https://alibaba.com/product-detail/rev.html");

    assert_eq!(
        supplier.sales_revenue,
        Some("Above US$100 million".to_string())
    );
}

#[test]
#[serial]
fn alibaba_parse_supplier_handles_a_genuinely_empty_page_without_panicking() {
    let html = "<html><body><p>Not an Alibaba listing at all</p></body></html>";
    let supplier =
        AlibabaScraper.parse_supplier(html, "https://alibaba.com/product-detail/none.html");

    assert_eq!(supplier.company_name, None);
    assert_eq!(supplier.country, None);
    assert_eq!(supplier.badge_honorific, None);
    assert!(!supplier.platform_verified_badge);
    assert_eq!(supplier.year_established, None);
}

#[test]
#[serial]
fn alibaba_parse_listing_reads_ladder_pricing_as_joined_tiers() {
    let html = r#"
        <html><body>
        <h1 title="Custom Cotton T-Shirts Wholesale">Custom Cotton T-Shirts Wholesale</h1>
        <div data-testid="ladder-price">
            <span class="price-item">100-499 pieces $2.50</span>
            <span class="price-item">500+ pieces $2.10</span>
        </div>
        </body></html>
    "#;

    let listing =
        AlibabaScraper.parse_listing(html, "https://alibaba.com/product-detail/shirts.html");

    assert_eq!(
        listing.title,
        Some("Custom Cotton T-Shirts Wholesale".to_string())
    );
    assert_eq!(
        listing.unit_price,
        Some("100-499 pieces $2.50 | 500+ pieces $2.10".to_string())
    );
    assert_eq!(
        listing.minimum_order_quantity, None,
        "expected no separate MOQ field when ladder pricing already encodes quantities per tier"
    );
}

#[test]
#[serial]
fn alibaba_parse_listing_falls_back_to_range_pricing_and_splits_out_the_moq() {
    let html = r#"
        <html><body>
        <h1 title="Bulk Plastic Bottles">Bulk Plastic Bottles</h1>
        <div data-testid="range-price">US$0.05-0.30 Minimum order quantity: 100 pieces</div>
        </body></html>
    "#;

    let listing =
        AlibabaScraper.parse_listing(html, "https://alibaba.com/product-detail/bottles.html");

    assert_eq!(listing.unit_price, Some("US$0.05-0.30".to_string()));
    assert_eq!(
        listing.minimum_order_quantity,
        Some("100 pieces".to_string())
    );
}

#[test]
#[serial]
fn alibaba_parse_listing_builds_description_from_key_attributes() {
    let html = r#"
        <html><body>
        <h1 title="Cotton Hoodie">Cotton Hoodie</h1>
        <div data-testid="three-column-key-attributes-row">
            <p>Material</p><p>Cotton</p>
            <p>Gender</p><p>Unisex</p>
        </div>
        </body></html>
    "#;

    let listing =
        AlibabaScraper.parse_listing(html, "https://alibaba.com/product-detail/hoodie.html");

    assert_eq!(
        listing.description,
        Some("Material: Cotton. Gender: Unisex".to_string())
    );
}

#[test]
#[serial]
fn alibaba_parse_listing_extracts_real_image_urls_and_skips_play_icons() {
    let html = r#"
        <html><body>
        <div data-testid="main-image-thumbnail" style='background-image:url("//img.alibaba.com/photo1.jpg")'></div>
        <div data-testid="main-image-thumbnail" style='background-image:url("https://img.alibaba.com/photo2.jpg")'></div>
        <div data-testid="main-image-thumbnail" style='background-image:url("https://img.alibaba.com/icon-play.png")'></div>
        </body></html>
    "#;

    let listing =
        AlibabaScraper.parse_listing(html, "https://alibaba.com/product-detail/photos.html");

    assert_eq!(
        listing.image_urls,
        vec![
            "https://img.alibaba.com/photo1.jpg".to_string(),
            "https://img.alibaba.com/photo2.jpg".to_string(),
        ],
        "expected the protocol-relative URL normalized to https, and the play-icon thumbnail excluded entirely"
    );
}

#[test]
#[serial]
fn alibaba_extracts_the_real_company_profile_link_from_a_listing_page() {
    let html = r#"
        <html><body>
        <a href="https://alibaba.com/company_profile.html">Company profile</a>
        </body></html>
    "#;

    let url = AlibabaScraper.extract_company_profile_url(html);
    assert_eq!(
        url,
        Some("https://alibaba.com/company_profile.html".to_string())
    );
}

#[test]
#[serial]
fn alibaba_extract_company_profile_url_returns_none_when_the_link_is_genuinely_absent() {
    let html = "<html><body><a href=\"/other-link.html\">Contact supplier</a></body></html>";
    assert_eq!(AlibabaScraper.extract_company_profile_url(html), None);
}

#[test]
#[serial]
fn alibaba_enrich_from_company_profile_reads_the_older_vd_item_template() {
    let html = r#"
        <html><body>
        <a class="vd-item">Total Employees:<span class="con-text">51 - 100 People</span></a>
        </body></html>
    "#;

    let supplier = B2bSupplierProfile::default();
    let enriched = AlibabaScraper.enrich_from_company_profile(supplier, html);

    assert_eq!(enriched.employee_count, Some("51 - 100 People".to_string()));
}

#[test]
#[serial]
fn alibaba_enrich_from_company_profile_reads_the_newer_span_pair_template() {
    let html = r#"
        <html><body>
        <div class="items-start justify-between">
            <span>Total employees</span>
            <span>201-500</span>
        </div>
        </body></html>
    "#;

    let supplier = B2bSupplierProfile::default();
    let enriched = AlibabaScraper.enrich_from_company_profile(supplier, html);

    assert_eq!(enriched.employee_count, Some("201-500".to_string()));
}

#[test]
#[serial]
fn alibaba_enrich_from_company_profile_prefers_the_older_template_when_both_are_present() {
    let html = r#"
        <html><body>
        <a class="vd-item">Total Employees:<span class="con-text">51 - 100 People</span></a>
        <div class="items-start justify-between">
            <span>Total employees</span>
            <span>201-500</span>
        </div>
        </body></html>
    "#;

    let supplier = B2bSupplierProfile::default();
    let enriched = AlibabaScraper.enrich_from_company_profile(supplier, html);

    assert_eq!(
        enriched.employee_count,
        Some("51 - 100 People".to_string()),
        "expected the older template's value to win when both are genuinely present"
    );
}

#[test]
#[serial]
fn alibaba_enrich_from_company_profile_leaves_employee_count_none_when_genuinely_absent() {
    let html = "<html><body><p>No employee data on this page</p></body></html>";
    let supplier = B2bSupplierProfile::default();
    let enriched = AlibabaScraper.enrich_from_company_profile(supplier, html);
    assert_eq!(enriched.employee_count, None);
}

// --- Alibaba: company_description regression guard ---
// Alibaba doesn't scrape a company description yet - this pins that
// down so a future change doesn't silently start filling it in with
// something wrong, unnoticed.

#[test]
#[serial]
fn alibaba_parse_supplier_never_sets_a_company_description() {
    let html = r#"
        <html><body>
        <div data-testid="three-column-mini-company-card">
            <a class="id-underline">Some Alibaba Supplier</a>
        </div>
        </body></html>
    "#;
    let supplier = AlibabaScraper.parse_supplier(html, "https://alibaba.com/product-detail/x.html");
    assert_eq!(supplier.company_description, None);
}

// --- B2Brazil: extract_company_profile_url ---

#[test]
#[serial]
fn b2brazil_extract_company_profile_url_keeps_an_absolute_href_as_is() {
    let html = r#"<html><body><a class="nav-home" href="https://b2brazil.com/hotsite/acme">Home</a></body></html>"#;
    let url = B2brazilScraper.extract_company_profile_url(html);
    assert_eq!(url, Some("https://b2brazil.com/hotsite/acme".to_string()));
}

#[test]
#[serial]
fn b2brazil_extract_company_profile_url_prefixes_a_relative_href_with_the_real_domain() {
    let html = r#"<html><body><a class="nav-home" href="/hotsite/acme">Home</a></body></html>"#;
    let url = B2brazilScraper.extract_company_profile_url(html);
    assert_eq!(url, Some("https://b2brazil.com/hotsite/acme".to_string()));
}

#[test]
#[serial]
fn b2brazil_extract_company_profile_url_returns_none_when_the_link_is_genuinely_absent() {
    let html = "<html><body><a href=\"/something-else\">Not it</a></body></html>";
    assert_eq!(B2brazilScraper.extract_company_profile_url(html), None);
}

// --- B2Brazil: enrich_from_company_profile ---

#[test]
#[serial]
fn b2brazil_enrich_from_company_profile_reads_the_real_about_text() {
    let html = r#"
        <html><body>
        <div class="section-content-about">
            <div>Founded in 1998, specializing in industrial equipment for the mining sector.</div>
        </div>
        </body></html>
    "#;
    let supplier = B2bSupplierProfile::default();
    let enriched = B2brazilScraper.enrich_from_company_profile(supplier, html);
    assert_eq!(
        enriched.company_description,
        Some(
            "Founded in 1998, specializing in industrial equipment for the mining sector."
                .to_string()
        )
    );
}

#[test]
#[serial]
fn b2brazil_enrich_from_company_profile_leaves_description_none_when_the_about_block_is_genuinely_empty()
 {
    let html =
        r#"<html><body><div class="section-content-about"><div>   </div></div></body></html>"#;
    let supplier = B2bSupplierProfile::default();
    let enriched = B2brazilScraper.enrich_from_company_profile(supplier, html);
    assert_eq!(
        enriched.company_description, None,
        "expected whitespace-only about text to be treated as genuinely no description"
    );
}

#[test]
#[serial]
fn b2brazil_enrich_from_company_profile_leaves_description_none_when_the_section_is_genuinely_absent()
 {
    let html = "<html><body><p>No about section on this page</p></body></html>";
    let supplier = B2bSupplierProfile::default();
    let enriched = B2brazilScraper.enrich_from_company_profile(supplier, html);
    assert_eq!(enriched.company_description, None);
}

#[test]
#[serial]
fn b2brazil_enrich_from_company_profile_never_touches_other_fields() {
    // Guard against a careless future edit widening this function's
    // scope - it should only ever write company_description.
    let mut supplier = B2bSupplierProfile::default();
    supplier.company_name = Some("Existing Name Should Survive".to_string());
    let html = r#"<html><body><div class="section-content-about"><div>New description.</div></div></body></html>"#;
    let enriched = B2brazilScraper.enrich_from_company_profile(supplier, html);
    assert_eq!(
        enriched.company_name,
        Some("Existing Name Should Survive".to_string())
    );
}

// check_b2b_page's looks_like_a_real_page() guard rejects anything
// under 2000 bytes as probable ScraperAPI garbage - real fixtures
// need to genuinely clear that bar, not just carry the real markup
// being tested. This pads every fixture with an inert, hidden filler
// block so the byte-size gate passes without affecting parsing.
fn pad_html(inner: &str) -> String {
    format!(
        "<!doctype html><html><body>{}<div style=\"display:none\">{}</div></body></html>",
        inner,
        "x".repeat(2200)
    )
}

fn b2brazil_listing_no_profile_link() -> String {
    pad_html(
        r#"<div id="header-info-company"><h1>Some Company</h1></div><h2 class="section-product-title">Some Listing</h2>"#,
    )
}

fn listing_html_with_profile_link(profile_url: &str) -> String {
    pad_html(&format!(
        r#"<div id="header-info-company"><h1>Some Company</h1></div><h2 class="section-product-title">Some Listing</h2><a class="nav-home" href="{}">Home</a>"#,
        profile_url
    ))
}

fn b2brazil_profile_with_description() -> String {
    pad_html(
        r#"<div class="section-content-about">
        <div>Founded in 1998, specializing in industrial equipment.</div>
    </div>"#,
    )
}

#[tokio::test]
#[serial]
async fn check_b2b_page_does_not_fetch_a_profile_page_when_no_link_is_found() {
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/listing"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(b2brazil_listing_no_profile_link()),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let listing_url = format!("{}/listing", mock_server.uri());
    let result = check_b2b_page("b2brazil", &listing_url).await;

    assert!(result.is_some());
    let (supplier, _listing) = result.unwrap();
    assert_eq!(
        supplier.company_description, None,
        "expected no enrichment fetch, so no description, when the profile link is genuinely absent"
    );
    // The `.expect(1)` above on the mock is itself the real assertion
    // that exactly one request was made - mock_server.verify() below
    // makes that explicit and fails loudly if it's ever violated.
    mock_server.verify().await;
}

#[tokio::test]
#[serial]
async fn check_b2b_page_fetches_the_profile_page_and_applies_real_enrichment() {
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    let mock_server = MockServer::start().await;
    let profile_url = format!("{}/hotsite/acme", mock_server.uri());

    Mock::given(method("GET"))
        .and(path("/listing"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(listing_html_with_profile_link(&profile_url)),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/hotsite/acme"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(b2brazil_profile_with_description()),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let listing_url = format!("{}/listing", mock_server.uri());
    let result = check_b2b_page("b2brazil", &listing_url).await;

    assert!(result.is_some());
    let (supplier, _listing) = result.unwrap();
    assert_eq!(
        supplier.company_description,
        Some("Founded in 1998, specializing in industrial equipment.".to_string()),
        "expected the real enrichment fetch to genuinely fill in the company description"
    );
    mock_server.verify().await;
}

#[tokio::test]
#[serial]
async fn check_b2b_page_still_succeeds_when_the_enrichment_fetch_genuinely_fails() {
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    let mock_server = MockServer::start().await;
    let profile_url = format!("{}/hotsite/broken", mock_server.uri());

    Mock::given(method("GET"))
        .and(path("/listing"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(listing_html_with_profile_link(&profile_url)),
        )
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/hotsite/broken"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let listing_url = format!("{}/listing", mock_server.uri());
    let result = check_b2b_page("b2brazil", &listing_url).await;

    assert!(
        result.is_some(),
        "a failed ENRICHMENT fetch must never fail the whole analysis - only the primary fetch is load-bearing"
    );
    let (supplier, _listing) = result.unwrap();
    assert_eq!(
        supplier.company_description, None,
        "expected the supplier to come back exactly as parsed from the listing page, un-enriched"
    );
}

#[tokio::test]
#[serial]
async fn check_b2b_page_returns_none_when_the_primary_fetch_fails() {
    unsafe {
        remove_var("SCRAPERAPI_KEY");
    }
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/listing"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let listing_url = format!("{}/listing", mock_server.uri());
    let result = check_b2b_page("b2brazil", &listing_url).await;

    assert!(result.is_none());
}

#[tokio::test]
#[serial]
async fn check_b2b_page_genuinely_never_retries_a_failed_primary_fetch() {
    // This is the direct regression guard for the retry loop's
    // removal: a 500 on the primary fetch must be requested EXACTLY
    // once, not up to 3 times like the old behavior.
    unsafe {
        remove_var("SCRAPERAPI_KEY");
    }
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/listing"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&mock_server)
        .await;

    let listing_url = format!("{}/listing", mock_server.uri());
    let _ = check_b2b_page("b2brazil", &listing_url).await;

    mock_server.verify().await;
}

// --- TradeWheel scraper: matches_platform ---

#[test]
#[serial]
fn tradewheel_matches_platform_correctly_identifies_tradewheel_only() {
    let scraper = TradewheelScraper;
    assert!(scraper.matches_platform("tradewheel"));
    assert!(!scraper.matches_platform("alibaba"));
    assert!(!scraper.matches_platform("b2brazil"));
}

// --- TradeWheel scraper: parse_supplier ---

#[test]
#[serial]
fn tradewheel_parse_supplier_reads_the_real_company_name_and_country() {
    let html = r#"
        <html><body>
        <div class="comp-info">
            <h2>Global Textiles Trading Co.</h2>
            <div class="bo-flag"><i class="flag-icon"></i> United Kingdom</div>
        </div>
        </body></html>
    "#;

    let supplier = TradewheelScraper.parse_supplier(html, "https://tradewheel.com/p/company/x");

    assert_eq!(
        supplier.company_name,
        Some("Global Textiles Trading Co.".to_string())
    );
    assert_eq!(supplier.country, Some("United Kingdom".to_string()));
    assert_eq!(supplier.source_platform, "tradewheel");
    assert_eq!(supplier.profile_url, "https://tradewheel.com/p/company/x");
}

#[test]
#[serial]
fn tradewheel_parse_supplier_detects_a_genuine_gold_badge() {
    let html = r#"
        <html><body>
        <div class="comp-info">
            <h2>Gold Member Supplier</h2>
            <img src="https://cdn.tradewheel.com/badges/gold-txt1.png" />
        </div>
        </body></html>
    "#;

    let supplier = TradewheelScraper.parse_supplier(html, "https://tradewheel.com/p/company/gold");

    assert_eq!(supplier.badge_honorific, Some("Gold".to_string()));
    assert!(
        supplier.platform_verified_badge,
        "expected a real gold badge to also count as platform_verified_badge"
    );
}

#[test]
#[serial]
fn tradewheel_parse_supplier_leaves_badge_none_when_the_image_is_not_a_gold_badge() {
    let html = r#"
        <html><body>
        <div class="comp-info">
            <h2>Regular Supplier</h2>
            <img src="https://cdn.tradewheel.com/badges/silver-txt1.png" />
        </div>
        </body></html>
    "#;

    let supplier =
        TradewheelScraper.parse_supplier(html, "https://tradewheel.com/p/company/silver");

    assert_eq!(supplier.badge_honorific, None);
    assert!(
        !supplier.platform_verified_badge,
        "expected no verified badge when the badge image genuinely isn't the gold one"
    );
}

#[test]
#[serial]
fn tradewheel_parse_supplier_badge_detection_is_case_insensitive() {
    let html = r#"
        <html><body>
        <div class="comp-info">
            <h2>Cased Differently Co.</h2>
            <img src="https://cdn.tradewheel.com/badges/GOLD-TXT1.PNG" />
        </div>
        </body></html>
    "#;

    let supplier = TradewheelScraper.parse_supplier(html, "https://tradewheel.com/p/company/cased");
    assert_eq!(supplier.badge_honorific, Some("Gold".to_string()));
}

#[test]
#[serial]
fn tradewheel_parse_supplier_handles_a_genuinely_empty_page_without_panicking() {
    let html = "<html><body><p>Not a TradeWheel listing at all</p></body></html>";
    let supplier = TradewheelScraper.parse_supplier(html, "https://tradewheel.com/p/company/none");

    assert_eq!(supplier.company_name, None);
    assert_eq!(supplier.country, None);
    assert_eq!(supplier.badge_honorific, None);
    assert!(!supplier.platform_verified_badge);
}

// --- TradeWheel scraper: parse_listing ---

#[test]
#[serial]
fn tradewheel_parse_listing_reads_the_real_title() {
    let html = r#"<html><body><h1 class="pd-heading">Bulk Cotton Yarn</h1></body></html>"#;
    let listing = TradewheelScraper.parse_listing(html, "https://tradewheel.com/p/x");
    assert_eq!(listing.title, Some("Bulk Cotton Yarn".to_string()));
}

#[test]
#[serial]
fn tradewheel_parse_listing_joins_multiple_description_paragraphs() {
    let html = r#"
        <html><body>
        <div class="product-details-container">
            <p>High quality cotton yarn.</p>
            <p>Available in bulk quantities.</p>
        </div>
        </body></html>
    "#;

    let listing = TradewheelScraper.parse_listing(html, "https://tradewheel.com/p/x");

    assert_eq!(
        listing.description,
        Some("High quality cotton yarn. Available in bulk quantities.".to_string())
    );
}

#[test]
#[serial]
fn tradewheel_parse_listing_prefers_po_box_table_value_when_the_same_label_is_in_multiple_tables() {
    let html = r#"
        <html><body>
        <div class="po-box"><table><tr><td>Price</td><td>$5.00/kg</td></tr></table></div>
        <table class="attr_table"><tr><td>Price</td><td>$9.99/kg</td></tr></table>
        </body></html>
    "#;

    let listing = TradewheelScraper.parse_listing(html, "https://tradewheel.com/p/x");
    assert_eq!(
        listing.unit_price,
        Some("$5.00/kg".to_string()),
        "expected the po-box table's value to win over attr_table's for the same label"
    );
}

#[test]
#[serial]
fn tradewheel_parse_listing_falls_back_to_quick_details_table_when_a_label_is_only_there() {
    let html = r#"
        <html><body>
        <table class="quick_details_table"><tr><td>Lead Time</td><td>15 days</td></tr></table>
        </body></html>
    "#;

    let listing = TradewheelScraper.parse_listing(html, "https://tradewheel.com/p/x");
    assert_eq!(listing.delivery_timeframe, Some("15 days".to_string()));
}

#[test]
#[serial]
fn tradewheel_parse_listing_reads_multiple_label_value_pairs_from_a_single_row() {
    // Quick Details rows can hold two label/value pairs side by side
    // in one <tr> - the chunks(2) walk must pick up both, not just
    // the first pair.
    let html = r#"
        <html><body>
        <table class="quick_details_table">
            <tr><td>Port</td><td>Shanghai</td><td>Packaging</td><td>Cartons</td></tr>
        </table>
        </body></html>
    "#;

    let listing = TradewheelScraper.parse_listing(html, "https://tradewheel.com/p/x");
    assert_eq!(listing.preferred_port, Some("Shanghai".to_string()));
    assert_eq!(listing.packaging_details, Some("Cartons".to_string()));
}

#[test]
#[serial]
fn tradewheel_parse_listing_extracts_images_preferring_data_zoom_image_over_data_image() {
    let html = r#"
        <html><body>
        <div class="pd-thumbs">
            <a data-zoom-image="https://cdn.tradewheel.com/zoom1.jpg" data-image="https://cdn.tradewheel.com/thumb1.jpg"></a>
            <a data-image="https://cdn.tradewheel.com/thumb2.jpg"></a>
        </div>
        </body></html>
    "#;

    let listing = TradewheelScraper.parse_listing(html, "https://tradewheel.com/p/x");

    assert_eq!(
        listing.image_urls,
        vec![
            "https://cdn.tradewheel.com/zoom1.jpg".to_string(),
            "https://cdn.tradewheel.com/thumb2.jpg".to_string(),
        ],
        "expected data-zoom-image to win when present, and data-image used as fallback otherwise"
    );
}

#[test]
#[serial]
fn tradewheel_parse_listing_caps_image_urls_at_three_even_when_more_are_present() {
    let html = r#"
        <html><body>
        <div class="pd-thumbs">
            <a data-image="https://cdn.tradewheel.com/1.jpg"></a>
            <a data-image="https://cdn.tradewheel.com/2.jpg"></a>
            <a data-image="https://cdn.tradewheel.com/3.jpg"></a>
            <a data-image="https://cdn.tradewheel.com/4.jpg"></a>
        </div>
        </body></html>
    "#;

    let listing = TradewheelScraper.parse_listing(html, "https://tradewheel.com/p/x");
    assert_eq!(listing.image_urls.len(), 3);
}

// --- TradeWheel scraper: extract_company_profile_url ---

#[test]
#[serial]
fn tradewheel_extracts_the_real_company_profile_link_from_a_listing_page() {
    let html = r#"<html><body><div class="comp-info"><a href="https://tradewheel.com/company/acme">Acme</a></div></body></html>"#;
    let url = TradewheelScraper.extract_company_profile_url(html);
    assert_eq!(url, Some("https://tradewheel.com/company/acme".to_string()));
}

#[test]
#[serial]
fn tradewheel_extract_company_profile_url_returns_none_when_genuinely_absent() {
    let html = "<html><body><a href=\"/other-link\">Contact</a></body></html>";
    assert_eq!(TradewheelScraper.extract_company_profile_url(html), None);
}

// --- TradeWheel scraper: enrich_from_company_profile ---

#[test]
#[serial]
fn tradewheel_enrich_from_company_profile_reads_company_information_section() {
    let html = r#"
        <html><body>
        <div class="co-specification-container">
            <h3 class="secondary-heading">Company Information</h3>
            <table>
                <tr><td>Established Year</td><td>2005</td></tr>
                <tr><td>Total Employees</td><td>101-200</td></tr>
            </table>
        </div>
        </body></html>
    "#;

    let supplier = B2bSupplierProfile::default();
    let enriched = TradewheelScraper.enrich_from_company_profile(supplier, html);

    assert_eq!(enriched.year_established, Some("2005".to_string()));
    assert_eq!(enriched.employee_count, Some("101-200".to_string()));
}

#[test]
#[serial]
fn tradewheel_enrich_from_company_profile_reads_trading_information_section() {
    let html = r#"
        <html><body>
        <div class="co-specification-container">
            <h3 class="secondary-heading">Trading Information</h3>
            <table>
                <tr><td>Total Revenue</td><td>US$1M - US$5M</td></tr>
                <tr><td>Export Percentage</td><td>70%</td></tr>
            </table>
        </div>
        </body></html>
    "#;

    let supplier = B2bSupplierProfile::default();
    let enriched = TradewheelScraper.enrich_from_company_profile(supplier, html);

    assert_eq!(enriched.sales_revenue, Some("US$1M - US$5M".to_string()));
    assert_eq!(enriched.export_percentage, Some("70%".to_string()));
}

#[test]
#[serial]
fn tradewheel_enrich_from_company_profile_reads_contact_details_name_logo_and_website() {
    let html = r#"
        <html><body>
        <div class="co-specification-container">
            <h3 class="secondary-heading">Contact Details</h3>
            <div class="contact_p_txt1">Jane Doe</div>
            <img id="m_img" src="https://cdn.tradewheel.com/logo.png" />
            <div class="contact_details">
                <table><tr><td>Website: https://real-supplier.example.com</td></tr></table>
            </div>
        </div>
        </body></html>
    "#;

    let supplier = B2bSupplierProfile::default();
    let enriched = TradewheelScraper.enrich_from_company_profile(supplier, html);

    assert_eq!(enriched.contact_name, Some("Jane Doe".to_string()));
    assert_eq!(
        enriched.logo_url,
        Some("https://cdn.tradewheel.com/logo.png".to_string())
    );
    assert_eq!(
        enriched.website_url,
        Some("https://real-supplier.example.com".to_string())
    );
}

#[test]
#[serial]
fn tradewheel_enrich_from_company_profile_never_sets_a_website_when_the_value_is_genuinely_just_show()
 {
    let html = r#"
        <html><body>
        <div class="co-specification-container">
            <h3 class="secondary-heading">Contact Details</h3>
            <div class="contact_details">
                <table><tr><td>Website: Show</td></tr></table>
            </div>
        </div>
        </body></html>
    "#;

    let supplier = B2bSupplierProfile::default();
    let enriched = TradewheelScraper.enrich_from_company_profile(supplier, html);
    assert_eq!(enriched.website_url, None);
}

#[test]
#[serial]
fn tradewheel_enrich_from_company_profile_show_filter_is_case_insensitive() {
    let html = r#"
        <html><body>
        <div class="co-specification-container">
            <h3 class="secondary-heading">Contact Details</h3>
            <div class="contact_details">
                <table><tr><td>Website: SHOW</td></tr></table>
            </div>
        </div>
        </body></html>
    "#;

    let supplier = B2bSupplierProfile::default();
    let enriched = TradewheelScraper.enrich_from_company_profile(supplier, html);
    assert_eq!(enriched.website_url, None);
}

#[test]
#[serial]
fn tradewheel_enrich_from_company_profile_ignores_sections_with_an_unrecognized_heading() {
    let html = r#"
        <html><body>
        <div class="co-specification-container">
            <h3 class="secondary-heading">Some Other Section</h3>
            <table><tr><td>Established Year</td><td>1999</td></tr></table>
        </div>
        </body></html>
    "#;

    let supplier = B2bSupplierProfile::default();
    let enriched = TradewheelScraper.enrich_from_company_profile(supplier, html);
    assert_eq!(
        enriched.year_established, None,
        "expected fields under an unrecognized heading to be genuinely ignored, not merged in"
    );
}

#[test]
#[serial]
fn tradewheel_enrich_from_company_profile_never_touches_other_fields() {
    let mut supplier = B2bSupplierProfile::default();
    supplier.company_name = Some("Existing Name Should Survive".to_string());
    let html = r#"
        <html><body>
        <div class="co-specification-container">
            <h3 class="secondary-heading">Company Information</h3>
            <table><tr><td>Established Year</td><td>2020</td></tr></table>
        </div>
        </body></html>
    "#;

    let enriched = TradewheelScraper.enrich_from_company_profile(supplier, html);
    assert_eq!(
        enriched.company_name,
        Some("Existing Name Should Survive".to_string())
    );
}

// --- ExportHub scraper: matches_platform ---

#[test]
#[serial]
fn exporthub_matches_platform_correctly_identifies_exporthub_only() {
    let scraper = ExporthubScraper;
    assert!(scraper.matches_platform("exporthub"));
    assert!(!scraper.matches_platform("tradewheel"));
    assert!(!scraper.matches_platform("alibaba"));
}

// --- ExportHub scraper: parse_supplier ---

#[test]
#[serial]
fn exporthub_parse_supplier_reads_company_name_and_a_genuine_logo() {
    let html = r#"
        <html><body>
        <div class="product-del_sidebar__comp-ttl">Real Exporter Co.</div>
        <div class="product-del_sidebar__comp-imgspn"><img src="https://cdn.exporthub.com/logo123.png"></div>
        </body></html>
    "#;

    let supplier = ExporthubScraper.parse_supplier(html, "https://exporthub.com/company/x");

    assert_eq!(supplier.company_name, Some("Real Exporter Co.".to_string()));
    assert_eq!(
        supplier.logo_url,
        Some("https://cdn.exporthub.com/logo123.png".to_string())
    );
    assert_eq!(supplier.source_platform, "exporthub");
}

#[test]
#[serial]
fn exporthub_parse_supplier_filters_out_the_real_placeholder_logo() {
    let html = r#"
        <html><body>
        <div class="product-del_sidebar__comp-ttl">No Real Logo Co.</div>
        <div class="product-del_sidebar__comp-imgspn"><img src="https://cdn.exporthub.com/noimg.png"></div>
        </body></html>
    "#;

    let supplier = ExporthubScraper.parse_supplier(html, "https://exporthub.com/company/x");
    assert_eq!(
        supplier.logo_url, None,
        "expected the genuine noimg placeholder to never be captured as a real logo"
    );
}

#[test]
#[serial]
fn exporthub_parse_supplier_reads_the_real_about_box_fields() {
    let html = r#"
        <html><body>
        <div class="prod-dtl_desp__abt-atr"><span>Year of Establishment: </span>2010</div>
        <div class="prod-dtl_desp__abt-atr"><span>Country / Region: </span>Pakistan</div>
        <div class="prod-dtl_desp__abt-atr"><span>Total Annual Revenue: </span>US$5 Million - US$10 Million</div>
        </body></html>
    "#;

    let supplier = ExporthubScraper.parse_supplier(html, "https://exporthub.com/company/x");

    assert_eq!(supplier.year_established, Some("2010".to_string()));
    assert_eq!(supplier.country, Some("Pakistan".to_string()));
    assert_eq!(
        supplier.sales_revenue,
        Some("US$5 Million - US$10 Million".to_string())
    );
}

#[test]
#[serial]
fn exporthub_parse_supplier_treats_not_provided_about_box_values_as_genuinely_absent() {
    let html = r#"
        <html><body>
        <div class="prod-dtl_desp__abt-atr"><span>Year of Establishment: </span>Not Provided</div>
        </body></html>
    "#;

    let supplier = ExporthubScraper.parse_supplier(html, "https://exporthub.com/company/x");
    assert_eq!(supplier.year_established, None);
}

#[test]
#[serial]
fn exporthub_parse_supplier_detects_a_genuine_premium_membership_seal() {
    let html = r#"
        <html><body>
        <div class="product-del_sidebar__seal"><img alt="Premium Membership" src="https://cdn.exporthub.com/seal.png"></div>
        </body></html>
    "#;

    let supplier = ExporthubScraper.parse_supplier(html, "https://exporthub.com/company/x");
    assert_eq!(
        supplier.badge_honorific,
        Some("Premium Membership".to_string())
    );
    assert!(supplier.platform_verified_badge);
}

#[test]
#[serial]
fn exporthub_parse_supplier_handles_a_genuinely_empty_page_without_panicking() {
    let html = "<html><body><p>Not an ExportHub listing at all</p></body></html>";
    let supplier = ExporthubScraper.parse_supplier(html, "https://exporthub.com/company/none");
    assert_eq!(supplier.company_name, None);
    assert_eq!(supplier.badge_honorific, None);
    assert!(!supplier.platform_verified_badge);
}

// --- ExportHub scraper: parse_listing ---

#[test]
#[serial]
fn exporthub_parse_listing_reads_title_price_and_description() {
    let html = r#"
        <html><body>
        <h1 class="prod-dtl_ttl">Bulk Industrial Fasteners</h1>
        <div class="prod-dtl_sl__pr">$0.10 - $0.50 / piece</div>
        <div id="detail"><p>High-grade steel fasteners for industrial use.</p></div>
        </body></html>
    "#;

    let listing = ExporthubScraper.parse_listing(html, "https://exporthub.com/x");

    assert_eq!(listing.title, Some("Bulk Industrial Fasteners".to_string()));
    assert_eq!(
        listing.unit_price,
        Some("$0.10 - $0.50 / piece".to_string())
    );
    assert_eq!(
        listing.description,
        Some("High-grade steel fasteners for industrial use.".to_string())
    );
}

#[test]
#[serial]
fn exporthub_parse_listing_reads_colon_separated_attribute_fields() {
    let html = r#"
        <html><body>
        <div class="prod-dtl_atr__box">Minimum Order Quantity: 500 pieces</div>
        <div class="prod-dtl_atr__box">Shipment Port: Karachi</div>
        <div class="prod-dtl_atr__box">Packaging: Cartons</div>
        </body></html>
    "#;

    let listing = ExporthubScraper.parse_listing(html, "https://exporthub.com/x");

    assert_eq!(
        listing.minimum_order_quantity,
        Some("500 pieces".to_string())
    );
    assert_eq!(listing.preferred_port, Some("Karachi".to_string()));
    assert_eq!(listing.packaging_details, Some("Cartons".to_string()));
}

#[test]
#[serial]
fn exporthub_parse_listing_joins_multiple_payment_method_icons() {
    let html = r#"
        <html><body>
        <span class="pm-icon" aria-label="Bank Transfer"></span>
        <span class="pm-icon" aria-label="Letter of Credit"></span>
        </body></html>
    "#;

    let listing = ExporthubScraper.parse_listing(html, "https://exporthub.com/x");
    assert_eq!(
        listing.payment_type,
        Some("Bank Transfer, Letter of Credit".to_string())
    );
}

#[test]
#[serial]
fn exporthub_parse_listing_payment_type_is_none_when_genuinely_no_icons_present() {
    let listing =
        ExporthubScraper.parse_listing("<html><body></body></html>", "https://exporthub.com/x");
    assert_eq!(listing.payment_type, None);
}

#[test]
#[serial]
fn exporthub_parse_listing_extracts_the_real_image_and_filters_the_noimage_placeholder() {
    let real_html =
        r#"<html><body><img id="show-img" src="https://cdn.exporthub.com/real.jpg"></body></html>"#;
    let placeholder_html = r#"<html><body><img id="show-img" src="https://cdn.exporthub.com/noimage.png"></body></html>"#;

    let real_listing = ExporthubScraper.parse_listing(real_html, "https://exporthub.com/x");
    let placeholder_listing =
        ExporthubScraper.parse_listing(placeholder_html, "https://exporthub.com/x");

    assert_eq!(
        real_listing.image_urls,
        vec!["https://cdn.exporthub.com/real.jpg".to_string()]
    );
    assert_eq!(
        placeholder_listing.image_urls,
        Vec::<String>::new(),
        "expected the genuine noimage placeholder to be excluded"
    );
}

// --- ExportHub scraper: extract_company_profile_url / build_extended_profile_url ---

#[test]
#[serial]
fn exporthub_extracts_the_real_company_profile_link() {
    let html = r#"<html><body><div class="product-del_sidebar__comp-nm"><a href="https://exporthub.com/company/real-exporter/">Real Exporter</a></div></body></html>"#;
    let url = ExporthubScraper.extract_company_profile_url(html);
    assert_eq!(
        url,
        Some("https://exporthub.com/company/real-exporter/".to_string())
    );
}

#[test]
#[serial]
fn exporthub_build_extended_profile_url_appends_correctly_when_the_profile_url_already_ends_in_a_slash()
 {
    let url =
        ExporthubScraper.build_extended_profile_url("https://exporthub.com/company/real-exporter/");
    assert_eq!(
        url,
        Some("https://exporthub.com/company/real-exporter/profile.html".to_string())
    );
}

#[test]
#[serial]
fn exporthub_build_extended_profile_url_inserts_a_slash_when_the_profile_url_genuinely_has_none() {
    let url =
        ExporthubScraper.build_extended_profile_url("https://exporthub.com/company/real-exporter");
    assert_eq!(
        url,
        Some("https://exporthub.com/company/real-exporter/profile.html".to_string())
    );
}

// --- ExportHub scraper: enrich_from_extended_profile ---
// (written against the CORRECTED version - see exporthub_prof_table_fix.rs)

#[test]
#[serial]
fn exporthub_enrich_from_extended_profile_reads_the_real_longer_description() {
    let html = r#"
        <html><body>
        <div class="rmp-comp--desp_cont">
            <p>Founded in 1998, we specialize in high-quality industrial exports. Name: Real Exporter Co.</p>
        </div>
        </body></html>
    "#;

    let supplier = B2bSupplierProfile::default();
    let enriched = ExporthubScraper.enrich_from_extended_profile(supplier, html);

    assert_eq!(
        enriched.company_description,
        Some("Founded in 1998, we specialize in high-quality industrial exports.".to_string()),
        "expected the description to be cut off cleanly before the trailing ' Name:' segment"
    );
}

#[test]
#[serial]
fn exporthub_enrich_from_extended_profile_keeps_the_full_paragraph_when_there_is_no_name_marker() {
    let html = r#"
        <html><body>
        <div class="rmp-comp--desp_cont">
            <p>A real, clean description with no trailing name marker at all.</p>
        </div>
        </body></html>
    "#;

    let supplier = B2bSupplierProfile::default();
    let enriched = ExporthubScraper.enrich_from_extended_profile(supplier, html);

    assert_eq!(
        enriched.company_description,
        Some("A real, clean description with no trailing name marker at all.".to_string())
    );
}

#[test]
#[serial]
fn exporthub_enrich_from_extended_profile_reads_total_workforce_and_year_incorporated_from_the_table()
 {
    let html = r#"
        <html><body>
        <table class="rmp-comp--prof_table">
            <tr><td>Total Workforce</td><td>51-100</td></tr>
            <tr><td>Year Incorporated</td><td>2005</td></tr>
        </table>
        </body></html>
    "#;

    let supplier = B2bSupplierProfile::default();
    let enriched = ExporthubScraper.enrich_from_extended_profile(supplier, html);

    assert_eq!(enriched.employee_count, Some("51-100".to_string()));
    assert_eq!(enriched.year_established, Some("2005".to_string()));
}

#[test]
#[serial]
fn exporthub_enrich_from_extended_profile_never_overwrites_a_value_already_found_on_the_main_profile()
 {
    let mut supplier = B2bSupplierProfile::default();
    supplier.employee_count = Some("Already Found: 200+".to_string());

    let html = r#"
        <html><body>
        <table class="rmp-comp--prof_table">
            <tr><td>Total Workforce</td><td>51-100</td></tr>
        </table>
        </body></html>
    "#;

    let enriched = ExporthubScraper.enrich_from_extended_profile(supplier, html);
    assert_eq!(
        enriched.employee_count,
        Some("Already Found: 200+".to_string())
    );
}

#[test]
#[serial]
fn exporthub_enrich_from_extended_profile_table_ignores_not_provided_values() {
    let html = r#"
        <html><body>
        <table class="rmp-comp--prof_table">
            <tr><td>Total Workforce</td><td>Not Provided</td></tr>
        </table>
        </body></html>
    "#;

    let supplier = B2bSupplierProfile::default();
    let enriched = ExporthubScraper.enrich_from_extended_profile(supplier, html);
    assert_eq!(enriched.employee_count, None);
}

// --- ExportHub scraper: enrich_from_company_profile ---

#[test]
#[serial]
fn exporthub_enrich_from_company_profile_reads_comp_dtl_ic_box_fields() {
    let html = r#"
        <html><body>
        <div class="comp-dtl-ic_box">
            <h4 class="comp-dtl_rgtnm">No. Employees</h4>
            <p class="comp-dtl_rgtp">201-500</p>
        </div>
        <div class="comp-dtl-ic_box">
            <h4 class="comp-dtl_rgtnm">Annual Turnover</h4>
            <p class="comp-dtl_rgtp">US$10M - US$50M</p>
        </div>
        </body></html>
    "#;

    let supplier = B2bSupplierProfile::default();
    let enriched = ExporthubScraper.enrich_from_company_profile(supplier, html);

    assert_eq!(enriched.employee_count, Some("201-500".to_string()));
    assert_eq!(enriched.sales_revenue, Some("US$10M - US$50M".to_string()));
}

#[test]
#[serial]
fn exporthub_enrich_from_company_profile_reads_export_percentage_and_estimated_employees_from_list_divs()
 {
    let html = r#"
        <html><body>
        <p class="list-div">Export Percentage: 60%</p>
        <p class="list-div">Estimated Employees: 100-200</p>
        </body></html>
    "#;

    let supplier = B2bSupplierProfile::default();
    let enriched = ExporthubScraper.enrich_from_company_profile(supplier, html);

    assert_eq!(enriched.export_percentage, Some("60%".to_string()));
    assert_eq!(enriched.employee_count, Some("100-200".to_string()));
}

#[test]
#[serial]
fn exporthub_enrich_from_company_profile_never_overwrites_export_percentage_already_found() {
    let mut supplier = B2bSupplierProfile::default();
    supplier.export_percentage = Some("Already Found: 90%".to_string());

    let html = r#"<html><body><p class="list-div">Export Percentage: 60%</p></body></html>"#;
    let enriched = ExporthubScraper.enrich_from_company_profile(supplier, html);
    assert_eq!(
        enriched.export_percentage,
        Some("Already Found: 90%".to_string())
    );
}

// --- looks_like_a_real_page ---

#[test]
#[serial]
fn looks_like_a_real_page_rejects_html_under_the_real_byte_threshold() {
    let small_html = format!(
        "<!doctype html><html><body>{}</body></html>",
        "x".repeat(10)
    );
    assert!(!looks_like_a_real_page(&small_html));
}

#[test]
#[serial]
fn looks_like_a_real_page_accepts_a_genuinely_large_page_with_an_html_tag() {
    let real_html = format!("<html><body>{}</body></html>", "x".repeat(2200));
    assert!(looks_like_a_real_page(&real_html));
}

#[test]
#[serial]
fn looks_like_a_real_page_accepts_a_genuinely_large_page_with_a_doctype_only() {
    let real_html = format!("<!doctype html>{}", "x".repeat(2200));
    assert!(looks_like_a_real_page(&real_html));
}

#[test]
#[serial]
fn looks_like_a_real_page_rejects_a_large_but_genuinely_non_html_body() {
    // Guards the "likely ScraperAPI garbage" case directly - a large
    // JSON error blob with no real markers at all must still be
    // rejected, size alone isn't enough.
    let fake_html = format!("{{\"error\": \"{}\"}}", "x".repeat(2200));
    assert!(!looks_like_a_real_page(&fake_html));
}

#[test]
#[serial]
fn looks_like_a_real_page_marker_check_is_case_insensitive() {
    let real_html = format!("<HTML><BODY>{}</BODY></HTML>", "x".repeat(2200));
    assert!(looks_like_a_real_page(&real_html));
}

// --- B2bScraper trait defaults: build_extended_profile_url / enrich_from_extended_profile ---
// Alibaba and B2Brazil don't override these, so they exercise the
// trait's real default implementations directly.

#[test]
#[serial]
fn scrapers_that_do_not_override_build_extended_profile_url_genuinely_return_none() {
    assert_eq!(
        AlibabaScraper.build_extended_profile_url("https://alibaba.com/company/x"),
        None
    );
    assert_eq!(
        B2brazilScraper.build_extended_profile_url("https://b2brazil.com/hotsite/x"),
        None
    );
}

#[test]
#[serial]
fn scrapers_that_do_not_override_enrich_from_extended_profile_genuinely_leave_the_supplier_unchanged()
 {
    let mut supplier = B2bSupplierProfile::default();
    supplier.company_name = Some("Should Survive Untouched".to_string());

    let enriched =
        AlibabaScraper.enrich_from_extended_profile(supplier, "<html><body>anything</body></html>");

    assert_eq!(
        enriched.company_name,
        Some("Should Survive Untouched".to_string())
    );
}

#[test]
fn get_scraper_for_platform_finds_the_newly_registered_thomasnet_scraper() {
    let scraper = backend::services::b2b_scrapers::get_scraper_for_platform("thomasnet");
    assert!(
        scraper.is_some(),
        "expected thomasnet to be registered in the real scraper list"
    );
    assert!(scraper.unwrap().matches_platform("thomasnet"));
}

#[test]
fn get_scraper_for_platform_finds_the_newly_registered_b2bmap_scraper() {
    let scraper = backend::services::b2b_scrapers::get_scraper_for_platform("b2bmap");
    assert!(
        scraper.is_some(),
        "expected b2bmap to be registered in the real scraper list"
    );
    assert!(scraper.unwrap().matches_platform("b2bmap"));
}

#[test]
fn get_scraper_for_platform_still_finds_every_other_registered_scraper_after_b2bmap_was_added() {
    for platform in [
        "alibaba",
        "tradewheel",
        "exporthub",
        "b2brazil",
        "thomasnet",
    ] {
        assert!(
            backend::services::b2b_scrapers::get_scraper_for_platform(platform).is_some(),
            "expected '{}' to still be found after b2bmap was added",
            platform
        );
    }
}

#[test]
fn country_code_for_platform_returns_us_for_b2bmap() {
    assert_eq!(
        backend::services::scraper_client::country_code_for_platform("b2bmap"),
        Some("us")
    );
}

#[test]
fn get_scraper_for_platform_still_finds_every_other_registered_scraper() {
    // Regression guard - confirms adding thomasnet to the Vec didn't
    // accidentally push out or shadow any existing B2B entry.
    for platform in ["alibaba", "tradewheel", "exporthub", "b2brazil"] {
        assert!(
            backend::services::b2b_scrapers::get_scraper_for_platform(platform).is_some(),
            "expected '{}' to still be found after thomasnet was added",
            platform
        );
    }
}
