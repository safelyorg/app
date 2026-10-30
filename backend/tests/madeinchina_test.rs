use backend::services::sources::b2b::{b2b_page_from_browser_first, get_scraper_for_platform};

// Small pages built from the real Made-in-China pages (October 2026):
// the same elements and classes, with most of the text left out.

/// Pads a page past the "too small to be a real page" check.
fn page(head: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html><html><head>{head}</head><body>{body}<div>{}</div></body></html>",
        "x".repeat(2500)
    )
}

/// A product page on the supplier's shop (Changzhou Nanyi Bearing).
fn product_page() -> String {
    page(
        r#"<title>Heavy Duty Cat Excavator Pins - Bushing price | Made-in-china.com</title>
        <link rel="canonical" href="https://yongjie-bearing.en.made-in-china.com/product/cEvrzSKdrhpB/China-Heavy-Duty-Cat-Excavator-Pins.html" />"#,
        r#"<div class="sts"><span>Secured Trading Service</span></div>
        <h1>Heavy Duty Cat Excavator Pins &amp; Bushings Kit</h1>
        <a class="sr-comInfo-title" href="https://yongjie-bearing.en.made-in-china.com">Changzhou Nanyi Bearing Co., Ltd.</a>
        <div class="member"><span>Audited by an independent third-party inspection agency</span>
          <span>2 yrs</span><span>Diamond Member </span><span>Since 2025</span>
          <span>Suppliers with verified business licenses</span><span>5.0</span><span>(2 Reviews)</span></div>
        <div class="only-one-priceNum-tr only-one-priceNum">
          <span class="only-one-priceNum-td-left">US$5.00 </span>
          <span class="sa-only-property-price only-one-priceNum-price">100 Pieces(MOQ)</span></div>
        <dl class="product-attrs-list J-product-attrs-list">
          <dt class="product-attrs-label">Condition</dt><dd class="product-attrs-value">Used</dd>
          <dt class="product-attrs-label">Trademark</dt><dd class="product-attrs-value">JRZC or OEM</dd>
          <dt class="product-attrs-label is-extra">Transport Package</dt><dd class="product-attrs-value">Wooden Pallet</dd>
          <dt class="product-attrs-label is-extra">Production Capacity</dt><dd class="product-attrs-value">100000pieces/Year</dd></dl>
        <section class="sr-comProfile">
          <div>This supplier has been audited by TÜV Rheinland. Verify the report on <a>the official TÜV Rheinland website</a> with ID "<span>MIC-ASI254819</span>". Items marked "<i></i>" are certified.</div>
          <dl class="sr-comProfile-infos">
            <dt class="sr-comProfile-label"> Business Type</dt><dd class="sr-comProfile-fields"> Manufacturer/Factory &amp; Trading Company</dd>
            <dt class="sr-comProfile-label"> Address</dt><dd class="sr-comProfile-fields"> No. 888, Changwu Mid Road, Wujin Distric, Changzhou, Jiangsu.</dd>
            <dt class="sr-comProfile-label"> Registered Capital</dt><dd class="sr-comProfile-fields">1,000,000 RMB</dd>
            <dt class="sr-comProfile-label"> Terms of Payment</dt><dd class="sr-comProfile-fields">LC, T/T</dd>
            <dt class="sr-comProfile-label"> International Commercial Terms(Incoterms)</dt><dd class="sr-comProfile-fields">FOB, CIF, CFR, EXW</dd>
            <dt class="sr-comProfile-label"> Average Lead Time</dt><dd class="sr-comProfile-fields">Peak Season Lead Time: within 15 workdays<br />Off Season Lead Time: within 15 workdays</dd>
            <dt class="sr-comProfile-label"> Nearest Port</dt><dd class="sr-comProfile-fields">Shanghai Port</dd>
            <dt class="sr-comProfile-label"> Main Products</dt><dd class="sr-comProfile-fields">Tapered Roller Bearing <span>,</span> Spherical Roller Bearing</dd>
          </dl></section>
        <script type="application/ld+json">{"@context":"https://schema.org","@type":"Product","name":"Heavy Duty Cat Excavator Pins & Bushings Kit with Customizable Options","image":["https://image.made-in-china.com/2f0j00ZlzowPihZIpW/a.webp","//image.made-in-china.com/2f0j00ShgqJblrZIpV/b.webp"],"description":"Heavy Duty Cat Excavator Pins & Bushings Kit. High Wear Resistance.","offers":{"@type":"Offer","priceCurrency":"USD","price":"5.00"}}</script>
        <script type="application/ld+json">{"@context":"https://schema.org","@type":"Organization","url":"https://yongjie-bearing.en.made-in-china.com","logo":"https://image.made-in-china.com/205f0j00IagYneEcqUbl/Changzhou-Nanyi-Bearing-Co-Ltd-.jpg","name":"Changzhou Nanyi Bearing Co., Ltd.","description":"Wafangdian Jinrui Bearing Manufacturing Co., Ltd. is located in Dalian. According to the plan, Changzhou Nanyi Bearing Co., Ltd. Will be established in 2021. The company&apos;s trademark is JRZC."}</script>
        <script>var tracking = "Wafangdian";</script>"#,
    )
}

