use crate::services::b2b_scrapers::{B2bListingProfile, B2bScraper, B2bSupplierProfile};
use scraper::{ElementRef, Html, Selector};
use std::collections::HashMap;

pub struct TradewheelScraper;

/// Longest product description passed on to the check.
const MAX_DESCRIPTION_CHARS: usize = 3000;

impl B2bScraper for TradewheelScraper {
    fn matches_platform(&self, platform: &str) -> bool {
        platform == "tradewheel"
    }

    fn parse_supplier(&self, html: &str, profile_url: &str) -> B2bSupplierProfile {
        let document = Html::parse_document(html);

        let company_name = select_text(&document, ".comp-info h2");

        // " China" next to a flag icon.
        let country = select_text(&document, ".bo-flag")
            .or_else(|| select_text(&document, ".comp-info p.address"));

        // TradeWheel's Gold / Platinum badges are paid membership levels.
        // The tier name is kept (it is shown to Claude and in the
        // verification card), but it is never counted as verification.
        let badge_honorific = membership_tier(&document);

        B2bSupplierProfile {
            company_name,
            logo_url: None,
            year_established: None,
            country,
            platform_verified_badge: false,
            employee_count: None,
            sales_revenue: None,
            export_percentage: None,
            profile_url: profile_url.to_string(),
            source_platform: "tradewheel".to_string(),
            contact_name: None,
            contact_phone: None,
            badge_honorific,
            company_description: None,
            website_url: None,
        }
    }

    fn parse_listing(&self, html: &str, listing_url: &str) -> B2bListingProfile {
        let document = Html::parse_document(html);

        let title = select_text(&document, "h1.pd-heading");

        // Attribute table and quick details table hold "Label | Value"
        // cells, sometimes two pairs in one row.
        let mut fields = HashMap::new();
        for selector in ["table.attr_table tr", "table.quick_details_table tr"] {
            read_table_pairs(&document, selector, &mut fields);
        }
        // "FOB Price  60 - 80 USD / Carat" and the price-tier table.
        let (headline_fob, headline_price) = read_price_headline(&document);
        let tiers = read_price_tiers(&document);
        // Product details written as <dl><dt>Label:</dt><dd>Value</dd></dl>.
        let detail_pairs = read_dl_pairs(&document);
        for (label, value) in &detail_pairs {
            fields.entry(label.clone()).or_insert_with(|| value.clone());
        }

        let get = |labels: &[&str]| -> Option<String> {
            labels
                .iter()
                .find_map(|l| fields.get(&normalize_label(l)).cloned())
        };

        let description = extract_description(&document);

        B2bListingProfile {
            title,
            description,
            image_urls: extract_image_urls(&document),
            unit_price: tiers
                .get("price")
                .cloned()
                .or(headline_price)
                .or_else(|| get(&["Price", "Unit Price"])),
            fob_price: headline_fob.or_else(|| get(&["FOB Price"])),
            // The product table says "MOQ". The tier table's "Quantity"
            // is a price bracket, not the minimum order, so it is only a
            // last resort.
            minimum_order_quantity: get(&["MOQ", "Minimum Order Quantity", "Min. Order"])
                .or_else(|| tiers.get("quantity").cloned()),
            payment_type: get(&["Payment Terms", "Payment Term", "Payment Type", "Payment"]),
            preferred_port: get(&["Port"]),
            reference: None,
            production_capacity: get(&["Supply Ability", "Production Capacity"]),
            delivery_timeframe: get(&["Lead Time", "Delivery Time"]),
            incoterms: get(&["Incoterms", "Trade Terms"]),
            packaging_details: get(&["Packaging", "Packaging Details"]),
            listing_url: listing_url.to_string(),
            source_platform: "tradewheel".to_string(),
        }
    }

    fn extract_company_profile_url(&self, listing_html: &str) -> Option<String> {
        let document = Html::parse_document(listing_html);
        let sel = Selector::parse(".comp-info a").ok()?;
        // The first link that is actually the company page (/co/...).
        document
            .select(&sel)
            .filter_map(|a| a.value().attr("href"))
            .map(str::trim)
            .find(|href| href.contains("/co/"))
            .map(|href| {
                if href.starts_with("http") {
                    href.to_string()
                } else {
                    format!("https://www.tradewheel.com{}", href)
                }
            })
    }

