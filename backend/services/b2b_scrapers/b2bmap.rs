use super::{B2bListingProfile, B2bScraper, B2bSupplierProfile};
use scraper::{ElementRef, Html, Selector};

pub struct B2bmapScraper;

fn text_of(el: &ElementRef) -> String {
    el.text().collect::<Vec<_>>().join(" ")
}

fn clean_optional_text(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Reads a value out of any of b2bmap's real "label / value" tables -
/// covers both shapes actually used on the site: a plain label with
/// no colon (the top product-summary table, e.g. "MOQ") and a label
/// with a trailing colon (the Payment/Packaging/Delivery table and
/// the Member Information table, e.g. "Employees:") - normalizing
/// both by trimming a trailing ':' before comparing.
fn find_table_value(document: &Html, table_selector: &str, label: &str) -> Option<String> {
    let table_sel = Selector::parse(table_selector).ok()?;
    let row_sel = Selector::parse("tr").ok()?;
    let cell_sel = Selector::parse("td").ok()?;

    for table in document.select(&table_sel) {
        for row in table.select(&row_sel) {
            let cells: Vec<_> = row.select(&cell_sel).collect();
            if cells.len() < 2 {
                continue;
            }
            let row_label = text_of(&cells[0]).trim().trim_end_matches(':').to_string();
            if row_label.eq_ignore_ascii_case(label) {
                // Real b2bmap tables come in two shapes: a plain
                // 2-cell "label / value" row (product-summery-table)
                // and a 3-cell "label / : / value" row (the Member
                // Information table). The value is always the LAST
                // cell in the row either way, so read from the end
                // rather than assuming index 1 - assuming 1 silently
                // grabbed the ":" separator cell on 3-cell rows.
                return clean_optional_text(&text_of(cells.last().unwrap()));
            }
        }
    }
    None
}

/// Reads a value out of the Company Overview section on the company
/// profile page - a real, actual <div> grid rather than a <table>
/// (unlike every other section on the same page), so it needs its
/// own, separate lookup.
fn find_overview_field(document: &Html, label: &str) -> Option<String> {
    let row_sel = Selector::parse("div.d-md-table-row, div.d-flex.d-md-table-row").ok()?;
    let cell_sel = Selector::parse("div.d-md-table-cell").ok()?;

    for row in document.select(&row_sel) {
        let cells: Vec<_> = row.select(&cell_sel).collect();
        if cells.len() < 2 {
            continue;
        }
        let row_label = text_of(&cells[0]).trim().trim_end_matches(':').to_string();
        if row_label.eq_ignore_ascii_case(label) {
            return clean_optional_text(&text_of(&cells[1]));
        }
    }
    None
}

/// The real supplier-name link in the product page's sidebar (e.g.
/// "Wuhan Nice Laser Co., Ltd" linking to
/// b2bmap.com/wuhan-nice-laser) - this exact selector is what both
/// the company name AND the company profile URL are read from, since
/// on b2bmap they're the same element.
fn find_supplier_name_link(document: &Html) -> Option<ElementRef<'_>> {
    let sel = Selector::parse("h4.text-18.text-lg-22 a").ok()?;
    document.select(&sel).next()
}

impl B2bScraper for B2bmapScraper {
    fn matches_platform(&self, platform: &str) -> bool {
        platform == "b2bmap"
    }

