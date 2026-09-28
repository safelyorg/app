use backend::services::b2c_scrapers::{check_listing_page, check_store_page};
use serial_test::serial;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

#[tokio::test]
async fn check_listing_page_fetches_and_parses_a_real_live_olx_page() {
    let real_url = "https://www.olx.com.pk/item/electric-scooty-electric-scooter-bikes-2026-zero-meter-iid-1117692571";

    let result = check_listing_page("olx", real_url).await;

    assert!(
        result.is_some(),
        "expected a real, successful fetch and parse against OLX's live site"
    );

    let data = result.unwrap();
    println!("Fetched title: {:?}", data.title);
    println!("Fetched price: {:?}", data.price);
    println!("Fetched description: {:?}", data.description);
    println!("Fetched seller_name: {:?}", data.seller_name);
    println!("Fetched location: {:?}", data.location);

    assert!(data.title.is_some(), "expected a real title to be found");
    assert!(data.price.is_some(), "expected a real price to be found");
}

#[tokio::test]
async fn check_listing_page_returns_none_for_a_genuinely_unrecognized_platform() {
    let result =
        check_listing_page("some_platform_that_does_not_exist", "https://example.com").await;
    assert!(result.is_none());
}

fn pad_html(inner: &str) -> String {
    format!(
        "<!doctype html><html><body>{}<div style=\"display:none\">{}</div></body></html>",
        inner,
        "x".repeat(2200)
    )
}

// --- check_listing_page ---

#[tokio::test]
#[serial]
async fn check_listing_page_returns_some_for_a_genuinely_real_sized_page() {
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/listing"))
        .respond_with(ResponseTemplate::new(200).set_body_string(pad_html("<h1>Real Listing</h1>")))
        .expect(1)
        .mount(&mock_server)
        .await;

    let listing_url = format!("{}/listing", mock_server.uri());
    let result = check_listing_page("olx", &listing_url).await;

    assert!(result.is_some());
    mock_server.verify().await;
}

#[tokio::test]
#[serial]
async fn check_listing_page_returns_none_when_the_page_is_too_small_dependency_down() {
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/listing"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html><body>tiny</body></html>"))
        .mount(&mock_server)
        .await;

    let listing_url = format!("{}/listing", mock_server.uri());
    let result = check_listing_page("olx", &listing_url).await;

    assert!(
        result.is_none(),
        "expected a suspiciously small response to be treated as DEPENDENCY DOWN, not a real page"
    );
}

#[tokio::test]
#[serial]
async fn check_listing_page_returns_none_when_the_fetch_fails() {
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/listing"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let listing_url = format!("{}/listing", mock_server.uri());
    let result = check_listing_page("olx", &listing_url).await;

    assert!(result.is_none());
}

#[tokio::test]
#[serial]
async fn check_listing_page_genuinely_never_retries_a_failed_fetch() {
    // Direct regression guard for the retry loop's removal - a 500
    // must be requested EXACTLY once, not up to 3 times like before.
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
    let _ = check_listing_page("olx", &listing_url).await;

    mock_server.verify().await;
}

// --- check_store_page ---

#[tokio::test]
#[serial]
async fn check_store_page_returns_some_for_a_genuinely_real_sized_page() {
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/store"))
        .respond_with(ResponseTemplate::new(200).set_body_string(pad_html("<h1>Real Store</h1>")))
        .expect(1)
        .mount(&mock_server)
        .await;

    let store_url = format!("{}/store", mock_server.uri());
    let result = check_store_page("olx", &store_url, "Some Seller").await;

    assert!(result.is_some());
    mock_server.verify().await;
}

#[tokio::test]
#[serial]
async fn check_store_page_returns_none_when_the_page_is_too_small_dependency_down() {
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/store"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html><body>tiny</body></html>"))
        .mount(&mock_server)
        .await;

    let store_url = format!("{}/store", mock_server.uri());
    let result = check_store_page("olx", &store_url, "Some Seller").await;

    assert!(result.is_none());
}

#[tokio::test]
#[serial]
async fn check_store_page_genuinely_never_retries_a_failed_fetch() {
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/store"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&mock_server)
        .await;

    let store_url = format!("{}/store", mock_server.uri());
    let _ = check_store_page("olx", &store_url, "Some Seller").await;

    mock_server.verify().await;
}

#[tokio::test]
#[serial]
async fn check_store_page_returns_none_for_a_genuinely_unrecognized_platform() {
    unsafe {
        std::env::remove_var("SCRAPERAPI_KEY");
    }
    let result =
        check_store_page("not-a-real-platform", "https://example.com/store", "Seller").await;
    assert!(result.is_none());
}