/// The same supplier's own page.
fn supplier_page() -> String {
    page(
        r#"<title>Spherical Bearing Manufacturer, Bearing Supplier - Changzhou Nanyi Bearing Co., Ltd.</title>
        <link rel="canonical" href="https://yongjie-bearing.en.made-in-china.com"/>"#,
        r#"<h1 class="shop-sign__header-title--text">Changzhou Nanyi Bearing Co., Ltd.</h1>
        <div class="detail-address"><i class="ob-icon icon-coordinate"></i> Liaoning, China </div>
        <div class="info-item"><div class="info-label"><i></i>Business Type:</div><div class="info-fields"> Manufacturer/Factory &amp; Trading Company </div></div>
        <div class="info-item"><div class="info-label">Year of Establishment:</div><div class="info-fields"> 2021-03-02 </div></div>
        <div class="info-item"><div class="info-label">Number of Employees:</div><div class="info-fields"> 15 </div></div>
        <div class="info-item"><div class="info-label">Address:</div><div class="info-fields"> No. 888, Changwu Mid Road, Wujin Distric, Changzhou, Jiangsu. </div></div>
        <div class="info-item"><div class="info-label">Terms of Payment:</div><div class="info-fields"> LC, T/T, Western Union </div></div>
        <div><span>Diamond Member</span><span> Since 2025</span><span>Suppliers with verified business licenses</span><span>Audited Supplier</span></div>
        <div class="prod-item"><a class="prod-name" href="//yongjie-bearing.en.made-in-china.com/product/KmHrvebAaQpo/China-Bearing.html">Low Price Spherical Roller Bearing</a>
          <div class="prod-price">US$0.01 -10.00 / Piece</div><div>1 Piece (MOQ)</div></div>
        <div class="prod-item"><a class="prod-name" href="/product/Abc/China-NSK-Bearing.html">NTN NSK Koyo Needle Roller Bearing</a>
          <div class="prod-price">US$1.90 -25.00 / Piece</div></div>
        <a href="https://www.made-in-china.com/product/Other-Supplier-Item-xyz.html">Another supplier's product</a>
        <span class="sr-sendMsg-name">Mrs. Ann</span>"#,
    )
}

/// An older product page with no company data block (XCMG).
fn old_product_page() -> String {
    page(
        r#"<link rel="canonical" href="https://machmall.en.made-in-china.com/product/FbPQmuwraRlL/China-XCMG-Concrete-Pump.html" />"#,
        r#"<h1>XCMG Official 43m Second Hand Concrete Pump</h1>
        <table><tr class="only-one-priceNum-tr"><td><span class="only-one-priceNum-td-left">US$10,000.00-1,000,000.00</span></td><td><span>1 Unit</span> <span>(MOQ)</span></td></tr></table>
        <a href="https://machmall.en.made-in-china.com" target="_blank" title="XCMG E-Commerce Inc."><span class="text-ellipsis">XCMG E-Commerce Inc.</span></a>
        <div class="tip-con"> Jiangsu, China </div>
        <span>Diamond Member</span><span>Since 2013</span><span>Audited Supplier</span>
        <div class="bsc-item cf"><div class="bac-item-label fl">Model NO.</div><div class="bac-item-value fl">HB43K</div></div>
        <div class="bsc-item cf"><div class="bac-item-label fl">Transport Package</div><div class="bac-item-value fl">Comply with Export Packaging Requirements</div></div>
        <script type="application/ld+json">{"@context":"https://schema.org","@type":"Product","name":"XCMG Official 43m Second Hand Truck Mounted Boom Concrete Pump Hb43K Price","image":["https://image.made-in-china.com/2f0j00TzlqksiMgprb/x.webp"]}</script>
        <script type="application/ld+json">
        </script>"#,
    )
}