    fn enrich_from_company_profile(
        &self,
        mut supplier: B2bSupplierProfile,
        profile_html: &str,
    ) -> B2bSupplierProfile {
        let document = Html::parse_document(profile_html);

        // Company Information, Trading Information and Contact Details
        // are all "Label | Value" table rows.
        let mut fields = HashMap::new();
        read_table_pairs(&document, "tr", &mut fields);
        let get = |label: &str| -> Option<String> {
            fields
                .get(&normalize_label(label))
                .and_then(|v| non_placeholder(v))
        };

        fill(
            &mut supplier.year_established,
            get("Established Year").or_else(|| get("Year Established")),
        );
        fill(&mut supplier.employee_count, get("Total Employees"));
        fill(
            &mut supplier.sales_revenue,
            get("Total Revenue").or_else(|| get("Annual Revenue")),
        );
        fill(&mut supplier.export_percentage, get("Export Percentage"));
        fill(
            &mut supplier.country,
            get("Country/Region").or_else(|| get("Country")),
        );

        // The Website row shows a "Show" button that only works for
        // logged-in members - only a real address is accepted. (The
        // extension sends the real one from the buyer's own view.)
        if supplier.website_url.is_none() {
            supplier.website_url = get("Website").filter(|w| looks_like_website(w));
        }

        if let Some(name) =
            select_text(&document, ".contact_p_txt1").and_then(|n| non_placeholder(&n))
        {
            supplier.contact_name = Some(name);
        }

        // TradeWheel's company page has no "about" text, but it does
        // list what the company sells and what kind of business it is -
        // what Claude needs to check the name against the products.
        if supplier.company_description.is_none() {
            let mut parts = Vec::new();
            if let Some(v) = get("Business Type") {
                parts.push(format!("Business type: {}.", v));
            }
            if let Some(v) = get("Main Products") {
                parts.push(format!("Main products: {}.", v));
            }
            if let Some(v) =
                select_text(&document, ".contact_p_txt2").and_then(|r| non_placeholder(&r))
            {
                parts.push(format!("Contact person's role: {}.", v));
            }
            if !parts.is_empty() {
                supplier.company_description = Some(parts.join(" "));
            }
        }

        // TradeWheel never shows a phone number on the page. If the page
        // ever carries one in its structured data, use it.
        if supplier.contact_phone.is_none() {
            supplier.contact_phone = extract_structured_phone(profile_html);
        }

        if supplier.logo_url.is_none() {
            supplier.logo_url = select_img(&document, ".comp_logo img")
                .or_else(|| select_img(&document, "#m_img"))
                .filter(|u| !is_placeholder_image(u));
        }

        if supplier.company_name.is_none() {
            supplier.company_name = select_text(&document, "h1");
        }

        supplier
    }

    /// One seller record per company: the company page's slug,
    /// e.g. "https://www.tradewheel.com/co/fengtai-clothing/" ->
    /// "fengtai-clothing". Every product from that company links to the
    /// same page, so every scan lands on the same record.
    fn company_key(&self, listing_html: &str) -> Option<String> {
        company_slug(&self.extract_company_profile_url(listing_html)?)
    }
}

/// "https://www.tradewheel.com/co/fengtai-clothing/" -> "fengtai-clothing".
fn company_slug(url: &str) -> Option<String> {
    let rest = url.split("/co/").nth(1)?;
    let rest = rest.split(|c| c == '?' || c == '#').next()?;
    let slug = rest
        .split('/')
        .find(|s| !s.is_empty())?
        .trim()
        .to_lowercase();
    if slug.is_empty() { None } else { Some(slug) }
}

/// The headline price, e.g. `<span>FOB Price</span> 60 - 80 USD / Carat`.
/// Returns (fob_price, other_price).
fn read_price_headline(document: &Html) -> (Option<String>, Option<String>) {
    let Some(text) = select_text(document, ".po-box .pd-price") else {
        return (None, None);
    };
    if let Some(rest) = text.strip_prefix("FOB Price") {
        let rest = rest.trim().to_string();
        return (if rest.is_empty() { None } else { Some(rest) }, None);
    }
    let rest = text
        .strip_prefix("Price")
        .unwrap_or(&text)
        .trim()
        .to_string();
    (None, if rest.is_empty() { None } else { Some(rest) })
}

/// The price-tier table: each row is a label followed by one value per
/// tier ("Price | USD 60 | USD 60 ( wholesale only )"). Values are
/// joined, repeats dropped. Keys are normalized labels.
fn read_price_tiers(document: &Html) -> HashMap<String, String> {
    let mut tiers = HashMap::new();
    let (Ok(row_sel), Ok(cell_sel)) = (
        Selector::parse(".po-box table tr"),
        Selector::parse("th, td"),
    ) else {
        return tiers;
    };
    for row in document.select(&row_sel) {
        let cells: Vec<String> = row
            .select(&cell_sel)
            .map(|c| collapse_whitespace(&c.text().collect::<String>()))
            .collect();
        let Some((label, values)) = cells.split_first() else {
            continue;
        };
        let mut distinct: Vec<&str> = Vec::new();
        for v in values {
            if !v.is_empty() && !distinct.contains(&v.as_str()) {
                distinct.push(v);
            }
        }
        if !distinct.is_empty() {
            tiers.insert(normalize_label(label), distinct.join(" / "));
        }
    }
    tiers
}

/// "Gold" from the gold badge image (gold-txt1.png.webp), same for
/// Platinum. None when the company has no membership badge.
fn membership_tier(document: &Html) -> Option<String> {
    let sel = Selector::parse(".comp-info img").ok()?;
    for img in document.select(&sel) {
        let src = img_url(&img).unwrap_or_default().to_lowercase();
        let alt = img.value().attr("alt").unwrap_or("").to_lowercase();
        let text = format!("{} {}", src, alt);
        if text.contains("platinum") {
            return Some("Platinum".to_string());
        }
        if text.contains("gold") {
            return Some("Gold".to_string());
        }
    }
    None
}

/// "Place of Origin:" -> "place of origin". Labels are compared this way
/// so small spelling differences in capitals or colons don't matter.
fn normalize_label(label: &str) -> String {
    collapse_whitespace(label)
        .trim_end_matches(':')
        .trim()
        .to_lowercase()
}

