use backend::services::b2b_scrapers::{b2b_page_from_browser_first, get_scraper_for_platform};

// Small pages built from the real Global Sources pages (October 2026):
// the same elements and classes, with most of the text left out.

const PRODUCT_URL: &str = "https://www.globalsources.com/TWS-earbud/tws-earbud-1231057850p.htm";
const SUPPLIER_HOST: &str = "boxinltd.manufacturer.globalsources.com";

/// Pads a page past the "too small to be a real page" check.
fn page(head: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html><html><head>{head}</head><body>{body}<div>{}</div></body></html>",
        "x".repeat(2500)
    )
}

/// The supplier card on a product page. `badges` is the badge pictures.
fn supplier_card(badges: &str, trade_shows: &str) -> String {
    format!(
        r#"<div class="company-info"><div class="max-row supplier-name">
          <a target="_blank" href="//boxinltd.manufacturer.globalsources.com/homepage_6008846289828.htm" title="Huizhou Boxin Electronics Co., Ltd"><div class="name"> Huizhou Boxin Electronics Co., Ltd </div></a></div>
          <div class="vr-img" alt="Huizhou Boxin Electronics Co., Ltd"></div>
          <div class="gs-tag-group new-tag">{badges}
            <p class="gs-tag years"><em class="year-bg"><i class="num">7</i> <span class="suffix">yrs</span></em></p>
            <em class="gs-tag dot">China</em></div>
          <div class="response item">Response Rate: <span class="type"> High</span></div>
          {trade_shows}
          <div class="Business-Type"><div class="tit">Business Type</div> <div><span class="businessTypeName"> Manufacturer<img src="verify.svg" alt="" class="businessTypeImg"></span></div></div>
        </div>"#
    )
}

fn product_page_with(card: &str, own_text: &str) -> String {
    page(
        r#"<title>TWS earbud | Global Sources</title>
        <script data-n-head="ssr" type="application/ld+json">{"@context":"http://schema.org/","@type":"Product","name":"TWS earbud Bluetooth 5.3 Low Latency TWS Earbuds","image":["https://p.globalsources.com/IMAGES/PDT/B1231057850/tws-earbud.jpg?ver=6116953297"],"description":"Are you looking for top-notch, guaranteed-quality tws earbud products from China?","manufacturer":"Huizhou Boxin Electronics Co., Ltd","offers":{"@type":"AggregateOffer","lowPrice":"12.8","highPrice":"13.6","priceCurrency":"USD"}}</script>"#,
        &format!(
            r#"<h1>TWS earbud Bluetooth 5.3</h1>
        <img data-src="https://p.globalsources.com/IMAGES/PDT/S1231057850/tws-earbud-2.jpg?ver=1" src="blank.gif">
        <div class="price-range"><div class="item"><div class="num">US$ 13.6</div> <div class="pieces"> 3000–4999 Pieces </div></div><div class="item"><div class="num">US$ 12.8</div> <div class="pieces"> ≥50000 Pieces </div></div></div>
        <div class="productAttributes-item"><div class="productAttributes-item-left"> Bluetooth Version </div> <div class="productAttributes-item-right"><div class="txt"><span>5.3</span></div></div></div>
        <table><tbody><tr><th class="ant-descriptions-item-label">Model Number</th><td class="ant-descriptions-item-content">TW065C</td>
          <th class="ant-descriptions-item-label">Brand Name</th><td class="ant-descriptions-item-content">JINERLE / can be OEM brand</td></tr>
          <tr><th class="ant-descriptions-item-label">FOB Port</th><td class="ant-descriptions-item-content">Shenzhen</td>
          <th class="ant-descriptions-item-label">Lead Time</th><td class="ant-descriptions-item-content">30 days</td></tr>
          <tr><th class="ant-descriptions-item-label">Units per Export Carton</th><td class="ant-descriptions-item-content">100</td></tr>
          <tr><th class="ant-descriptions-item-label">Payment Method</th><td class="ant-descriptions-item-content">Telegraphic Transfer (TT,T/T)</td></tr></tbody></table>
        <div class="tpl_txt_editor">{own_text}</div>
        {card}
        <script>var tracking = "Other Trading Co., Ltd.";</script>"#
        ),
    )
}