/// A product page on the main site with tiered prices (Angtong).
fn main_site_product_page() -> String {
    page(
        r#"<link rel="canonical" href="https://www.made-in-china.com/product/Elitebook650-G10-Business-Laptop-UJrpmMjbvoRP.html" />"#,
        r#"<ul class="swiper-wrapper-div swiper-wrapper">
          <li class="swiper-slide-div swiper-slide"><span class="swiper-money-container">US$901.00</span><div class="swiper-unit-container">1-9 <span class="unit">Pieces</span></div></li>
          <li class="swiper-slide-div swiper-slide"><span class="swiper-money-container">US$896.00</span><div class="swiper-unit-container">10-49 <span class="unit">Pieces</span></div></li>
          <li class="swiper-slide-div swiper-slide"><span class="swiper-money-container">US$901.00</span><div class="swiper-unit-container">1-9 <span class="unit">Pieces</span></div></li>
        </ul>
        <dl class="sr-comProfile-infos"><dt class="sr-comProfile-label">Terms of Payment</dt><dd class="sr-comProfile-fields">LC, T/T, D/P, PayPal, Western Union, Small-amount payment, Money Gram</dd></dl>
        <script type="application/ld+json">{"@type":"Product","name":"Elitebook650 G10 Business Laptop 15.6inch I7-1355u 32g 2t SSD"}</script>
        <script type="application/ld+json">{"@type":"Organization","url":"https://angtongtech.en.made-in-china.com","name":"Shanghai Angtong Information Technology Co., Ltd."}</script>"#,
    )
}

/// A supplier's own page with no heading (Angtong): the name comes from
/// the page title.
fn supplier_page_without_heading() -> String {
    page(
        r#"<title>Business Computer Manufacturer, Server Supplier - Shanghai Angtong Information Technology Co., Ltd.</title>
        <link rel="canonical" href="https://angtongtech.en.made-in-china.com"/>"#,
        r#"<div class="detail-address">Beijing, China</div>"#,
    )
}

const PRODUCT_URL: &str = "https://yongjie-bearing.en.made-in-china.com/product/cEvrzSKdrhpB/China-Heavy-Duty-Cat-Excavator-Pins.html";
const SHOP_URL: &str = "https://yongjie-bearing.en.made-in-china.com/";

#[test]
fn made_in_china_is_a_b2b_platform() {
    assert!(get_scraper_for_platform("madeinchina").is_some());
}

#[test]
fn a_product_page_gives_the_product() {
    let scraper = get_scraper_for_platform("madeinchina").unwrap();
    let l = scraper.parse_listing(&product_page(), PRODUCT_URL);
    assert_eq!(
        l.title.as_deref(),
        Some("Heavy Duty Cat Excavator Pins & Bushings Kit with Customizable Options")
    );
    assert_eq!(l.unit_price.as_deref(), Some("US$5.00"));
    assert_eq!(l.minimum_order_quantity.as_deref(), Some("100 Pieces"));
    assert_eq!(
        l.payment_type.as_deref(),
        Some("LC, T/T; Made-in-China Secured Trading Service (escrow)")
    );
    assert_eq!(l.incoterms.as_deref(), Some("FOB, CIF, CFR, EXW"));
    assert_eq!(
        l.delivery_timeframe.as_deref(),
        Some("Peak Season Lead Time: within 15 workdays Off Season Lead Time: within 15 workdays")
    );
    assert_eq!(l.preferred_port.as_deref(), Some("Shanghai Port"));
    assert_eq!(l.production_capacity.as_deref(), Some("100000pieces/Year"));
    assert_eq!(l.packaging_details.as_deref(), Some("Wooden Pallet"));
    assert_eq!(
        l.image_urls,
        vec![
            "https://image.made-in-china.com/2f0j00ZlzowPihZIpW/a.webp",
            "https://image.made-in-china.com/2f0j00ShgqJblrZIpV/b.webp"
        ]
    );
    assert_eq!(
        l.description.as_deref(),
        Some(
            "Heavy Duty Cat Excavator Pins & Bushings Kit. High Wear Resistance.\nProduct details: Condition: Used; Trademark: JRZC or OEM; Transport Package: Wooden Pallet; Production Capacity: 100000pieces/Year."
        )
    );
    assert_eq!(l.source_platform, "madeinchina");
}

