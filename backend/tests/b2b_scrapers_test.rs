mod common;

use crate::common::test_pool;
use backend::{
    models::analysis::AnalyzeRequest,
    services::{
        analysis::build_b2b_analysis_path,
        b2b_scrapers::{
            B2bScraper, B2bSupplierProfile, alibaba::AlibabaScraper, b2brazil::B2brazilScraper,
            check_b2b_page,
        },
    },
};
use chrono::{Datelike, Utc};
use serial_test::serial;
use std::env::remove_var;
use uuid::Uuid;
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
async fn build_b2b_analysis_path_fails_gracefully_for_a_genuinely_broken_url() {
    let pool = test_pool().await;
    let request = AnalyzeRequest {
        platform: "b2brazil".to_string(),
        listing_url: "https://this-domain-genuinely-does-not-exist-xyz789.com".to_string(),
        seller_id: None,
        listing_id: None,
        title: None,
        price: None,
        description: None,
        category: None,
        image_urls: None,
        posted_date: None,
        platform_id: None,
        seller_name: None,
        seller_handle: None,
        seller_phone: None,
        seller_profile_url: None,
        seller_join_date: None,
        seller_location: None,
        seller_last_active: None,
        seller_website: None,
        seller_verified: None,
        seller_rating: None,
        seller_total_products: None,
        domain_check_status: None,
        domain_check_real_name: None,
        domain_check_real_domain: None,
        domain_check_current_domain: None,
        domain_check_current_html: None,
        domain_check_real_html: None,
        language: None,
    };

    let result = build_b2b_analysis_path(&pool, &request, 0, Uuid::new_v4()).await;
    assert!(
        result.is_err(),
        "expected the analysis to fail gracefully for a genuinely broken URL, but it succeeded"
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
fn alibaba_parse_supplier_reads_an_unbadged_listing_correctly() {
    let html = r#"
        <html><body>
        <div data-testid="three-column-mini-company-card">
            <a class="id-underline">Shenzhen Trustco Electronics Co., Ltd.</a>
            <img src="https://img.alibaba.com/logo.png" />
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

    let expected_year = (Utc::now().year() - 10).to_string();
    assert_eq!(supplier.year_established, Some(expected_year));
}

#[test]
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

    let expected_year = (chrono::Utc::now().year() - 8).to_string();
    assert_eq!(supplier.year_established, Some(expected_year));
}

#[test]
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
fn alibaba_extract_company_profile_url_returns_none_when_the_link_is_genuinely_absent() {
    let html = "<html><body><a href=\"/other-link.html\">Contact supplier</a></body></html>";
    assert_eq!(AlibabaScraper.extract_company_profile_url(html), None);
}

#[test]
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
fn b2brazil_extract_company_profile_url_keeps_an_absolute_href_as_is() {
    let html = r#"<html><body><a class="nav-home" href="https://b2brazil.com/hotsite/acme">Home</a></body></html>"#;
    let url = B2brazilScraper.extract_company_profile_url(html);
    assert_eq!(url, Some("https://b2brazil.com/hotsite/acme".to_string()));
}

#[test]
fn b2brazil_extract_company_profile_url_prefixes_a_relative_href_with_the_real_domain() {
    let html = r#"<html><body><a class="nav-home" href="/hotsite/acme">Home</a></body></html>"#;
    let url = B2brazilScraper.extract_company_profile_url(html);
    assert_eq!(url, Some("https://b2brazil.com/hotsite/acme".to_string()));
}

#[test]
fn b2brazil_extract_company_profile_url_returns_none_when_the_link_is_genuinely_absent() {
    let html = "<html><body><a href=\"/something-else\">Not it</a></body></html>";
    assert_eq!(B2brazilScraper.extract_company_profile_url(html), None);
}

// --- B2Brazil: enrich_from_company_profile ---

#[test]
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
fn b2brazil_enrich_from_company_profile_leaves_description_none_when_the_section_is_genuinely_absent()
 {
    let html = "<html><body><p>No about section on this page</p></body></html>";
    let supplier = B2bSupplierProfile::default();
    let enriched = B2brazilScraper.enrich_from_company_profile(supplier, html);
    assert_eq!(enriched.company_description, None);
}

#[test]
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
        std::env::remove_var("SCRAPERAPI_KEY");
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