/// A product from a Verified Manufacturer that exhibits at trade shows.
fn product_page() -> String {
    product_page_with(
        &supplier_card(
            r#"<img alt="Exhibitor" class="gs-tag starImg"> <img alt="O2O Supported" class="gs-tag o2o">
            <div class="gs-tag vImg vm"><img alt="Verified Maufacturer"></div>"#,
            r#"<div class="tradeshow"><div class="tit">Trade Shows</div> <div class="shows"><div> We exhibited at <span class="bold">7</span> Global Sources Trade Shows. </div></div></div>"#,
        ),
        r#"<p><strong>Specification:</strong></p><p>1. Pair with your phone.</p>
        <p>Payment</p><p>1: we accept T/T and Paypal.</p><p>2: 30% deposit, 70% balance before the shipment.</p><p>Shipping</p><p>Sample in stock: 1-3 days</p>"#,
    )
}

/// The same supplier's own page.
fn supplier_page() -> String {
    page(
        r#"<title>Huizhou Boxin Electronics Co., Ltd - Global Sources</title>
        <script data-n-head="ssr" type="application/ld+json">{"@context":"http://schema.org/","@type":"Organization","name":"Huizhou Boxin Electronics Co., Ltd","foundingDate":"2005","url":"https://boxinltd.manufacturer.globalsources.com/homepage_6008846289828.htm","logo":{"contentUrl":"https://s.globalsources.com/IMAGES/SPL/LOGO/828/L8846289828.jpg","@type":"ImageObject"},"slogan":"BSCI-certified headphone manufacturer with 30+ years of experience"}</script>"#,
        r#"<div class="item-info"><div class="item-col"><p class="item-d-title">Ranking:</p> <div class="gs-tag-group"><img alt="Exhibitor"></div></div>
          <div class="item-col"><p class="item-d-title">Supplier Assessment:</p> <div class="item-content flex"><a class="gs-tag vs-Img vm"><img alt="Verified Maufacturer"></a></div></div>
          <div class="item-col"><p class="item-d-title">Country / Region:</p> <div class="item-content">China</div></div>
          <div class="item-col"><p class="item-d-title">Export Markets:</p> <div class="item-content">Asia , Europe</div></div></div>
        <div class="overview-list"><h2 class="overview-title"><span>BSCI-certified headphone manufacturer</span></h2> <h3 class="sub-title">10,000sqm factory | 6 production lines</h3> <div class="res"><span> Avg Response time: </span> 24–48 h</div></div>
        <span class="txt-content"><p><strong>Operating Since 2005</strong><br />Huizhou Boxin Electronics Co., Ltd. is a designer and manufacturer of headphones. Its molds come from Shenzhen Partner Plastics Co., Ltd.</p></span>"#,
    )
}

#[test]
fn global_sources_is_a_b2b_platform() {
    assert!(get_scraper_for_platform("globalsources").is_some());
}

#[test]
fn a_product_page_gives_the_product() {
    let scraper = get_scraper_for_platform("globalsources").unwrap();
    let l = scraper.parse_listing(&product_page(), PRODUCT_URL);
    assert_eq!(
        l.title.as_deref(),
        Some("TWS earbud Bluetooth 5.3 Low Latency TWS Earbuds")
    );
    assert_eq!(
        l.unit_price.as_deref(),
        Some("US$ 13.6 (3000–4999 Pieces); US$ 12.8 (≥50000 Pieces)")
    );
    assert_eq!(l.minimum_order_quantity.as_deref(), Some("3000 Pieces"));
    assert_eq!(l.preferred_port.as_deref(), Some("Shenzhen"));
    assert_eq!(l.incoterms.as_deref(), Some("FOB Shenzhen"));
    assert_eq!(l.delivery_timeframe.as_deref(), Some("30 days"));
    assert_eq!(l.reference.as_deref(), Some("TW065C"));
    assert_eq!(
        l.packaging_details.as_deref(),
        Some("Units per Export Carton: 100")
    );
    assert_eq!(
        l.image_urls.len(),
        2,
        "page data plus the lazy gallery picture"
    );
    let pay = l.payment_type.unwrap();
    assert!(
        pay.starts_with("Telegraphic Transfer (TT,T/T); Payment, in the supplier's own words: ")
    );
    assert!(pay.contains("30% deposit, 70% balance before the shipment."));
    let d = l.description.unwrap();
    assert!(d.contains("Bluetooth Version: 5.3"));
    assert!(d.contains("Brand Name: JINERLE / can be OEM brand"));
    assert!(
        !d.contains("top-notch"),
        "the Global Sources template text is not used"
    );
    assert!(!d.contains("tracking"), "scripts are never read");
    assert_eq!(l.source_platform, "globalsources");
}