#[test]
fn a_product_page_gives_the_supplier_and_its_checks() {
    let scraper = get_scraper_for_platform("madeinchina").unwrap();
    let html = product_page();
    let s = scraper.parse_supplier(&html, PRODUCT_URL);
    assert_eq!(
        s.company_name.as_deref(),
        Some("Changzhou Nanyi Bearing Co., Ltd.")
    );
    assert_eq!(
        s.logo_url.as_deref(),
        Some(
            "https://image.made-in-china.com/205f0j00IagYneEcqUbl/Changzhou-Nanyi-Bearing-Co-Ltd-.jpg"
        )
    );
    assert!(s.platform_verified_badge, "audited on site");
    assert_eq!(
        s.badge_honorific.as_deref(),
        Some("Audited on site by TÜV Rheinland (report MIC-ASI254819)")
    );
    assert_eq!(s.country.as_deref(), Some("China"));
    let d = s.company_description.unwrap();
    assert!(
        d.starts_with("Address: No. 888, Changwu Mid Road, Wujin Distric, Changzhou, Jiangsu.\n")
    );
    assert!(d.contains("\nBusiness type: Manufacturer/Factory & Trading Company.\n"));
    assert!(d.contains("\nMain products: Tapered Roller Bearing, Spherical Roller Bearing.\n"));
    assert!(d.contains(
        "\nOn Made-in-China: Diamond Member (a paid membership) since 2025; 2 years on Made-in-China; rating 5.0 out of 5 from 2 reviews; audited on site by TÜV Rheinland (report MIC-ASI254819); business licence checked by Made-in-China.\n"
    ));
    // The profile describes another company: Claude is told.
    assert!(d.contains(
        "\nThe company profile also names other companies: Wafangdian Jinrui Bearing Manufacturing Co., Ltd. Ask how they are related.\n"
    ));
    assert!(
        d.ends_with("The company's trademark is JRZC."),
        "HTML codes are decoded"
    );
    assert!(!d.contains("tracking"), "scripts are never read");

    let r = scraper.supplier_record(&html).unwrap();
    assert_eq!(r.platform, "Made-in-China");
    assert!(r.checked_by_platform);
    assert_eq!(r.member_label.as_deref(), Some("Diamond Member"));
    assert_eq!(r.years_on_platform, Some(2));
    assert_eq!(r.rating, Some(5.0));
    assert_eq!(r.review_count, Some(2));
    assert_eq!(
        r.products_offered,
        vec!["Tapered Roller Bearing", "Spherical Roller Bearing"]
    );
    assert_eq!(
        r.business_types,
        vec!["Manufacturer/Factory & Trading Company"]
    );
}

#[test]
fn product_and_supplier_page_belong_to_one_company() {
    let scraper = get_scraper_for_platform("madeinchina").unwrap();
    assert_eq!(
        scraper
            .extract_company_profile_url(&product_page())
            .as_deref(),
        Some("https://yongjie-bearing.en.made-in-china.com")
    );
    assert_eq!(
        scraper.extract_company_profile_url(&supplier_page()),
        None,
        "the supplier page already is the company page"
    );
    let key = scraper.company_key(&product_page());
    assert_eq!(key.as_deref(), Some("yongjie-bearing.en.made-in-china.com"));
    assert_eq!(scraper.company_key(&supplier_page()), key);
}

#[test]
fn the_supplier_page_adds_region_contact_and_products() {
    let scraper = get_scraper_for_platform("madeinchina").unwrap();
    let supplier = scraper.parse_supplier(&product_page(), PRODUCT_URL);
    let s = scraper.enrich_from_company_profile(supplier, &supplier_page());
    assert_eq!(s.country.as_deref(), Some("Liaoning, China"));
    assert_eq!(s.contact_name.as_deref(), Some("Mrs. Ann"));
    assert_eq!(s.year_established.as_deref(), Some("2021"));
    assert_eq!(s.employee_count.as_deref(), Some("15"));
    assert_eq!(
        s.badge_honorific.as_deref(),
        Some("Audited on site by TÜV Rheinland (report MIC-ASI254819)"),
        "the more exact audit note is kept"
    );
    let d = s.company_description.unwrap();
    assert_eq!(d.matches("Address: ").count(), 1, "no line twice");
    assert!(d.contains("\nRegion shown by Made-in-China: Liaoning, China."));
    assert!(d.contains("\nYear of establishment: 2021-03-02."));
    assert!(d.contains(
        "\nProducts on its Made-in-China page: Low Price Spherical Roller Bearing (US$0.01-10.00); NTN NSK Koyo Needle Roller Bearing (US$1.90-25.00)."
    ));
    assert!(!d.contains("Another supplier"));

    let r = scraper
        .enrich_record_from_company_profile(
            scraper.supplier_record(&product_page()),
            &supplier_page(),
        )
        .unwrap();
    assert_eq!(
        r.years_on_platform,
        Some(2),
        "the product page's figures are kept"
    );
    assert!(
        r.products_offered
            .contains(&"NTN NSK Koyo Needle Roller Bearing (US$1.90-25.00)".to_string())
    );
}

