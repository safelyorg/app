use backend::services::b2c_scrapers::check_listing_page;

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
    let result = check_listing_page("some_platform_that_does_not_exist", "https://example.com").await;
    assert!(result.is_none());
}