/// Reads rows of "Label | Value" cells. A row can hold several pairs
/// (the quick details table has two per row), so cells are taken two
/// at a time. The first value seen for a label wins.
fn read_table_pairs(document: &Html, row_selector: &str, fields: &mut HashMap<String, String>) {
    let (Ok(row_sel), Ok(cell_sel)) = (Selector::parse(row_selector), Selector::parse("th, td"))
    else {
        return;
    };
    for row in document.select(&row_sel) {
        let cells: Vec<String> = row
            .select(&cell_sel)
            .map(|c| collapse_whitespace(&c.text().collect::<String>()))
            .collect();
        for pair in cells.chunks(2) {
            if let [label, value] = pair {
                let label = normalize_label(label);
                if !label.is_empty() && !value.is_empty() {
                    fields.entry(label).or_insert_with(|| value.clone());
                }
            }
        }
    }
}

/// Product details as <dl class="do-entry-item"><dt><span class="attr-name">
/// Place of Origin:</span></dt><dd><div class="text-ellipsis">China</div></dd></dl>.
/// Returned in page order, labels normalized, values as shown.
fn read_dl_pairs(document: &Html) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let (Ok(dl_sel), Ok(dt_sel), Ok(dd_sel)) = (
        Selector::parse(".product-details-container dl"),
        Selector::parse("dt"),
        Selector::parse("dd"),
    ) else {
        return pairs;
    };
    for dl in document.select(&dl_sel) {
        let label = dl
            .select(&dt_sel)
            .next()
            .map(|e| e.text().collect::<String>());
        let value = dl
            .select(&dd_sel)
            .next()
            .map(|e| e.text().collect::<String>());
        if let (Some(label), Some(value)) = (label, value) {
            let value = collapse_whitespace(&value);
            if !label.trim().is_empty() && !value.is_empty() {
                pairs.push((normalize_label(&label), value));
            }
        }
    }
    pairs
}

/// The product details section has no paragraphs on most listings - it
/// is a list of "Label: Value" entries, so those are used as the
/// description, followed by any paragraph text the seller wrote.
fn extract_description(document: &Html) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();

    if let Ok(dl_sel) = Selector::parse(".product-details-container dl") {
        if let (Ok(dt_sel), Ok(dd_sel)) = (Selector::parse("dt"), Selector::parse("dd")) {
            for dl in document.select(&dl_sel) {
                let label = dl
                    .select(&dt_sel)
                    .next()
                    .map(|e| collapse_whitespace(&e.text().collect::<String>()));
                let value = dl
                    .select(&dd_sel)
                    .next()
                    .map(|e| collapse_whitespace(&e.text().collect::<String>()));
                if let (Some(label), Some(value)) = (label, value) {
                    if !label.is_empty() && !value.is_empty() {
                        let label = label.trim_end_matches(':').trim();
                        parts.push(format!("{}: {}", label, value));
                    }
                }
            }
        }
    }
    if let Ok(p_sel) = Selector::parse(".product-details-container p") {
        for p in document.select(&p_sel) {
            let text = collapse_whitespace(&p.text().collect::<String>());
            if !text.is_empty() && !parts.iter().any(|x| x.contains(&text)) {
                parts.push(text);
            }
        }
    }

    // No detail list (e.g. Dadal): use the quick details table instead,
    // so product specifics still reach the check.
    if !parts.iter().any(|p| p.contains(": ")) {
        if let (Ok(row_sel), Ok(cell_sel)) = (
            Selector::parse("table.quick_details_table tr"),
            Selector::parse("td"),
        ) {
            let mut specs = Vec::new();
            for row in document.select(&row_sel) {
                let cells: Vec<String> = row
                    .select(&cell_sel)
                    .map(|c| collapse_whitespace(&c.text().collect::<String>()))
                    .collect();
                for pair in cells.chunks(2) {
                    if let [l, v] = pair {
                        if !l.is_empty() && !v.is_empty() {
                            specs.push(format!("{}: {}", l, v));
                        }
                    }
                }
            }
            if !specs.is_empty() {
                parts.insert(0, specs.join("; "));
            }
        }
    }

    if parts.is_empty() {
        return None;
    }
    let joined = parts.join("\n");
    Some(if joined.chars().count() > MAX_DESCRIPTION_CHARS {
        joined.chars().take(MAX_DESCRIPTION_CHARS).collect()
    } else {
        joined
    })
}

/// Full-size gallery images, then the page's og:image as a fallback.
/// At most this many product photos are kept (same as the other
/// B2B scrapers), so image checking never gets a long gallery.
const MAX_IMAGES: usize = 3;

fn extract_image_urls(document: &Html) -> Vec<String> {
    fn push(urls: &mut Vec<String>, url: &str) {
        let url = url.trim();
        if urls.len() < MAX_IMAGES
            && url.starts_with("http")
            && !is_placeholder_image(url)
            && !urls.iter().any(|u| u == url)
        {
            urls.push(url.to_string());
        }
    }
    let mut urls: Vec<String> = Vec::new();
    if let Ok(sel) = Selector::parse(".pd-thumbs a") {
        for a in document.select(&sel) {
            if let Some(url) = a
                .value()
                .attr("data-zoom-image")
                .or_else(|| a.value().attr("data-image"))
                .or_else(|| a.value().attr("href"))
            {
                push(&mut urls, url);
            }
        }
    }
    if urls.is_empty() {
        if let Some(og) = select_attr(document, r#"meta[property="og:image"]"#, "content") {
            push(&mut urls, &og);
        }
    }
    urls
}