#[test]
fn a_product_page_gives_the_supplier_and_its_checks() {
    let scraper = get_scraper_for_platform("globalsources").unwrap();
    let s = scraper.parse_supplier(&product_page(), PRODUCT_URL);
    assert_eq!(
        s.company_name.as_deref(),
        Some("Huizhou Boxin Electronics Co., Ltd")
    );
    assert_eq!(s.country.as_deref(), Some("China"));
    assert!(s.platform_verified_badge);
    assert_eq!(s.badge_honorific.as_deref(), Some("Verified Manufacturer"));
    assert_eq!(s.contact_phone, None, "the inquiry form is not a contact");
    let d = s.company_description.unwrap();
    assert!(d.contains("Business type: Manufacturer."));
    assert!(d.contains("7 years with Global Sources"));
    assert!(d.contains("exhibited at 7 Global Sources trade shows"));

    let r = scraper.supplier_record(&product_page()).unwrap();
    assert_eq!(r.platform, "Global Sources");
    assert!(r.checked_by_platform);
    assert_eq!(r.years_on_platform, Some(7));
    assert_eq!(
        r.member_label, None,
        "\"Exhibitor\" is not a membership level"
    );
    assert_eq!(r.business_types, vec!["Manufacturer"]);
}

#[test]
fn product_and_supplier_page_belong_to_one_company() {
    let scraper = get_scraper_for_platform("globalsources").unwrap();
    assert_eq!(
        scraper
            .extract_company_profile_url(&product_page())
            .as_deref(),
        Some("https://boxinltd.manufacturer.globalsources.com/homepage_6008846289828.htm")
    );
    assert_eq!(
        scraper.company_key(&product_page()).as_deref(),
        Some(SUPPLIER_HOST)
    );
    assert_eq!(
        scraper.company_key(&supplier_page()).as_deref(),
        Some(SUPPLIER_HOST)
    );
    assert_eq!(scraper.extract_company_profile_url(&supplier_page()), None);
}

#[test]
fn the_supplier_page_adds_founding_year_markets_and_profile() {
    let scraper = get_scraper_for_platform("globalsources").unwrap();
    let s = scraper.parse_supplier(&product_page(), PRODUCT_URL);
    let s = scraper.enrich_from_company_profile(s, &supplier_page());
    assert_eq!(s.year_established.as_deref(), Some("2005"));
    assert_eq!(s.badge_honorific.as_deref(), Some("Verified Manufacturer"));
    assert!(s.logo_url.is_some());
    let d = s.company_description.unwrap();
    assert_eq!(d.matches("Country: ").count(), 1, "no line twice");
    assert!(d.contains("\nFounded: 2005."));
    assert!(d.contains("\nExport markets: Asia, Europe."));
    assert!(d.contains("10,000sqm factory | 6 production lines"));
    assert!(!d.contains("Avg Response"));
    assert!(d.contains(
        "The company profile also names other companies: Shenzhen Partner Plastics Co., Ltd. Ask how they are related."
    ));
    assert!(d.contains("Operating Since 2005"));
}

#[test]
fn a_supplier_page_has_no_product() {
    let scraper = get_scraper_for_platform("globalsources").unwrap();
    let l = scraper.parse_listing(&supplier_page(), "u");
    assert_eq!(l.title, None);
    assert_eq!(l.unit_price, None);
}