    fn parse_supplier(&self, html: &str, profile_url: &str) -> B2bSupplierProfile {
        let document = Html::parse_document(html);

        let company_name =
            find_supplier_name_link(&document).map(|el| text_of(&el).trim().to_string());

        // Location (e.g. "Hubei, China") - the real, plain-text line
        // right under the company name in the sidebar; scoped to the
        // desktop sidebar column specifically so it never picks up
        // the unrelated "Contact Supplier for..." paragraph further
        // down the page, which shares the .text-muted class.
        let country = Selector::parse(".col-lg-4.col-xl-3.d-lg-down-none p.text-muted.mb-2")
            .ok()
            .and_then(|s| document.select(&s).next())
            .map(|el| text_of(&el))
            .and_then(|t| clean_optional_text(&t));

        // "Year Established: 2022" - strip the label, keep the year.
        let year_established = Selector::parse(".col-lg-4.col-xl-3.d-lg-down-none p.mb-2")
            .ok()
            .and_then(|s| {
                document
                    .select(&s)
                    .find(|el| text_of(el).contains("Year Established"))
            })
            .map(|el| text_of(&el))
            .and_then(|t| t.split(':').nth(1).map(|s| s.trim().to_string()))
            .filter(|t| !t.is_empty());

        // Business type ("Manufacturer", "Exporter", ...) - the FIRST
        // matching <ul> only (there's a near-identical, later-in-page
        // "Supplier Info" section that reuses the same classes), then
        // every <li> inside that one specific list.
        let badge_honorific = Selector::parse("ul.d-flex.flex-wrap.pl-3")
            .ok()
            .and_then(|ul_sel| document.select(&ul_sel).next())
            .and_then(|ul| {
                let li_sel = Selector::parse("li").ok()?;
                let types: Vec<String> = ul
                    .select(&li_sel)
                    .map(|li| text_of(&li).trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
                if types.is_empty() {
                    None
                } else {
                    Some(types.join(", "))
                }
            });

        // The real, unmasked phone number - only ever shown on the
        // product listing page's sidebar. The company profile page
        // (enrich_from_company_profile, below) shows this same
        // number MASKED to anyone not signed in with a paid account
        // (e.g. "+861538xxxxx"), so that page is never used as a
        // phone source - only this one, real, complete number is.
        let contact_phone = Selector::parse("span.d-flex.mb-3.align-items-center span.text-muted")
            .ok()
            .and_then(|s| document.select(&s).next())
            .map(|el| text_of(&el))
            .and_then(|t| clean_optional_text(&t));

        B2bSupplierProfile {
            company_name,
            logo_url: None,
            year_established,
            country,
            platform_verified_badge: false,
            employee_count: None,
            sales_revenue: None,
            export_percentage: None,
            profile_url: profile_url.to_string(),
            source_platform: "b2bmap".to_string(),
            contact_name: None,
            contact_phone,
            badge_honorific,
            company_description: None,
            website_url: None,
        }
    }

    fn extract_company_profile_url(&self, listing_html: &str) -> Option<String> {
        let document = Html::parse_document(listing_html);
        find_supplier_name_link(&document)
            .and_then(|el| el.value().attr("href").map(|s| s.to_string()))
    }

    fn enrich_from_company_profile(
        &self,
        mut supplier: B2bSupplierProfile,
        profile_html: &str,
    ) -> B2bSupplierProfile {
        let document = Html::parse_document(profile_html);

        // The real, fuller company description - the product page
        // only shows a short, truncated ("...") preview of this same
        // text, so this page's version always wins when present.
        if let Ok(sel) = Selector::parse(".clean-link") {
            if let Some(el) = document.select(&sel).next() {
                if let Some(description) = clean_optional_text(&text_of(&el)) {
                    supplier.company_description = Some(description);
                }
            }
        }

        supplier.employee_count =
            find_table_value(&document, "table", "Employees").or(supplier.employee_count);

        // The real contact PERSON (e.g. "Mr. Mike Huang (CEO)") -
        // only ever shown here, never on the product listing page.
        if let Ok(sel) = Selector::parse("a.text-14.text-muted") {
            if let Some(el) = document.select(&sel).next() {
                let raw = text_of(&el);
                let cleaned: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
                if !cleaned.is_empty() {
                    supplier.contact_name = Some(cleaned);
                }
            }
        }

        // Membership tier (e.g. "Free Member") is the closest thing
        // b2bmap actually shows to a verification badge. Real
        // caveat: every profile pulled so far has been "Free Member"
        // - this heuristic (verified = anything NOT containing
        // "free") is a best guess until a paid-tier profile is seen
        // to confirm the real wording of that tier.
        if let Some(membership_type) = find_table_value(&document, "table", "Membership Type") {
            supplier.platform_verified_badge = !membership_type.to_lowercase().contains("free");
            supplier.badge_honorific = Some(membership_type);
        }

        // Fallback fill only - the listing page's "Hubei, China" style
        // region text is more specific than this page's bare
        // "Country: China", so this never overwrites a value already
        // captured from the listing page.
        if supplier.country.is_none() {
            supplier.country = find_overview_field(&document, "Country");
        }
        if supplier.company_name.is_none() {
            supplier.company_name = find_overview_field(&document, "Company Name");
        }

        // Deliberately NOT reading Contact Number/Whatsapp here -
        // both are always masked ("+861538xxxxx") without a paid
        // account, and supplier.contact_phone already holds the real
        // number captured from the listing page in parse_supplier.

        supplier
    }

    fn parse_listing(&self, html: &str, listing_url: &str) -> B2bListingProfile {
        let document = Html::parse_document(html);

        let title = Selector::parse("h1.text-18.text-md-26.text-strong")
            .ok()
            .and_then(|s| document.select(&s).next())
            .map(|el| text_of(&el))
            .and_then(|t| clean_optional_text(&t));

        let description = Selector::parse(".product-details-content p")
            .ok()
            .map(|sel| {
                document
                    .select(&sel)
                    .map(|p| text_of(&p).trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .and_then(|t| clean_optional_text(&t));

        let image_urls: Vec<String> = Selector::parse("#viewProductImages img")
            .ok()
            .map(|sel| {
                document
                    .select(&sel)
                    .filter_map(|el| el.value().attr("src").map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let unit_price = find_table_value(&document, "table.product-summery-table", "Price");
        let minimum_order_quantity =
            find_table_value(&document, "table.product-summery-table", "MOQ");

        // b2bmap has no dedicated "reference code" field of its own -
        // HS Code is the closest real match (a genuine, standardized
        // trade-classification reference for the product). Not seen filled
        // in on any of the 3 real listings checked so far, but left
        // querying rather than hardcoded to None, in case another product
        // category on b2bmap does provide it.
        let reference = find_table_value(&document, "table.product-summery-table", "HS Code");

        let payment_type = find_table_value(
            &document,
            "div.table-responsive.mt-4 table",
            "Payment Terms",
        );
        let packaging_details = find_table_value(
            &document,
            "div.table-responsive.mt-4 table",
            "Packaging Info",
        );
        let delivery_timeframe = find_table_value(
            &document,
            "div.table-responsive.mt-4 table",
            "Delivery Info",
        );

        B2bListingProfile {
            title,
            description,
            image_urls,
            unit_price,
            fob_price: None,
            minimum_order_quantity,
            payment_type,
            preferred_port: None,
            reference,
            production_capacity: None,
            delivery_timeframe,
            incoterms: None,
            packaging_details,
            listing_url: listing_url.to_string(),
            source_platform: "b2bmap".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing_html() -> &'static str {
        r#"
        <html><body>
        <div class="col-lg-8 col-xl-9">
            <h1 class="text-18 text-md-26 text-strong mb-3 d-lg-none">Black Coarse Thread Drywall Screws for Wood Studs - Bulk Supply from China</h1>
            <div class="table-responsive">
                <table class="table table-sm table-bordered product-summery-table">
                    <tbody>
                        <tr><td>Country of Origin</td><td>China </td></tr>
                        <tr><td>MOQ</td><td class="min_order_unit"> 1 Tons </td></tr>
                        <tr><td>Price</td><td class="price_info">USD 2000 / Tons</td></tr>
                        <tr><td>Category</td><td><a href="https://b2bmap.com/product-list/construction-hardware">Tools &amp; Hardware</a></td></tr>
                    </tbody>
                </table>
            </div>
            <div class="mb-2 product-details-content">
                <p>Black coarse thread drywall screws are designed for fastening drywall.</p>
                <p>The coarse thread provides strong holding power in wood.</p>
            </div>
            <div class="table-responsive mt-4">
                <table class="table table-sm table-bordered text-14">
                    <tbody></tbody>
                </table>
            </div>
        </div>

        <div class="col-lg-4 col-xl-3 d-lg-down-none">
            <h4 class="text-18 text-lg-22">
                <a href="https://b2bmap.com/tianjin-huayan-international-trading" class="d-block text-strong">Tianjin Huayan International Trading Co., Ltd.</a>
            </h4>
            <p class="text-muted mb-2"><img src="cn.png" class="mr-1">Tianjin, China</p>
            <p class="mb-2"><span>Year Established:</span> 2001</p>
            <div class="mb-2">
                <p class="text-strong text-strong mb-1">Business Type:</p>
                <ul class="d-flex flex-wrap pl-3">
                    <li class="text-muted mr-4 mb-2">Manufacturer</li>
                    <li class="text-muted mr-4 mb-2">Exporter</li>
                </ul>
            </div>
            <div class="mb-3">
                <span class="d-flex mb-3 align-items-center">
                    <span class="box-30 border rounded-circle bg-light-white mr-2"><i class="fa fa-phone mr-2 text-13"></i></span>
                    <span class="text-muted">+8613389057831</span>
                </span>
            </div>
        </div>

        <div id="viewProductImages">
            <img src="https://b2bmap.com/product-image/202608/black-coarse-thread-drywall-screws-for-wood-studs-05248.png">
            <img src="https://b2bmap.com/product-image/202608/black-coarse-thread-drywall-screws-for-wood-studs-18828.jpg">
        </div>
        </body></html>
        "#
    }

    fn listing_html_with_duplicate_mobile_sidebar() -> &'static str {
        // Same real page, but with a later "mobile" duplicate section
        // reusing the exact same classes - matches how b2bmap's real
        // markup actually duplicates this block for mobile. Values
        // here are deliberately different from the real sidebar so a
        // test can prove the first, correct one is what gets read.
        r#"
        <html><body>
        <div class="col-lg-4 col-xl-3 d-lg-down-none">
            <h4 class="text-18 text-lg-22">
                <a href="https://b2bmap.com/tianjin-huayan-international-trading" class="d-block text-strong">Tianjin Huayan International Trading Co., Ltd.</a>
            </h4>
            <p class="text-muted mb-2"><img src="cn.png" class="mr-1">Tianjin, China</p>
            <p class="mb-2"><span>Year Established:</span> 2001</p>
            <div class="mb-2">
                <p class="text-strong text-strong mb-1">Business Type:</p>
                <ul class="d-flex flex-wrap pl-3">
                    <li class="text-muted mr-4 mb-2">Manufacturer</li>
                    <li class="text-muted mr-4 mb-2">Exporter</li>
                </ul>
            </div>
            <div class="mb-3">
                <span class="d-flex mb-3 align-items-center">
                    <span class="box-30 border rounded-circle bg-light-white mr-2"><i class="fa fa-phone mr-2 text-13"></i></span>
                    <span class="text-muted">+8613389057831</span>
                </span>
            </div>
        </div>

        <div class="d-lg-none mb-3">
            <div class="mb-2">
                <p class="text-strong text-strong mb-1">Business Type:</p>
                <ul class="d-flex flex-wrap pl-3">
                    <li class="text-muted mr-4 mb-2">WRONG_TYPE</li>
                </ul>
            </div>
            <span class="d-flex mb-3 align-items-center">
                <span class="box-30 border rounded-circle bg-light-white mr-2"><i class="fa fa-phone mr-2 text-13"></i></span>
                <span class="text-muted">+19999999999</span>
            </span>
        </div>
        </body></html>
        "#
    }

    fn listing_html_with_payment_details_table_filled() -> &'static str {
        // No real b2bmap listing seen so far has this table filled
        // in - this only proves the lookup mechanics work, not that
        // these are the real label strings b2bmap actually uses.
        r#"
        <html><body>
        <div class="table-responsive mt-4">
            <table class="table table-sm table-bordered text-14">
                <tbody>
                    <tr><td>Payment Terms</td><td>T/T, L/C</td></tr>
                    <tr><td>Packaging Info</td><td>Carton box</td></tr>
                    <tr><td>Delivery Info</td><td>15-30 days</td></tr>
                </tbody>
            </table>
        </div>
        </body></html>
        "#
    }

    fn profile_html() -> &'static str {
        r#"
        <html><body>
        <div class="card-body bg-white">
            <div class="clean-link">
                <p>Tianjin Huayan International Trading Co., Ltd. is a factory-backed foreign trade company specialising in the export of metal fasteners and nail products.</p>
            </div>
        </div>

        <div class="card-body bg-white">
            <table class="table table-sm table-borderless w-auto mb-0">
                <tbody>
                    <tr><td>Business Type</td><td class="px-2">:</td><td><ul><li>Manufacturer</li><li>Exporter</li></ul></td></tr>
                    <tr><td>Founded in</td><td class="px-2">:</td><td>2001</td></tr>
                    <tr><td>Employees</td><td class="px-2">:</td><td>101-500</td></tr>
                </tbody>
            </table>
            <table class="table table-sm table-borderless w-auto mb-0">
                <tbody>
                    <tr><td>Member Since</td><td class="px-2 text-muted">:</td><td>29 Aug 2026</td></tr>
                    <tr><td>Membership Type</td><td class="px-2 text-muted">:</td><td>Free Member</td></tr>
                    <tr><td>Business Category</td><td class="px-2">:</td><td><a href="https://b2bmap.com/china/tools-hardware-product-suppliers">Tools &amp; Hardware</a></td></tr>
                </tbody>
            </table>
        </div>

        <div class="border-left border-4px border-business-secondary-close px-3 py-2">
            <a href="https://b2bmap.com/tianjin-huayan-international-trading" class="d-block text-strong">Tianjin Huayan International Trading Co., Ltd.</a>
            <a href="https://b2bmap.com/tianjin-huayan-international-trading/contact-info" class="text-14 text-muted"> Mr. Man zhao  (Sales Manager)  <i class="fa fa-envelope text-business-secondary-close ml-1"></i></a>
        </div>

        <div class="d-md-table d-company-info-table w-100">
            <div class="d-flex d-md-table-row">
                <div class="d-md-table-cell"><span class="d-md-down-none text-nowrap">Company Name:</span></div>
                <div class="d-md-table-cell"><span class="d-inline-block">Tianjin Huayan International Trading Co., Ltd.</span></div>
            </div>
            <div class="d-flex d-md-table-row">
                <div class="d-md-table-cell"><span class="d-md-down-none text-nowrap">Contact Number:</span></div>
                <div class="d-md-table-cell"><span data-toggle="modal" class="cursor">+861338xxxxx</span></div>
            </div>
            <div class="d-flex d-md-table-row">
                <div class="d-md-table-cell"><span class="d-md-down-none text-nowrap">Country:</span></div>
                <div class="d-md-table-cell"><a href="https://b2bmap.com/china">China</a></div>
            </div>
        </div>
        </body></html>
        "#
    }

    fn profile_html_paid_member() -> &'static str {
        r#"
        <html><body>
        <table class="table table-sm table-borderless w-auto mb-0">
            <tbody>
                <tr><td>Membership Type</td><td class="px-2 text-muted">:</td><td>Gold Member</td></tr>
            </tbody>
        </table>
        </body></html>
        "#
    }

    fn blank_supplier() -> B2bSupplierProfile {
        B2bSupplierProfile {
            source_platform: "b2bmap".to_string(),
            profile_url: "https://b2bmap.com/tianjin-huayan-international-trading".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn matches_platform_is_true_only_for_b2bmap() {
        let scraper = B2bmapScraper;
        assert!(scraper.matches_platform("b2bmap"));
        assert!(!scraper.matches_platform("thomasnet"));
        assert!(!scraper.matches_platform("alibaba"));
    }

    #[test]
    fn parse_supplier_extracts_the_real_fields_from_the_listing_sidebar() {
        let scraper = B2bmapScraper;
        let profile_url = "https://b2bmap.com/tianjin-huayan-international-trading";
        let supplier = scraper.parse_supplier(listing_html(), profile_url);

        assert_eq!(
            supplier.company_name.as_deref(),
            Some("Tianjin Huayan International Trading Co., Ltd.")
        );
        assert_eq!(supplier.country.as_deref(), Some("Tianjin, China"));
        assert_eq!(supplier.year_established.as_deref(), Some("2001"));
        assert_eq!(
            supplier.badge_honorific.as_deref(),
            Some("Manufacturer, Exporter")
        );
        assert_eq!(supplier.contact_phone.as_deref(), Some("+8613389057831"));
        assert_eq!(supplier.profile_url, profile_url);
        assert_eq!(supplier.source_platform, "b2bmap");
        assert!(
            !supplier.platform_verified_badge,
            "expected no verification claim until the profile page is enriched"
        );
        assert!(supplier.employee_count.is_none());
        assert!(supplier.sales_revenue.is_none());
        assert!(supplier.export_percentage.is_none());
    }

    #[test]
    fn parse_supplier_picks_the_first_sidebar_business_type_and_phone_not_a_later_duplicate() {
        let scraper = B2bmapScraper;
        let supplier = scraper.parse_supplier(
            listing_html_with_duplicate_mobile_sidebar(),
            "https://b2bmap.com/tianjin-huayan-international-trading",
        );

        assert_eq!(
            supplier.badge_honorific.as_deref(),
            Some("Manufacturer, Exporter"),
            "expected the real desktop sidebar business type, not the later duplicate section's"
        );
        assert_eq!(
            supplier.contact_phone.as_deref(),
            Some("+8613389057831"),
            "expected the real phone number, not a later duplicate section's value"
        );
    }

    #[test]
    fn parse_supplier_returns_none_fields_for_a_page_with_no_sidebar_at_all() {
        let scraper = B2bmapScraper;
        let supplier = scraper.parse_supplier(
            "<html><body><p>nothing here</p></body></html>",
            "https://b2bmap.com/some-supplier",
        );

        assert!(supplier.company_name.is_none());
        assert!(supplier.country.is_none());
        assert!(supplier.year_established.is_none());
        assert!(supplier.badge_honorific.is_none());
        assert!(supplier.contact_phone.is_none());
    }

    #[test]
    fn extract_company_profile_url_reads_the_real_supplier_link_href() {
        let scraper = B2bmapScraper;
        let url = scraper.extract_company_profile_url(listing_html());
        assert_eq!(
            url.as_deref(),
            Some("https://b2bmap.com/tianjin-huayan-international-trading")
        );
    }

    #[test]
    fn extract_company_profile_url_is_none_when_the_supplier_link_is_missing() {
        let scraper = B2bmapScraper;
        let url = scraper.extract_company_profile_url("<html><body></body></html>");
        assert!(url.is_none());
    }

    #[test]
    fn enrich_from_company_profile_fills_in_the_real_fields_only_available_there() {
        let scraper = B2bmapScraper;
        let supplier = scraper.enrich_from_company_profile(blank_supplier(), profile_html());

        assert!(
            supplier
                .company_description
                .as_deref()
                .unwrap()
                .contains("factory-backed foreign trade company")
        );
        assert_eq!(supplier.employee_count.as_deref(), Some("101-500"));
        assert_eq!(
            supplier.contact_name.as_deref(),
            Some("Mr. Man zhao (Sales Manager)")
        );
        assert_eq!(supplier.badge_honorific.as_deref(), Some("Free Member"));
        assert!(
            !supplier.platform_verified_badge,
            "a Free Member should never be marked as platform-verified"
        );
    }

    #[test]
    fn enrich_from_company_profile_marks_a_non_free_membership_tier_as_verified() {
        let scraper = B2bmapScraper;
        let supplier =
            scraper.enrich_from_company_profile(blank_supplier(), profile_html_paid_member());

        assert_eq!(supplier.badge_honorific.as_deref(), Some("Gold Member"));
        assert!(
            supplier.platform_verified_badge,
            "expected a non-'Free' membership tier to be treated as verified under the current heuristic"
        );
    }

    #[test]
    fn enrich_from_company_profile_never_overwrites_a_phone_number_already_captured_from_the_listing_page()
     {
        let scraper = B2bmapScraper;
        let mut supplier = blank_supplier();
        supplier.contact_phone = Some("+8613389057831".to_string());

        let supplier = scraper.enrich_from_company_profile(supplier, profile_html());

        assert_eq!(
            supplier.contact_phone.as_deref(),
            Some("+8613389057831"),
            "the real listing-page phone number must never be overwritten by the profile page, masked or not"
        );
    }

    #[test]
    fn enrich_from_company_profile_only_fills_country_and_company_name_as_a_fallback() {
        let scraper = B2bmapScraper;
        let mut supplier = blank_supplier();
        supplier.country = Some("Tianjin, China".to_string());
        supplier.company_name = Some("Tianjin Huayan International Trading Co., Ltd.".to_string());

        let supplier = scraper.enrich_from_company_profile(supplier, profile_html());

        assert_eq!(
            supplier.country.as_deref(),
            Some("Tianjin, China"),
            "the listing page's more specific region text must not be overwritten by the profile page's bare 'China'"
        );
        assert_eq!(
            supplier.company_name.as_deref(),
            Some("Tianjin Huayan International Trading Co., Ltd.")
        );
    }

    #[test]
    fn enrich_from_company_profile_fills_country_and_company_name_when_missing() {
        let scraper = B2bmapScraper;
        let supplier = scraper.enrich_from_company_profile(blank_supplier(), profile_html());

        assert_eq!(supplier.country.as_deref(), Some("China"));
        assert_eq!(
            supplier.company_name.as_deref(),
            Some("Tianjin Huayan International Trading Co., Ltd.")
        );
    }

    #[test]
    fn parse_listing_extracts_the_real_fields_that_actually_exist_on_b2bmap() {
        let scraper = B2bmapScraper;
        let listing_url = "https://b2bmap.com/products/black-coarse-thread-drywall-screws";
        let listing = scraper.parse_listing(listing_html(), listing_url);

        assert_eq!(
            listing.title.as_deref(),
            Some("Black Coarse Thread Drywall Screws for Wood Studs - Bulk Supply from China")
        );
        assert!(
            listing
                .description
                .as_deref()
                .unwrap()
                .contains("Black coarse thread drywall screws")
        );
        assert_eq!(listing.image_urls.len(), 2);
        assert_eq!(listing.unit_price.as_deref(), Some("USD 2000 / Tons"));
        assert_eq!(listing.minimum_order_quantity.as_deref(), Some("1 Tons"));
        assert_eq!(listing.listing_url, listing_url);
        assert_eq!(listing.source_platform, "b2bmap");
    }

    #[test]
    fn parse_listing_leaves_genuinely_nonexistent_b2bmap_fields_as_none() {
        let scraper = B2bmapScraper;
        let listing = scraper.parse_listing(
            listing_html(),
            "https://b2bmap.com/products/black-coarse-thread-drywall-screws",
        );

        assert!(
            listing.fob_price.is_none(),
            "b2bmap has no FOB price field anywhere"
        );
        assert!(
            listing.preferred_port.is_none(),
            "b2bmap has no preferred port field anywhere"
        );
        assert!(
            listing.production_capacity.is_none(),
            "b2bmap has no production capacity field anywhere"
        );
        assert!(
            listing.incoterms.is_none(),
            "b2bmap has no Incoterms field anywhere"
        );
    }

    #[test]
    fn parse_listing_returns_none_for_payment_packaging_and_delivery_when_the_real_table_is_empty()
    {
        // Matches the real, observed b2bmap markup: the
        // div.table-responsive.mt-4 table wrapper genuinely exists on
        // the page but has an empty <tbody> - confirmed on a real,
        // live listing.
        let scraper = B2bmapScraper;
        let listing = scraper.parse_listing(
            listing_html(),
            "https://b2bmap.com/products/black-coarse-thread-drywall-screws",
        );

        assert!(listing.payment_type.is_none());
        assert!(listing.packaging_details.is_none());
        assert!(listing.delivery_timeframe.is_none());
    }

    #[test]
    fn parse_listing_reads_payment_packaging_and_delivery_when_that_table_is_actually_filled_in() {
        // No real b2bmap listing with this table filled in has been
        // seen yet - this only proves the lookup mechanics work, not
        // the real label wording b2bmap would actually use.
        let scraper = B2bmapScraper;
        let listing = scraper.parse_listing(
            listing_html_with_payment_details_table_filled(),
            "https://b2bmap.com/products/black-coarse-thread-drywall-screws",
        );

        assert_eq!(listing.payment_type.as_deref(), Some("T/T, L/C"));
        assert_eq!(listing.packaging_details.as_deref(), Some("Carton box"));
        assert_eq!(listing.delivery_timeframe.as_deref(), Some("15-30 days"));
    }

    #[test]
    fn parse_listing_reference_falls_back_to_none_when_no_hs_code_row_exists() {
        let scraper = B2bmapScraper;
        let listing = scraper.parse_listing(
            listing_html(),
            "https://b2bmap.com/products/black-coarse-thread-drywall-screws",
        );
        assert!(
            listing.reference.is_none(),
            "no real b2bmap listing seen so far has an HS Code row"
        );
    }

    #[test]
    fn parse_listing_reference_is_read_when_an_hs_code_row_is_actually_present() {
        let scraper = B2bmapScraper;
        let html = r#"
        <html><body>
        <div class="table-responsive">
            <table class="table table-sm table-bordered product-summery-table">
                <tbody>
                    <tr><td>Country of Origin</td><td>China</td></tr>
                    <tr><td>HS Code</td><td>7318.14</td></tr>
                </tbody>
            </table>
        </div>
        </body></html>
        "#;
        let listing = scraper.parse_listing(html, "https://b2bmap.com/products/some-listing");
        assert_eq!(listing.reference.as_deref(), Some("7318.14"));
    }
}
