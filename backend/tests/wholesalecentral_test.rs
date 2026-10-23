use backend::services::b2b_scrapers::get_scraper_for_platform;

// A small page built the same way as Wholesale Central's real pages:
// the data rows inside a self.__next_f.push([1,"..."]) script.
fn page(rows: &str) -> String {
    let pushed = serde_json::to_string(&serde_json::json!([1, rows])).unwrap();
    format!(
        "<!DOCTYPE html><html><body>{}<script>self.__next_f.push({pushed})</script></body></html>",
        "x".repeat(2500)
    )
}

fn product_page() -> String {
    page(concat!(
        "b:[\"$\",\"div\",null,{\"product\":{\"id\":33248501,\"name\":\"G102-Wireless TORIMA earphones\",",
        "\"image\":\"https://shopiphoenix.com/assets/images/G102-1.jpg\",",
        "\"description\":\"G102-Wireless TORIMA earphones. Contact us for more details.\",",
        "\"category\":\"Batteries\",\"supplierName\":\"iPhoenix Technology Corp. \",\"supplierId\":63409,",
        "\"supplierMinimumOrder\":300,\"memberSince\":\"2026-05-29T19:28:59.481\",",
        "\"email\":\"usaiphoenix@gmail.com\",\"companyCity\":\"South El Monte \",\"companyStateCode\":\"CA\",",
        "\"subdirectory\":\"iphoenixtechnologycorp-1f696edcb8e5ff3b\",\"wholesaleLink\":\"\",",
        "\"pricing\":[],\"productVariants\":[{\"priceAdjustmentAmount\":6.5,\"sku\":\"G102\"}]}}]\n",
    ))
}

#[test]
fn wholesale_central_is_a_b2b_platform() {
    assert!(get_scraper_for_platform("wholesalecentral").is_some());
}

#[test]
fn a_product_page_gives_the_product_and_its_supplier_page() {
    let scraper = get_scraper_for_platform("wholesalecentral").unwrap();
    let html = product_page();
    let url = "https://www.wholesalecentral.com/products/33248501";

    let listing = scraper.parse_listing(&html, url);
    assert_eq!(
        listing.title.as_deref(),
        Some("G102-Wireless TORIMA earphones")
    );
    assert_eq!(
        listing.minimum_order_quantity.as_deref(),
        Some("$300 minimum order")
    );
    assert_eq!(listing.unit_price, None, "the hidden price is never used");

    let supplier = scraper.parse_supplier(&html, url);
    assert_eq!(
        supplier.company_name.as_deref(),
        Some("iPhoenix Technology Corp.")
    );
    assert_eq!(supplier.country.as_deref(), Some("South El Monte, CA"));

    assert_eq!(
        scraper.extract_company_profile_url(&html).as_deref(),
        Some("https://www.wholesalecentral.com/suppliers/iphoenixtechnologycorp-1f696edcb8e5ff3b")
    );
    assert_eq!(scraper.company_key(&html).as_deref(), Some("63409"));
    let record = scraper.supplier_record(&html).unwrap();
    assert_eq!(record.platform, "Wholesale Central");
    assert_eq!(record.joined_platform_year, Some(2026));
}