#[test]
fn trade_show_only_and_unchecked_suppliers() {
    let scraper = get_scraper_for_platform("globalsources").unwrap();
    let show_only = product_page_with(
        &supplier_card(r#"<img alt="O2O Supported" class="gs-tag o2o">"#, ""),
        "",
    );
    let s = scraper.parse_supplier(&show_only, PRODUCT_URL);
    assert!(!s.platform_verified_badge);
    assert_eq!(s.badge_honorific.as_deref(), Some("Trade show exhibitor"));

    let nothing = product_page_with(&supplier_card(r#"<img alt="Super Supplier">"#, ""), "");
    let s = scraper.parse_supplier(&nothing, PRODUCT_URL);
    assert!(!s.platform_verified_badge);
    assert_eq!(s.badge_honorific, None);
    let r = scraper.supplier_record(&nothing).unwrap();
    assert!(!r.checked_by_platform);
    assert_eq!(r.member_label.as_deref(), Some("Super Supplier"));
}

#[tokio::test]
async fn product_and_supplier_pages_from_the_browser_make_one_result() {
    unsafe {
        std::env::set_var("SCRAPERAPI_KEY", "k");
    }
    let page = b2b_page_from_browser_first(
        "globalsources",
        PRODUCT_URL,
        Some(&product_page()),
        Some(&supplier_page()),
    )
    .await
    .expect("read from the browser");
    assert_eq!(page.company_key.as_deref(), Some(SUPPLIER_HOST));
    assert!(!page.company_page_missing);
    assert_eq!(page.supplier.year_established.as_deref(), Some("2005"));
    assert!(page.listing.title.is_some());
}

#[test]
fn the_profile_is_added_even_when_the_platform_line_is_long() {
    // Verified + "Super Supplier" + trade shows make a long "On Global
    // Sources" line; the profile must still be added (once).
    let scraper = get_scraper_for_platform("globalsources").unwrap();
    let page = product_page_with(
        &supplier_card(
            r#"<img alt="Super Supplier"> <img alt="O2O Supported">
            <div class="gs-tag vImg vm"><img alt="Verified Supplier"></div>"#,
            r#"<div class="tradeshow"><div> We exhibited at <span class="bold">6</span> Global Sources Trade Shows. </div></div>"#,
        ),
        "",
    );
    let s = scraper.parse_supplier(&page, PRODUCT_URL);
    assert!(
        s.company_description
            .as_deref()
            .unwrap()
            .lines()
            .any(|l| l.chars().count() > 200),
        "the platform line is long"
    );
    let s = scraper.enrich_from_company_profile(s, &supplier_page());
    let s = scraper.enrich_from_company_profile(s, &supplier_page());
    let d = s.company_description.unwrap();
    assert_eq!(d.matches("Operating Since 2005").count(), 1);
}

/// The supplier's contact data block, as the extension keeps it.
const CONTACT_DATA: &str = r#"<script type="text/x-safely-data">contactUsInfo:{companyName:M,address:"Rm 701, Building 5, Huicheng District,Huizhou,Guangdong,China",telephone:a,fax:a,mobile:a,homePageAddress:"https://www.globalsources.com/boxinltd.co",othHomePageAddress:"https://boxinheadset.com/",registerCompanyName:"惠州市博新电子有限公司",registerNum:"91441303594089865C",registerAddress:"惠州市惠城区水口街道",lastUpdateDate:1792976741000}</script>"#;

#[test]
fn the_page_data_gives_the_website_address_and_registration() {
    let scraper = get_scraper_for_platform("globalsources").unwrap();
    let page = product_page().replace("</body>", &format!("{CONTACT_DATA}</body>"));
    let s = scraper.parse_supplier(&page, PRODUCT_URL);
    assert_eq!(s.website_url.as_deref(), Some("https://boxinheadset.com/"));
    assert_eq!(s.contact_phone, None, "no phone given");
    let d = s.company_description.unwrap();
    assert!(d.contains(
        "\nAddress given to Global Sources: Rm 701, Building 5, Huicheng District,Huizhou,Guangdong,China."
    ));
    assert!(d.contains(
        "Official registration in China (shown by Global Sources): name 惠州市博新电子有限公司; number 91441303594089865C; address 惠州市惠城区水口街道."
    ));
}

#[test]
fn a_global_sources_page_is_never_the_suppliers_website_and_hidden_numbers_are_marked() {
    let scraper = get_scraper_for_platform("globalsources").unwrap();
    let data = r#"<script>contactUsInfo:{othHomePageAddress:"https://www.globalsources.com/yueyinda.co",registerNum:"91440605MACJQBGXXX"}</script>"#;
    let page = product_page().replace("</body>", &format!("{data}</body>"));
    let s = scraper.parse_supplier(&page, PRODUCT_URL);
    assert_eq!(s.website_url, None);
    assert!(
        s.company_description
            .unwrap()
            .contains("number 91440605MACJQBGXXX (partly hidden by Global Sources)")
    );
}

#[test]
fn the_supplier_page_data_fills_a_missing_website() {
    let scraper = get_scraper_for_platform("globalsources").unwrap();
    let s = scraper.parse_supplier(&product_page(), PRODUCT_URL);
    assert_eq!(s.website_url, None);
    let supplier = supplier_page().replace("</body>", &format!("{CONTACT_DATA}</body>"));
    let s = scraper.enrich_from_company_profile(s, &supplier);
    assert_eq!(s.website_url.as_deref(), Some("https://boxinheadset.com/"));
    assert_eq!(
        s.company_description
            .unwrap()
            .matches("Official registration")
            .count(),
        1
    );
}
