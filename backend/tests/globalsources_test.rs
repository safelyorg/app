use backend::services::sources::b2b::{b2b_page_from_browser_first, get_scraper_for_platform};

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

/// The same supplier's company profile page (company-profile_{id}.htm),
/// trimmed from the real Shenzhen N+1 page (October 2026): label / value
/// tables, as the extension sends them (no scripts).
fn profile_page() -> String {
    let row = |label: &str, value: &str| {
        format!(
            r#"<div class="table-box"><div class="table-item"><div class="table-label">{label}</div> <div class="table-value-long last-value"><span>{value}</span></div></div></div>"#
        )
    };
    let pair = |l1: &str, v1: &str, l2: &str, v2: &str| {
        format!(
            r#"<div class="table-box"><div class="table-item"><div class="table-label">{l1}</div> <div class="table-value"><span>{v1}</span></div> <div class="table-label">{l2}</div> <div class="table-value last-value"><span>{v2}</span></div></div></div>"#
        )
    };
    page(
        "<title>Company Overview of China Manufacturer - Huizhou Boxin Electronics Co., Ltd</title>",
        &[
            "<h3 class=\"article-title\">Basic Information</h3><div class=\"company-table\">".to_string(),
            row("Total Capitalization", "US$400,000 to 499,999"),
            pair("Year Established", "2005", "Total Employees", "200 to 249"),
            pair("Company Certificate", "NA", "Product Certificate", "ROHS, FCC"),
            "</div><h3 class=\"article-title\">Trading Capabilities</h3><div class=\"company-table\">".to_string(),
            pair("Total Annual Sales", "US$40,000,000 to 44,999,999", "Export Percentage", "100 percent"),
            pair("Major Customer", "Secrecy", "Small Orders Accepted", "Yes"),
            "</div><h3>Factory Information</h3><div class=\"company-table\">".to_string(),
            pair("Factory Size", "3000 Square Metre", "No. of Production Lines", "8"),
            pair("Number of Production Staff", "200 to 249", "Number of QC Staff", "30 to 39"),
            pair("Year Established", "2010", "Number of R &amp; D Staff", "10 to 19"),
            row("Address", "F5 No. 61 Liuxian 2nd Rd., Baoan District, Shenzhen, Guangdong, China"),
            row("Contact Person", "Ms.Yu, Esther"),
            row("Job Title", "Sales Manager"),
            row("Hotline", "(86-755) 2757 3322"),
            row("Customer Service e-mail", "Service@boxin-example.com"),
            "</div>".to_string(),
        ]
        .concat(),
    )
}

const PROFILE_URL: &str =
    "https://boxinltd.manufacturer.globalsources.com/company-profile_6008846289828.htm";

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
        Some(&profile_page()),
    )
    .await
    .expect("read from the browser");
    assert_eq!(page.company_key.as_deref(), Some(SUPPLIER_HOST));
    assert!(!page.company_page_missing);
    assert_eq!(page.supplier.year_established.as_deref(), Some("2005"));
    assert_eq!(page.supplier.employee_count.as_deref(), Some("200 to 249"));
    assert_eq!(page.supplier.contact_name.as_deref(), Some("Ms.Yu, Esther"));
    assert!(page.listing.title.is_some());
}

#[test]
fn the_company_profile_page_is_the_second_company_page() {
    let scraper = get_scraper_for_platform("globalsources").unwrap();
    let company = scraper
        .extract_company_profile_url(&product_page())
        .unwrap();
    assert_eq!(
        scraper.build_extended_profile_url(&company).as_deref(),
        Some(PROFILE_URL)
    );
    // Trade-show suppliers have their exhibitor page instead.
    let exhibitor = scraper
        .extract_company_profile_url(&show_product(true))
        .unwrap();
    assert_eq!(scraper.build_extended_profile_url(&exhibitor), None);
}

#[test]
fn the_company_profile_page_gives_figures_contact_and_factory() {
    let scraper = get_scraper_for_platform("globalsources").unwrap();
    let s = scraper.parse_supplier(&product_page(), PRODUCT_URL);
    let s = scraper.enrich_from_company_profile(s, &supplier_page());
    let s = scraper.enrich_from_extended_profile(s, &profile_page());
    assert_eq!(s.year_established.as_deref(), Some("2005"), "kept");
    assert_eq!(s.employee_count.as_deref(), Some("200 to 249"));
    assert_eq!(
        s.sales_revenue.as_deref(),
        Some("US$40,000,000 to 44,999,999")
    );
    assert_eq!(s.export_percentage.as_deref(), Some("100 percent"));
    assert_eq!(s.contact_name.as_deref(), Some("Ms.Yu, Esther"));
    assert_eq!(s.contact_phone.as_deref(), Some("(86-755) 2757 3322"));
    let d = s.company_description.unwrap();
    assert!(d.contains("\nContact person's role: Sales Manager."));
    assert!(d.contains("\nEmail given on its Global Sources profile: Service@boxin-example.com."));
    assert!(d.contains(
        "\nCompany profile on Global Sources: Total capitalization: US$400,000 to 499,999; Factory size: 3000 Square Metre; Production lines: 8; Production staff: 200 to 249; Quality control staff: 30 to 39; R&D staff: 10 to 19."
    ));
    assert!(d.contains("\nFactory address: F5 No. 61 Liuxian 2nd Rd."));
    assert!(!d.contains("Secrecy"), "placeholders are left out");
    // Read twice: no line twice.
    let s2 = scraper.enrich_from_extended_profile(
        scraper.enrich_from_extended_profile(
            scraper.parse_supplier(&product_page(), PRODUCT_URL),
            &profile_page(),
        ),
        &profile_page(),
    );
    assert_eq!(
        s2.company_description
            .unwrap()
            .matches("Factory address:")
            .count(),
        1
    );
}