/// Reads `"telephone": "..."` from structured data, as "+digits".
fn extract_structured_phone(html: &str) -> Option<String> {
    let start = html.find("\"telephone\"")?;
    let after_key = &html[start + "\"telephone\"".len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?.trim_start();
    let value = after_colon.strip_prefix('"')?;
    let end = value.find('"')?;
    let digits: String = value[..end]
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    if digits.len() < 7 {
        None
    } else {
        Some(format!("+{}", digits))
    }
}

/// "www.example.com" or "https://example.com" - not "Show", not an
/// email, not a TradeWheel link.
fn looks_like_website(value: &str) -> bool {
    let v = value.trim().to_lowercase();
    !v.contains(' ')
        && !v.contains('@')
        && v.contains('.')
        && !v.contains("tradewheel.com")
        && v.len() > 4
}

fn fill(slot: &mut Option<String>, value: Option<String>) {
    if slot.is_none() {
        *slot = value;
    }
}

fn is_placeholder_image(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("no-image")
        || lower.contains("noimage")
        || lower.contains("no_image")
        || lower.contains("placeholder")
}

fn select_img(document: &Html, selector: &str) -> Option<String> {
    let sel = Selector::parse(selector).ok()?;
    img_url(&document.select(&sel).next()?)
}

/// Real URL of an <img>, whichever attribute holds it (lazy-loaded
/// images keep it in data-src while src is a "data:" placeholder).
fn img_url(el: &ElementRef) -> Option<String> {
    ["src", "data-src", "data-original", "data-lazy-src"]
        .iter()
        .filter_map(|a| el.value().attr(a))
        .map(str::trim)
        .find(|s| !s.is_empty() && !s.starts_with("data:"))
        .map(|s| s.to_string())
}

fn select_text(document: &Html, selector: &str) -> Option<String> {
    let sel = Selector::parse(selector).ok()?;
    let text = collapse_whitespace(&document.select(&sel).next()?.text().collect::<String>());
    if text.is_empty() { None } else { Some(text) }
}

fn select_attr(document: &Html, selector: &str, attr: &str) -> Option<String> {
    let sel = Selector::parse(selector).ok()?;
    document
        .select(&sel)
        .next()?
        .value()
        .attr(attr)
        .map(|s| s.to_string())
}

fn collapse_whitespace(s: &str) -> String {
    fix_escaped_symbols(&s.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Some sellers' specs arrive with the unicode escape's backslash lost,
/// e.g. "u226575-u226595" for "≥75-≥95". Only a few common technical
/// symbols are restored; everything else is left as written.
fn fix_escaped_symbols(s: &str) -> String {
    if !s.contains('u') {
        return s.to_string();
    }
    s.replace("u2264", "≤")
        .replace("u2265", "≥")
        .replace("u00b0", "°")
        .replace("u00b1", "±")
}

fn non_placeholder(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let lower = trimmed.to_lowercase();
    if trimmed.is_empty()
        || lower == "not provided"
        || lower == "n/a"
        || lower == "-"
        || lower == "show"
    {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed to the structure of the real Fengtai Clothing listing page
    // (Sep 2026).
    const LISTING: &str = r#"<html><head>
<meta property="og:image" content="https://www.tradewheel.com/uploads/og-cover.jpg" />
</head><body>
<div class="pd-thumbs">
  <a href="javascript:void(0)" data-image="https://www.tradewheel.com/uploads/p1-small.jpg" data-zoom-image="https://www.tradewheel.com/uploads/p1.jpg"><img src="https://www.tradewheel.com/uploads/p1-thumb.jpg"></a>
  <a href="javascript:void(0)" data-image="https://www.tradewheel.com/uploads/p2-small.jpg" data-zoom-image="https://www.tradewheel.com/uploads/p2.jpg"><img src="https://www.tradewheel.com/uploads/p2-thumb.jpg"></a>
</div>
<h1 class="pd-heading">High-Quality Cotton Jacket with Calabash Pattern</h1>
<div class="po-box"></div>
<table class="attr_table">
  <tr><td>MOQ</td><td>100 500</td></tr>
  <tr><td>Port</td><td>China</td></tr>
  <tr><td>Packaging</td><td>OEM</td></tr>
  <tr><td>Lead Time</td><td>10-15 days</td></tr>
</table>
<table class="quick_details_table">
  <tr><td>Brand Name</td><td>Fengtai</td><td>Material</td><td>Cotton</td></tr>
</table>
<div class="product-details-container">
  <dl class="do-entry-item"><dt><span class="attr-name">Place of Origin:</span></dt><dd><div class="text-ellipsis">China</div></dd></dl>
  <dl class="do-entry-item"><dt><span class="attr-name">Material:</span></dt><dd><div class="text-ellipsis">100% Cotton</div></dd></dl>
</div>
<div class="comp-info">
  <a href="https://www.tradewheel.com/co/fengtai-clothing/"><h2>Fengtai Clothing</h2></a>
  <img src="https://www.tradewheel.com/images/gold-txt1.png.webp" alt="">
  <p class="address">Guangzhou, Guangdong</p>
  <span class="bo-flag"> China</span>
</div>
</body></html>"#;

    // Trimmed to the structure of the real Fengtai Clothing company page.
    const PROFILE: &str = r#"<html><body>
<div class="comp_logo"><img src="https://www.tradewheel.com/uploads/logo-fengtai.jpg"></div>
<h3 class="secondary-heading">Company Information</h3>
<table>
  <tr><td class="td1">Established Year</td><td>2015</td></tr>
  <tr><td class="td1">Total Employees</td><td>30-60</td></tr>
  <tr><td class="td1">Country/Region</td><td>China</td></tr>
  <tr><td class="td1">Business Type</td><td>Manufacturer</td></tr>
</table>
<h3 class="secondary-heading">Trading Information</h3>
<table>
  <tr><td class="td1">Total Revenue</td><td>1m</td></tr>
  <tr><td class="td1">Export Percentage</td><td>80%</td></tr>
</table>
<h3 class="secondary-heading">Contact Details</h3>
<p class="contact_p_txt1">Tony</p><p class="contact_p_txt2">Owner</p>
<table>
  <tr><td class="td1">Website</td><td><a href="javascript:void(0)" class="signup-popup">Show</a></td></tr>
</table>
</body></html>"#;

    // Copied (trimmed) from the real Dadal General Trading pages, Sep 2026.
    const DADAL_LISTING: &str = r##"<html><head>
<meta property="og:image" content="https://img2.tradewheel.com/uploads/images/products/5/2/0265297001790249005-wholesale-ethiopian.jpg.webp">
</head><body>
<div id="gallery_01" class="pd-thumbs"><ul>
<li class="active"><a class="product_secondary_images_container active video-thumb"><img class="btn_video" id="btn_video" src="https://img2.tradewheel.com/imagesv2/other/play.png.webp" /></a></li>
<li id="prod1_img_0"><a class="product_secondary_images_container " href="#" data-image="https://img2.tradewheel.com/uploads/images/products/5/2/0265297001790249005-wholesale-ethiopian-300-.jpg.webp" data-zoom-image="https://img2.tradewheel.com/uploads/images/products/5/2/0265297001790249005-wholesale-ethiopian.jpg"><img src="x-50-50.jpg.webp" /></a></li>
<li id="prod1_img_1"><a class="product_secondary_images_container" href="#" data-image="https://img2.tradewheel.com/uploads/images/products/5/2/0288224001790249005-wholesale-ethiopian-300-.jpg.webp" data-zoom-image="https://img2.tradewheel.com/uploads/images/products/5/2/0288224001790249005-wholesale-ethiopian.jpg"><img src="y-50-50.jpg.webp" /></a></li>
</ul></div>
<h1 class="pd-heading">Wholesale Ethiopian Natural Polished Wello Opal Gemstones</h1>
<div class="po-box">
  <div class="price-tag pd-price"> <span style='color:#3e3e3e;margin-right:10px;'>FOB Price</span>  60 - 80 USD / Carat </div>
</div>
<div class="po-box">
  <table class='table'>
    <tr><td>Quantity</td><td>305 - 305</td><td>305 - 305</td></tr>
    <tr><td>Price</td><td>USD 60</td><td>USD 60 ( wholesale only ))</td></tr>
  </table>
</div>
<div class="pd-attr-box"><table class="table attr_table">
  <tr><td class='c1'>MOQ</td><td class='c2'>305 Carat</td></tr>
  <tr><td class='c1'>Port</td><td class='c2'>Bole,Addis Ababa International Air port</td></tr>
  <tr><td class='c1'>Packaging</td><td class='c2'>Individually packed in protective pouches.</td></tr>
  <tr><td class='c1'>Lead Time</td><td class='c2'>3-5 days</td></tr>
</table></div>
<div class="wbg quick_details"><table class='table quick_details_table'>
  <tr><td class='c1'>Type</td><td class='c2' >Wello polished opal</td><td class='c1'>Origin</td><td class='c2' >Wello ethiopia</td></tr>
  <tr><td class='c1'>Supply type</td><td class='c2' >Wholesale</td>
</table></div>
<div class="wbg product-details-container">
  <h4>Product Details</h4>
  <p>
  Ethiopian Natural Polished Wello Opal Gemstones are genuine
  natural opals sourced from the Wello region of Ethiopia.
  </p>
</div>
<div class="comp-info">
  <a href="https://www.tradewheel.com/co/dadal-general-trading/" title="Dadal General Trading"><h2>Dadal General Trading</h2></a>
  <img src="https://img2.tradewheel.com/template1/images/icons/gold-txt1.png.webp" >
  <p class="address">Bole Africa Avenue - Addis Ababa - Ethiopia</p>
  <div class="bo-flag"> Ethiopia <i class="country-flag et" ></i></div>
</div>
</body></html>"##;

    const DADAL_PROFILE: &str = r##"<html><body>
<div class="comp_logo"><a href="https://www.tradewheel.com/co/dadal-general-trading/"><img src="https://img2.tradewheel.com/uploads/images/companies/logo/7/2/0996930001790236311--150-.jpg.webp" alt="Dadal General Trading"></a></div>
<h1 class="comp_heading">Dadal General Trading</h1>
<table class="table table-responsive">
  <tr><td class='td1' style="border-top:0;">Business Type </td><td style="border-top:0;"> Supplier</td></tr>
  <tr><td class='td1'>Company </td><td> Dadal General Trading</td></tr>
  <tr><td class='td1'>Main Products </td><td>
  Ethiopian wello polished opal gemstones                    </td></tr>
  <tr><td class='td1'>Established Year </td><td> 2026</td></tr>
  <tr><td class='td1'>Country/Region </td><td> Ethiopia</td></tr>
  <tr><td class='td1'>Total Employees </td><td> 1-10</td></tr>
  <tr><td class='td1'>Brochure </td><td><a href="https://img2.tradewheel.com/uploads/company/brochure/1807370/x.pdf">Download</a></td></tr>
</table>
<table class="table table-responsive">
  <tr><td class='td1' style="border-top:0;">Total Revenue </td><td style="border-top:0;">  </td></tr>
  <tr><td class='td1'>Export Percentage </td><td>  </td></tr>
</table>
<table class="table table-responsive">
  <tr><td style="border-top:0;"></td><td style="border-top:0;">
    <img id="m_img" class="rounded-circle" src="https://img2.tradewheel.com/uploads/images/users/avatar/2179700-1789840465-150-.jpg" alt="Ahmed Ali">
    <span class="contact_p_txt1">Ahmed Ali</span><br>
    <span class="contact_p_txt2">Sales and operation Mamager</span>
  </td></tr>
  <tr><td>Address:</td><td><p> Bole Africa Avenue - Addis Ababa - Ethiopia, Ethiopia</p></td></tr>
  <tr><td>Website:</td><td><a href='javascript:displayPopoup("Join Free","seller");' title='Show'>Show</a></td></tr>
</table>
</body></html>"##;

    #[test]
    fn dadal_listing_prices_and_fields() {
        let l = TradewheelScraper.parse_listing(DADAL_LISTING, "u");
        assert_eq!(
            l.title.as_deref(),
            Some("Wholesale Ethiopian Natural Polished Wello Opal Gemstones")
        );
        assert_eq!(l.fob_price.as_deref(), Some("60 - 80 USD / Carat"));
        assert_eq!(
            l.unit_price.as_deref(),
            Some("USD 60 / USD 60 ( wholesale only ))")
        );
        assert_eq!(
            l.minimum_order_quantity.as_deref(),
            Some("305 Carat"),
            "MOQ, not the tier quantity"
        );
        assert_eq!(
            l.preferred_port.as_deref(),
            Some("Bole,Addis Ababa International Air port")
        );
        assert_eq!(l.delivery_timeframe.as_deref(), Some("3-5 days"));
        assert!(l.packaging_details.is_some());
    }

    #[test]
    fn dadal_description_has_specs_and_text() {
        let d = TradewheelScraper
            .parse_listing(DADAL_LISTING, "u")
            .description
            .expect("description");
        assert!(d.starts_with(
            "Type: Wello polished opal; Origin: Wello ethiopia; Supply type: Wholesale"
        ));
        assert!(d.contains("sourced from the Wello region of Ethiopia"));
    }

    #[test]
    fn dadal_images_skip_the_video_thumb() {
        let l = TradewheelScraper.parse_listing(DADAL_LISTING, "u");
        assert_eq!(l.image_urls.len(), 2);
        assert!(
            l.image_urls
                .iter()
                .all(|u| u.ends_with("wholesale-ethiopian.jpg"))
        );
    }

    #[test]
    fn dadal_supplier() {
        let s = TradewheelScraper.enrich_from_company_profile(
            TradewheelScraper.parse_supplier(DADAL_LISTING, "u"),
            DADAL_PROFILE,
        );
        assert_eq!(s.company_name.as_deref(), Some("Dadal General Trading"));
        assert_eq!(s.country.as_deref(), Some("Ethiopia"));
        assert_eq!(s.contact_name.as_deref(), Some("Ahmed Ali"));
        assert_eq!(s.contact_phone, None);
        assert_eq!(s.website_url, None, "'Show' button is not a website");
        assert_eq!(s.year_established.as_deref(), Some("2026"));
        assert_eq!(s.employee_count.as_deref(), Some("1-10"));
        assert_eq!(s.sales_revenue, None, "empty cell");
        assert_eq!(s.export_percentage, None, "empty cell");
        assert!(s.logo_url.as_deref().unwrap().contains("/companies/logo/"));
        assert_eq!(s.badge_honorific.as_deref(), Some("Gold"));
        assert!(!s.platform_verified_badge);
        assert_eq!(
            s.company_description.as_deref(),
            Some(
                "Business type: Supplier. Main products: Ethiopian wello polished opal gemstones. Contact person's role: Sales and operation Mamager."
            )
        );
        assert_eq!(
            TradewheelScraper.company_key(DADAL_LISTING).as_deref(),
            Some("dadal-general-trading")
        );
    }

    // Trimmed from the real Shandong Topower listing (no badge, no price,
    // and TradeWheel's own auto-written text in the details box).
    const TOPOWER_LISTING: &str = r##"<html><body>
<div id="gallery_01" class="pd-thumbs"><ul>
<li id="prod1_img_0" class="active"><a class="product_secondary_images_container active" href="#" data-image="https://img2.tradewheel.com/uploads/images/products/1/2/ball0-0497026001721480843-300-.jpg.webp" data-zoom-image="https://img2.tradewheel.com/uploads/images/products/1/2/ball0-0497026001721480843.jpg"><img src="x.webp" /></a></li>
</ul></div>
<h1 class="pd-heading">High quality aluminium ceramics ball light weight grinding alumina ceramic ball manufacturers</h1>
<div class="po-box">
</div>
<div class="pd-attr-box"><table class="table attr_table">
</table></div>
<table class='table quick_details_table'>
  <tr><td class='c1'>Place of Origin</td><td class='c2' >China</td><td class='c1'>Material</td><td class='c2' >Alumina Ceramic</td></tr>
  <tr><td class='c1'>Al2O3(%)</td><td class='c2' >u226575-u226595</td><td class='c1'>Water Absorption(%)</td><td class='c2' >u22640.02</td></tr>
</table>
<div class="wbg product-details-container 2">
  <div style='line-height:1.5;'>
    Shandong Topower Pte Ltd offers premium quality High Quality Aluminium Ceramics Ball for B2B importers and distributors worldwide.
    <br><br>
    Tradewheel is a Business-to-Business platform that enables international traders dealing in Minerals & Metallurgy
  </div>
</div>
<div class="comp-info">
  <a href="https://www.tradewheel.com/co/shandong-topower-pte-ltd-1648974/" title="Shandong Topower Pte Ltd"><h2>Shandong Topower Pte Ltd</h2></a>
  <p class="address">Room 1704, Hongcheng Financial Center, Zibo, Shandong, China</p>
  <div class="bo-flag"> China <i class="country-flag cn" ></i></div>
</div>
</body></html>"##;

    #[test]
    fn topower_listing_without_badge_price_or_moq() {
        let s = TradewheelScraper.parse_supplier(TOPOWER_LISTING, "u");
        assert_eq!(s.company_name.as_deref(), Some("Shandong Topower Pte Ltd"));
        assert_eq!(s.country.as_deref(), Some("China"));
        assert_eq!(s.badge_honorific, None, "free member, no badge");
        assert_eq!(
            TradewheelScraper.company_key(TOPOWER_LISTING).as_deref(),
            Some("shandong-topower-pte-ltd-1648974")
        );

        let l = TradewheelScraper.parse_listing(TOPOWER_LISTING, "u");
        assert_eq!(l.unit_price, None);
        assert_eq!(l.fob_price, None);
        assert_eq!(l.minimum_order_quantity, None);
        assert_eq!(l.image_urls.len(), 1);
    }

    #[test]
    fn topower_description_is_specs_not_tradewheel_boilerplate() {
        let d = TradewheelScraper
            .parse_listing(TOPOWER_LISTING, "u")
            .description
            .expect("description");
        assert!(d.contains("Material: Alumina Ceramic"));
        assert!(
            d.contains("Al2O3(%): ≥75-≥95"),
            "escaped symbols restored: {}",
            d
        );
        assert!(d.contains("Water Absorption(%): ≤0.02"));
        assert!(
            !d.contains("Business-to-Business platform"),
            "TradeWheel's own text must not reach Claude"
        );
    }

    // Trimmed from the real Shandong Topower company page: free member,
    // no logo, most fields blank or "-".
    const TOPOWER_PROFILE: &str = r##"<html><body>
<div class="comp_logo">
</div>
<h1 class="comp_heading">Shandong Topower Pte Ltd</h1>
<div class="flag-container2">Free Member</div>
<table class="table table-responsive">
  <tr><td class='td1'>Business Type </td><td> -</td></tr>
  <tr><td class='td1'>Main Products </td><td>
  Refractory Brick,  Insulating Brick,  Ceramic Fiber Product,  Refractory Castable                    </td></tr>
  <tr><td class='td1'>Established Year </td><td> </td></tr>
  <tr><td class='td1'>City / State </td><td> , </td></tr>
  <tr><td class='td1'>Country/Region </td><td> China</td></tr>
  <tr><td class='td1'>Total Employees </td><td> </td></tr>
</table>
<table class="table table-responsive">
  <tr><td></td><td>
    <span class="contact_p_txt1">Mr. Marlene Ma</span><br>
    <span class="contact_p_txt2"></span>
  </td></tr>
  <tr><td>Website:</td><td><a href='javascript:displayPopoup("Join Free","seller");' title='Show'>Show</a></td></tr>
</table>
</body></html>"##;

    #[test]
    fn topower_mostly_empty_company_page() {
        let s = TradewheelScraper.enrich_from_company_profile(
            TradewheelScraper.parse_supplier(TOPOWER_LISTING, "u"),
            TOPOWER_PROFILE,
        );
        assert_eq!(s.contact_name.as_deref(), Some("Mr. Marlene Ma"));
        assert_eq!(
            s.year_established, None,
            "blank year is missing, not a value"
        );
        assert_eq!(s.employee_count, None);
        assert_eq!(s.sales_revenue, None);
        assert_eq!(s.logo_url, None);
        assert_eq!(s.website_url, None);
        assert_eq!(s.country.as_deref(), Some("China"));
        // "-" business type and the empty role are left out.
        assert_eq!(
            s.company_description.as_deref(),
            Some(
                "Main products: Refractory Brick, Insulating Brick, Ceramic Fiber Product, Refractory Castable."
            )
        );
    }

    fn full_supplier() -> B2bSupplierProfile {
        let s = TradewheelScraper;
        let supplier = s.parse_supplier(LISTING, "https://www.tradewheel.com/p/x-2402918/");
        s.enrich_from_company_profile(supplier, PROFILE)
    }

    #[test]
    fn listing_fields_are_read() {
        let l = TradewheelScraper.parse_listing(LISTING, "u");
        assert_eq!(
            l.title.as_deref(),
            Some("High-Quality Cotton Jacket with Calabash Pattern")
        );
        assert_eq!(l.minimum_order_quantity.as_deref(), Some("100 500"));
        assert_eq!(l.preferred_port.as_deref(), Some("China"));
        assert_eq!(l.packaging_details.as_deref(), Some("OEM"));
        assert_eq!(l.delivery_timeframe.as_deref(), Some("10-15 days"));
        assert_eq!(l.unit_price, None, "this listing has no price");
        assert_eq!(l.payment_type, None);
    }

    #[test]
    fn description_comes_from_the_detail_list() {
        let l = TradewheelScraper.parse_listing(LISTING, "u");
        let d = l.description.expect("description");
        assert!(d.contains("Place of Origin: China"));
        assert!(d.contains("Material: 100% Cotton"));
    }

    #[test]
    fn keeps_at_most_three_images() {
        let html = r#"<div class="pd-thumbs">
            <a data-zoom-image="https://www.tradewheel.com/uploads/a.jpg"></a>
            <a data-zoom-image="https://www.tradewheel.com/uploads/b.jpg"></a>
            <a data-zoom-image="https://www.tradewheel.com/uploads/c.jpg"></a>
            <a data-zoom-image="https://www.tradewheel.com/uploads/d.jpg"></a>
        </div>"#;
        let l = TradewheelScraper.parse_listing(html, "u");
        assert_eq!(l.image_urls.len(), 3);
        assert!(l.image_urls[2].ends_with("c.jpg"));
    }

    #[test]
    fn full_size_gallery_images_then_og_image() {
        let l = TradewheelScraper.parse_listing(LISTING, "u");
        assert_eq!(
            l.image_urls,
            vec![
                "https://www.tradewheel.com/uploads/p1.jpg".to_string(),
                "https://www.tradewheel.com/uploads/p2.jpg".to_string()
            ]
        );
        let og_only = r#"<html><head><meta property="og:image" content="https://www.tradewheel.com/uploads/o.jpg" /></head><body></body></html>"#;
        assert_eq!(
            TradewheelScraper.parse_listing(og_only, "u").image_urls,
            vec!["https://www.tradewheel.com/uploads/o.jpg".to_string()]
        );
    }

    #[test]
    fn supplier_fields_from_listing_and_company_page() {
        let s = full_supplier();
        assert_eq!(s.company_name.as_deref(), Some("Fengtai Clothing"));
        assert_eq!(s.country.as_deref(), Some("China"));
        assert_eq!(s.contact_name.as_deref(), Some("Tony"));
        assert_eq!(s.contact_phone, None, "TradeWheel shows no phone");
        assert_eq!(s.year_established.as_deref(), Some("2015"));
        assert_eq!(s.employee_count.as_deref(), Some("30-60"));
        assert_eq!(s.sales_revenue.as_deref(), Some("1m"));
        assert_eq!(s.export_percentage.as_deref(), Some("80%"));
        assert_eq!(
            s.logo_url.as_deref(),
            Some("https://www.tradewheel.com/uploads/logo-fengtai.jpg")
        );
    }

    #[test]
    fn hidden_website_button_is_not_a_website() {
        assert_eq!(full_supplier().website_url, None);
        let real = PROFILE.replace(
            r#"<a href="javascript:void(0)" class="signup-popup">Show</a>"#,
            "www.fengtaiclothing.com",
        );
        let s = TradewheelScraper
            .enrich_from_company_profile(TradewheelScraper.parse_supplier(LISTING, "u"), &real);
        assert_eq!(s.website_url.as_deref(), Some("www.fengtaiclothing.com"));
    }

    #[test]
    fn gold_is_a_tier_not_verification() {
        let s = TradewheelScraper.parse_supplier(LISTING, "u");
        assert!(!s.platform_verified_badge);
        assert_eq!(s.badge_honorific.as_deref(), Some("Gold"));

        let no_badge = LISTING.replace("gold-txt1", "flag");
        let s = TradewheelScraper.parse_supplier(&no_badge, "u");
        assert_eq!(s.badge_honorific, None);
    }

    #[test]
    fn company_key_is_the_company_page_slug() {
        assert_eq!(
            TradewheelScraper.company_key(LISTING).as_deref(),
            Some("fengtai-clothing")
        );
        assert_eq!(
            TradewheelScraper
                .extract_company_profile_url(LISTING)
                .as_deref(),
            Some("https://www.tradewheel.com/co/fengtai-clothing/")
        );
        assert_eq!(
            company_slug("https://www.tradewheel.com/co/abc-trading/?ref=1").as_deref(),
            Some("abc-trading")
        );
        assert_eq!(
            company_slug("https://www.tradewheel.com/p/x-2402918/"),
            None
        );
    }

    #[test]
    fn relative_company_link_is_made_absolute() {
        let html = r#"<div class="comp-info"><a href="/co/abc-trading/"><h2>ABC</h2></a></div>"#;
        assert_eq!(
            TradewheelScraper
                .extract_company_profile_url(html)
                .as_deref(),
            Some("https://www.tradewheel.com/co/abc-trading/")
        );
    }
}