#[test]
fn a_supplier_page_scan_reads_the_company_but_no_product() {
    let scraper = get_scraper_for_platform("madeinchina").unwrap();
    let l = scraper.parse_listing(&supplier_page(), SHOP_URL);
    assert_eq!(l.title, None, "no single product");
    assert_eq!(l.unit_price, None);
    assert_eq!(
        l.payment_type.as_deref(),
        Some("LC, T/T, Western Union"),
        "the company's payment terms are still checked"
    );
    let s = scraper.parse_supplier(&supplier_page(), SHOP_URL);
    assert_eq!(
        s.company_name.as_deref(),
        Some("Changzhou Nanyi Bearing Co., Ltd.")
    );
    assert_eq!(s.badge_honorific.as_deref(), Some("Audited on site"));
    assert_eq!(s.contact_name.as_deref(), Some("Mrs. Ann"));

    let s = scraper.parse_supplier(&supplier_page_without_heading(), SHOP_URL);
    assert_eq!(
        s.company_name.as_deref(),
        Some("Shanghai Angtong Information Technology Co., Ltd.")
    );
    assert_eq!(s.country.as_deref(), Some("Beijing, China"));
    assert_eq!(s.badge_honorific, None);
}

#[test]
fn an_older_product_page_still_gives_name_price_and_shop() {
    let scraper = get_scraper_for_platform("madeinchina").unwrap();
    let html = old_product_page();
    let s = scraper.parse_supplier(&html, "u");
    assert_eq!(s.company_name.as_deref(), Some("XCMG E-Commerce Inc."));
    assert_eq!(s.country.as_deref(), Some("Jiangsu, China"));
    assert_eq!(s.badge_honorific.as_deref(), Some("Audited on site"));
    let l = scraper.parse_listing(&html, "u");
    assert_eq!(l.unit_price.as_deref(), Some("US$10,000.00-1,000,000.00"));
    assert_eq!(l.minimum_order_quantity.as_deref(), Some("1 Unit"));
    assert_eq!(l.reference.as_deref(), Some("HB43K"));
    assert_eq!(
        scraper.company_key(&html).as_deref(),
        Some("machmall.en.made-in-china.com")
    );
}

#[test]
fn tiered_prices_and_a_main_site_product_page() {
    let scraper = get_scraper_for_platform("madeinchina").unwrap();
    let html = main_site_product_page();
    let l = scraper.parse_listing(&html, "u");
    assert_eq!(
        l.unit_price.as_deref(),
        Some("US$901.00 (1-9 Pieces); US$896.00 (10-49 Pieces)"),
        "repeated slides are read once"
    );
    assert_eq!(l.minimum_order_quantity.as_deref(), Some("1 Pieces"));
    assert!(l.payment_type.unwrap().contains("Money Gram"));
    // The shop comes from the company data, not the main-site address.
    assert_eq!(
        scraper.company_key(&html).as_deref(),
        Some("angtongtech.en.made-in-china.com")
    );
    let s = scraper.parse_supplier(&html, "u");
    assert!(!s.platform_verified_badge);
    assert_eq!(s.badge_honorific, None);
}