#[test]
fn a_profile_page_without_values_changes_nothing() {
    let scraper = get_scraper_for_platform("globalsources").unwrap();
    let before = scraper.parse_supplier(&product_page(), PRODUCT_URL);
    let empty = page("<title>x</title>", "<div class=\"company-table\"></div>");
    let after = scraper
        .enrich_from_extended_profile(scraper.parse_supplier(&product_page(), PRODUCT_URL), &empty);
    assert_eq!(after.company_description, before.company_description);
    assert_eq!(after.employee_count, None);
    assert_eq!(after.contact_phone, None);
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

// ---- Hong Kong trade-show pages ----------------------------------------

/// Nuxt's page data format: a flat list where each object field and
/// list item is the number of another entry.
fn nuxt_data(value: &serde_json::Value) -> String {
    use serde_json::Value;
    fn add(items: &mut Vec<Value>, v: &Value) -> usize {
        let at = items.len();
        items.push(Value::Null);
        let item = match v {
            Value::Object(fields) => {
                let mut out = serde_json::Map::new();
                for (k, child) in fields {
                    let i = add(items, child);
                    out.insert(k.clone(), Value::from(i));
                }
                Value::Object(out)
            }
            Value::Array(list) => Value::Array(
                list.iter()
                    .map(|child| Value::from(add(items, child)))
                    .collect(),
            ),
            other => other.clone(),
        };
        items[at] = item;
        at
    }
    let mut items = vec![serde_json::json!(["ShallowReactive", 1])];
    add(&mut items, &serde_json::json!({ "data": value }));
    serde_json::to_string(&items).unwrap()
}

fn show_page(data: serde_json::Value) -> String {
    page(
        "<title>Global Sources Hong Kong Shows</title>",
        &format!(
            r#"<a href="https://www.globalsources.com/products/HK/">Product List</a>
            <script type="application/json" data-nuxt-data="nuxt-app" id="__NUXT_DATA__">{}</script>"#,
            nuxt_data(&data)
        ),
    )
}

fn dujia(checked: bool) -> serde_json::Value {
    serde_json::json!({
        "supplierId": 2008801135123u64,
        "companyName": "Anhui Dujia Office Supplies Manufacturing Co.,Ltd",
        "country": "China",
        "logoImageUrl": "https://s.globalsources.com/IMAGES/SPL/LOGO/123/L8801135123.jpg",
        "orgDesc": "<p><strong>Years of experience</strong><br/>Anhui Dujia Office Supplies Manufacturing Co.,Ltd., established in 2021, is the new production base of Cixi Dujia.</p>",
        "obmFlag": true, "odmFlag": true, "oemFlag": true,
        "businessTypes": ["Exporter", "Manufacturer"],
        "tradeShowList": [{"tsName": "Global Sources Consumer Electronics", "boothNum": "10T37",
            "showVenue": "AsiaWorld-Expo, Hong Kong", "hall": "10", "showDate": "11-14 October, 2026"}],
        "categoryNames": ["Computer footrests"],
        "businessTypeList": [
            {"businessType": "Exporter", "verifiedFlag": checked, "vendorName": "Addcredit"},
            {"businessType": "Manufacturer", "verifiedFlag": checked, "vendorName": "Addcredit"}
        ]
    })
}

const SHOW_PRODUCT_URL: &str = "https://www.globalsources.com/products/HK/angle-rock-0-40-degrees-and-2-heights-adjustable_1213293180/";

fn show_product(checked: bool) -> String {
    show_page(serde_json::json!({
        "exhibitorProductDetail": {"code": 200, "data": {
            "productInfo": {
                "productId": 1213293180u64, "supplierId": 2008801135123u64,
                "productName": "Angle rock 0-40 degrees and 2 heights adjustable",
                "modelNumber": "F8039", "brandName": "", "smallOrderAcceptFlag": true,
                "productOutline": "<p>Angle adjustable footrest</p><p>Ergonomically designed.</p>",
                "productPrimaryImage": {"largeImageUrl": "https://p.globalsources.com/IMAGES/PDT/L1213293180/a.jpg"},
                "productAddlImageList": []
            },
            "supplierInfo": dujia(checked)
        }}
    }))
}

fn show_exhibitor() -> String {
    show_page(serde_json::json!({
        "ExhibitorDetail": [
            {"status": "fulfilled", "value": {"code": 200, "data": dujia(true)}},
            {"status": "fulfilled", "value": {"code": 200, "data": [
                {"productId": 1213293193u64, "productFOBMin": "5.00", "productFOBMax": "5.00",
                 "minOrderQuantity": 1000, "minOrderUnit": "Pieces"},
                {"productId": 1213293180u64, "productFOBMin": "3.90", "productFOBMax": "3.90",
                 "minOrderQuantity": 1000, "minOrderUnit": "Pieces"}
            ]}},
            {"status": "fulfilled", "value": null}
        ]
    }))
}

#[test]
fn a_trade_show_product_page_gives_the_product_and_the_supplier() {
    let scraper = get_scraper_for_platform("globalsources").unwrap();
    let l = scraper.parse_listing(&show_product(true), SHOW_PRODUCT_URL);
    assert_eq!(
        l.title.as_deref(),
        Some("Angle rock 0-40 degrees and 2 heights adjustable")
    );
    assert_eq!(l.reference.as_deref(), Some("F8039"));
    assert_eq!(l.image_urls.len(), 1);
    assert_eq!(l.unit_price, None, "the price is on the exhibitor page");
    let d = l.description.unwrap();
    assert!(d.contains("Model Number: F8039; Small Orders: Accepted."));
    assert!(d.contains("Angle adjustable footrest Ergonomically designed."));

    let s = scraper.parse_supplier(&show_product(true), SHOW_PRODUCT_URL);
    assert_eq!(
        s.company_name.as_deref(),
        Some("Anhui Dujia Office Supplies Manufacturing Co.,Ltd")
    );
    assert_eq!(s.year_established.as_deref(), Some("2021"));
    assert_eq!(s.badge_honorific.as_deref(), Some("Verified Manufacturer"));
    assert!(s.platform_verified_badge);
    assert_eq!(s.website_url, None);
    let d = s.company_description.unwrap();
    assert!(d.contains(
        "Business type: Exporter, Manufacturer (each checked by Addcredit, an outside company used by Global Sources)."
    ));
    assert!(d.contains("Hall 10, Booth 10T37, 11-14 October, 2026"));
    assert!(d.contains("Services: OEM, ODM, OBM."));
}

#[test]
fn the_exhibitor_page_is_found_and_gives_the_price() {
    let scraper = get_scraper_for_platform("globalsources").unwrap();
    assert_eq!(
        scraper
            .extract_company_profile_url(&show_product(true))
            .as_deref(),
        Some(
            "https://www.globalsources.com/exhibitors/HK/anhui-dujia-office-supplies-manufacturing-co-ltd_2008801135123/"
        )
    );
    assert_eq!(scraper.extract_company_profile_url(&show_exhibitor()), None);
    assert_eq!(
        scraper.company_key(&show_product(true)),
        scraper.company_key(&show_exhibitor())
    );
    let l = scraper.enrich_listing_from_company_profile(
        scraper.parse_listing(&show_product(true), SHOW_PRODUCT_URL),
        &show_exhibitor(),
    );
    assert_eq!(l.unit_price.as_deref(), Some("US$ 3.90"));
    assert_eq!(l.minimum_order_quantity.as_deref(), Some("1000 Pieces"));
    assert_eq!(l.incoterms.as_deref(), Some("FOB"));
    assert_eq!(scraper.parse_listing(&show_exhibitor(), "u").title, None);
}

#[test]
fn an_unchecked_exhibitor_has_no_verified_badge() {
    let scraper = get_scraper_for_platform("globalsources").unwrap();
    let s = scraper.parse_supplier(&show_product(false), SHOW_PRODUCT_URL);
    assert!(!s.platform_verified_badge);
    assert_eq!(s.badge_honorific, None);
    assert!(!s.company_description.unwrap().contains("Addcredit"));
}

#[tokio::test]
async fn a_trade_show_scan_reads_both_pages_from_the_browser() {
    unsafe {
        std::env::set_var("SCRAPERAPI_KEY", "k");
    }
    let page = b2b_page_from_browser_first(
        "globalsources",
        SHOW_PRODUCT_URL,
        Some(&show_product(true)),
        Some(&show_exhibitor()),
        None,
    )
    .await
    .expect("read from the browser");
    assert!(!page.company_page_missing);
    assert_eq!(page.listing.unit_price.as_deref(), Some("US$ 3.90"));
    assert_eq!(
        page.company_key.as_deref(),
        Some("globalsources-exhibitor-2008801135123")
    );
    assert!(page.record.unwrap().checked_by_platform);
}