#[tokio::test]
async fn product_and_supplier_pages_from_the_browser_make_one_result() {
    unsafe {
        std::env::set_var("SCRAPERAPI_KEY", "k");
    }
    let page = b2b_page_from_browser_first(
        "madeinchina",
        PRODUCT_URL,
        Some(&product_page()),
        Some(&supplier_page()),
        None,
    )
    .await
    .expect("read from the browser");
    assert_eq!(
        page.company_key.as_deref(),
        Some("yongjie-bearing.en.made-in-china.com")
    );
    assert!(!page.company_page_missing);
    assert_eq!(page.supplier.contact_name.as_deref(), Some("Mrs. Ann"));
    assert_eq!(page.supplier.country.as_deref(), Some("Liaoning, China"));
    assert!(page.listing.title.is_some());

    // A supplier page needs nothing more.
    let page =
        b2b_page_from_browser_first("madeinchina", SHOP_URL, Some(&supplier_page()), None, None)
            .await
            .expect("read from the browser");
    assert_eq!(page.listing.title, None);
    assert!(!page.company_page_missing);
}

#[test]
fn payment_written_only_in_the_faq_is_still_read() {
    let scraper = get_scraper_for_platform("madeinchina").unwrap();
    // Older page: no "Terms of Payment" box, the FAQ is in the product text.
    let html = page(
        r#"<link rel="canonical" href="https://machmall.en.made-in-china.com/product/FbPQmuwraRlL/x.html" />"#,
        r#"<h1>XCMG Concrete Pump</h1>
        <div class="rich-text cf"><p><span><strong>1: What kind terms of payment can be accepted?</strong><br/>
        A: For terms of payment, L/C, T/T, D/A, D/P, Western Union (can be) could accepted.<br/><br/>
        <strong>2: What about the delivery time?</strong><br/>A: 7-30 days after receiving the deposit.</span></p></div>
        <script type="application/ld+json">{"@type":"Product","name":"XCMG Concrete Pump"}</script>"#,
    );
    let l = scraper.parse_listing(&html, "u");
    assert_eq!(
        l.payment_type.as_deref(),
        Some(
            "Payment, in the supplier's own words: A: For terms of payment, L/C, T/T, D/A, D/P, Western Union (can be) could accepted."
        )
    );
    assert!(
        l.description.unwrap().contains(
            "Supplier's own product text: 1: What kind terms of payment can be accepted?"
        )
    );

    // Newer page: the FAQ is in the page data, next to the payment box.
    let html = page(
        r#"<link rel="canonical" href="https://superfasty.en.made-in-china.com/product/yFLftrTbnOGZ/x.html" />"#,
        r#"<div class="sts">Secured Trading Service</div>
        <dl class="sr-comProfile-infos"><dt class="sr-comProfile-label">Terms of Payment</dt><dd class="sr-comProfile-fields">T/T, PayPal</dd></dl>
        <script type="application/ld+json">{"@type":"Product","name":"Motorcycle Helmet"}</script>
        <script type="application/ld+json">{"@type":"FAQPage","mainEntity":[
          {"@type":"Question","name":"What&apos;s your minimum order quantity (MOQ)?","acceptedAnswer":{"@type":"Answer","text":"300 pieces."}},
          {"@type":"Question","name":"What are your payment terms?","acceptedAnswer":{"@type":"Answer","text":"T/T (30% deposit, 70% balance before shipment), and Western Union for sample orders."}}]}</script>"#,
    );
    let l = scraper.parse_listing(&html, "u");
    assert_eq!(
        l.payment_type.as_deref(),
        Some(
            "T/T, PayPal; Made-in-China Secured Trading Service (escrow); Payment, in the supplier's own words: T/T (30% deposit, 70% balance before shipment), and Western Union for sample orders."
        )
    );
    assert!(l.description.unwrap().contains(
        "Supplier's FAQ: What's your minimum order quantity (MOQ)? 300 pieces. | What are your payment terms?"
    ));
}

#[tokio::test]
async fn a_main_site_product_page_reads_the_supplier_page_from_the_browser() {
    unsafe {
        std::env::set_var("SCRAPERAPI_KEY", "k");
    }
    let page = b2b_page_from_browser_first(
        "madeinchina",
        "https://www.made-in-china.com/product/Elitebook650-G10-Business-Laptop-UJrpmMjbvoRP.html",
        Some(&main_site_product_page()),
        Some(&supplier_page_without_heading()),
        None,
    )
    .await
    .expect("read from the browser");
    assert!(
        !page.company_page_missing,
        "the supplier page from the browser is used"
    );
    assert_eq!(
        page.company_key.as_deref(),
        Some("angtongtech.en.made-in-china.com")
    );
    assert_eq!(page.supplier.country.as_deref(), Some("Beijing, China"));
}
