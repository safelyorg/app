use crate::services::b2b_scrapers::{B2bListingProfile, B2bScraper, B2bSupplierProfile};
use scraper::{Html, Selector};
use serde_json::Value;

pub struct AlibabaScraper;

/// Alibaba server-renders the whole product page's data into one
/// `window.detailData = {...};` script block. Some fields only exist
/// there and never appear as visible text - the supplier's contact
/// name is the main one. Returns `globalData.seller` if present.
fn extract_detail_data(html: &str) -> Option<Value> {
    let start = html.find("window.detailData")?;
    let rest = &html[start + "window.detailData".len()..];
    let rest = rest.trim_start().strip_prefix('=')?.trim_start();
    // Streaming parse: stops at the end of the JSON object and ignores
    // the `;` and whatever script follows it.
    let data: Value = serde_json::Deserializer::from_str(rest)
        .into_iter::<Value>()
        .next()?
        .ok()?;
    Some(data)
}

fn extract_detail_seller(html: &str) -> Option<Value> {
    extract_detail_data(html)?
        .get("globalData")?
        .get("seller")
        .cloned()
}

/// "Year founded" from the company card's performance fields in
/// window.detailData. Same value the visible overview panel shows, but
/// present in the raw HTML even when that panel's markup is not.
fn detail_year_founded(data: &Value) -> Option<String> {
    data.get("nodeMap")?
        .get("module_unifed_company_card")?
        .get("privateData")?
        .get("onlinePerformance")?
        .get("fields")?
        .as_array()?
        .iter()
        .find(|f| {
            f.get("title")
                .and_then(|t| t.as_str())
                .map_or(false, |t| t.trim_start().starts_with("Year founded"))
        })
        .and_then(|f| match f.get("value")? {
            Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
}

/// Product photo URLs from window.detailData (videos skipped).
fn detail_image_urls(data: &Value) -> Vec<String> {
    data.get("globalData")
        .and_then(|g| g.get("product"))
        .and_then(|p| p.get("mediaItems"))
        .and_then(|m| m.as_array())
        .map(|items| {
            items
                .iter()
                .filter(|m| m.get("group").and_then(|g| g.as_str()) == Some("photos"))
                .filter_map(|m| m.get("imageUrl")?.get("big")?.as_str())
                .map(absolute_url)
                .collect()
        })
        .unwrap_or_default()
}

fn json_str(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)?
        .as_str()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn absolute_url(url: &str) -> String {
    if url.starts_with("//") {
        format!("https:{}", url)
    } else {
        url.to_string()
    }
}

/// Normalises an Alibaba shop host to one canonical form, so every
/// source for it yields exactly the same key: lowercase, no "www.",
/// the mobile ".m.en." variant folded into ".en.", and only real
/// "<shop>.en.alibaba.com" hosts accepted (never www.alibaba.com
/// itself, which every page links to).
fn normalize_shop_host(raw: &str) -> Option<String> {
    let s = raw.trim().to_lowercase();
    let s = s
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("//");
    let host = s.split(|c| c == '/' || c == '?' || c == '#').next()?.trim();
    let host = host
        .trim_start_matches("www.")
        .replace(".m.en.alibaba.com", ".en.alibaba.com");
    let shop = host.strip_suffix(".en.alibaba.com")?;
    if shop.is_empty() || shop.contains('.') {
        return None;
    }
    Some(host)
}

fn extract_overview_field(document: &Html, label: &str) -> Option<String> {
    let button_sel = Selector::parse("button.id-cursor-default").ok()?;
    let div_sel = Selector::parse("div").ok()?;
    let buttons: Vec<_> = document.select(&button_sel).collect();
    for button in &buttons {
        let divs: Vec<_> = button.select(&div_sel).collect();
        let texts: Vec<String> = divs
            .iter()
            .map(|d| d.text().collect::<String>().trim().to_string())
            .collect();
        let label_matches = texts.iter().any(|t| t.starts_with(label));
        if label_matches {
            for d in &divs {
                if let Some(title) = d.value().attr("title") {
                    if !title.is_empty() {
                        return Some(title.to_string());
                    }
                }
            }
        }
    }
    None
}

impl B2bScraper for AlibabaScraper {
    fn matches_platform(&self, platform: &str) -> bool {
        platform == "alibaba"
    }

    fn parse_supplier(&self, html: &str, profile_url: &str) -> B2bSupplierProfile {
        let document = Html::parse_document(html);

        // Company name - the real, linked name inside the mini company card
        let company_name = select_attr_or_text(
            &document,
            "[data-testid='three-column-mini-company-card'] a.id-underline",
        );

        // Location + years + optional badge honorific all live in the
        // same, real row of small spans after the flag icon - e.g.
        // "CN", "10 yrs" (listing 1, no badge) or "Shenzhen, CN",
        // "8 yrs", "Trusted service provider" (listing 2, with badge).
        // Genuinely variable in length, so this walks every span and
        // classifies each by its real, actual content pattern, rather
        // than assuming a fixed position.
        let mut country = None;
        let mut badge_honorific = None;
        if let Ok(sel) =
            Selector::parse("[data-testid='three-column-mini-company-card'] .id-mt-1 span")
        {
            for el in document.select(&sel) {
                let text = el.text().collect::<String>().trim().to_string();
                if text.is_empty() {
                    continue;
                }
                if text.ends_with("yrs") {
                    // Years on Alibaba, not company age - deliberately
                    // not used as a founding year (see below).
                    continue;
                } else if text == "CN" || text.contains(", CN") || text.len() <= 4 {
                    // A bare country code, or "City, CC" - both count
                    // as the real, genuine location field.
                    if country.is_none() {
                        country = Some(text);
                    }
                } else {
                    // Anything else this specific, e.g. "Trusted
                    // service provider" or "Multispecialty Supplier",
                    // is a real, genuine supplier badge honorific.
                    badge_honorific = Some(text);
                }
            }
        }

        // The real, genuine verified badge - a small image with this
        // exact test ID, present only on badged suppliers (listings
        // 2-5), absent entirely for unbadged ones (listing 1).
        let platform_verified_badge = document
            .select(
                &Selector::parse("[data-testid='three-column-mini-company-card-verify-icon']")
                    .unwrap(),
            )
            .next()
            .is_some();

        // The company logo is the card image whose alt text is
        // "<Company name> logo". Taking the first <img> in the card was
        // wrong for badged suppliers: that one is the verified-badge
        // icon, not the logo.
        let seller_json = extract_detail_seller(html);
        let logo_url = select_attr(
            &document,
            "[data-testid='three-column-mini-company-card'] img[alt$=' logo']",
            "src",
        )
        .or_else(|| {
            seller_json
                .as_ref()
                .and_then(|s| json_str(s, "companyLogoFileUrlSmall"))
        })
        .map(|u| absolute_url(&u));

        let overview_year = extract_overview_field(&document, "Year founded");
        let sales_revenue = extract_overview_field(&document, "Online revenue");

        // Only a real founding year is used here. "N yrs" in the card is
        // how long the supplier has been on Alibaba, not how old the
        // company is, so it is no longer turned into a year. The company
        // profile page fills this in later where Alibaba shows it.
        let year_established = overview_year.or_else(|| {
            extract_detail_data(html)
                .as_ref()
                .and_then(detail_year_founded)
        });

        let contact_name = seller_json
            .as_ref()
            .and_then(|s| json_str(s, "contactName"));
        let employee_count = seller_json
            .as_ref()
            .and_then(|s| json_str(s, "employeesCount"));
        let sales_revenue = sales_revenue.or_else(|| {
            seller_json
                .as_ref()
                .and_then(|s| s.get("tradeHalfYear"))
                .and_then(|t| json_str(t, "ordAmt"))
                .map(|amt| format!("US$ {}", amt))
        });

        B2bSupplierProfile {
            company_name,
            logo_url,
            year_established,
            country,
            platform_verified_badge,
            employee_count,
            sales_revenue,
            export_percentage: None,
            profile_url: profile_url.to_string(),
            source_platform: "alibaba".to_string(),
            contact_name,
            contact_phone: None,
            badge_honorific,
            company_description: None,
            website_url: None,
        }
    }

    fn extract_company_profile_url(&self, listing_html: &str) -> Option<String> {
        let document = Html::parse_document(listing_html);
        let sel = Selector::parse("a").ok()?;
        for el in document.select(&sel) {
            if el.text().collect::<String>().trim() == "Company profile" {
                if let Some(href) = el.value().attr("href") {
                    return Some(href.to_string());
                }
            }
        }
        None
    }

    /// The company's shop address, e.g. "youhuan.en.alibaba.com" - the
    /// same on every product that company lists. Read from the page's
    /// hidden data first (globalData.seller.subDomain), then from the
    /// visible "Company profile" link, so it's found on either page
    /// layout.
    fn company_key(&self, listing_html: &str) -> Option<String> {
        extract_detail_seller(listing_html)
            .as_ref()
            .and_then(|s| json_str(s, "subDomain"))
            .and_then(|d| normalize_shop_host(&d))
            .or_else(|| {
                self.extract_company_profile_url(listing_html)
                    .and_then(|u| normalize_shop_host(&u))
            })
    }

    fn enrich_from_company_profile(
        &self,
        mut supplier: B2bSupplierProfile,
        profile_html: &str,
    ) -> B2bSupplierProfile {
        let document = Html::parse_document(profile_html);

        // Older, real template - a link (a.vd-item) whose text
        // includes "Total Employees:" with the real value in a
        // sibling ".con-text" span.
        if let Ok(sel) = Selector::parse("a.vd-item") {
            for el in document.select(&sel) {
                let text = el.text().collect::<String>();
                if text.contains("Total Employees:") {
                    if let Ok(value_sel) = Selector::parse(".con-text") {
                        if let Some(value_el) = el.select(&value_sel).next() {
                            let value = value_el.text().collect::<String>().trim().to_string();
                            if !value.is_empty() {
                                supplier.employee_count = Some(value);
                            }
                        }
                    }
                }
            }
        }

        // Newer, real "sp:"-prefixed template - a pair of sibling
        // spans, the first with the real label text "Total
        // employees", the second holding the real numeric value.
        // Only runs if the older template found nothing. Note: this
        // panel loads asynchronously on Alibaba's real page, so it
        // is genuinely absent from a plain ScraperAPI fetch even when
        // this selector is otherwise correct - a known, current gap.
        if supplier.employee_count.is_none() {
            if let Ok(div_sel) = Selector::parse("div") {
                for div in document.select(&div_sel) {
                    let class = div.value().attr("class").unwrap_or("");
                    if class.contains("items-start") && class.contains("justify-between") {
                        if let Ok(span_sel) = Selector::parse("span") {
                            let spans: Vec<_> = div.select(&span_sel).collect();
                            if spans.len() == 2 {
                                let label = spans[0].text().collect::<String>().trim().to_string();
                                if label == "Total employees" {
                                    let value =
                                        spans[1].text().collect::<String>().trim().to_string();
                                    if !value.is_empty() {
                                        supplier.employee_count = Some(value);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Company pages ship their data as URL-encoded JSON in each
        // module's `module-data` attribute; the visible tables are drawn
        // from it by JavaScript after load. A plain server fetch
        // (no rendering) only gets the attribute, so this is the primary
        // source. The rendered <td> table below is only a fallback for
        // HTML that was already rendered in a browser.
        let module_data = collect_module_data(&document);
        let field = |key: &str| -> Option<String> {
            module_data.iter().find_map(|d| {
                let v = d.get(key)?;
                let v = v.get("value").unwrap_or(v);
                match v {
                    Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                }
            })
        };

        let year = field("companyEstablishedYear")
            .or_else(|| extract_profile_table_field(&document, "Year established"));
        if let Some(year) = year {
            // The profile page states the real founding year; it wins
            // over anything taken from the listing page.
            supplier.year_established = Some(year);
        }
        if supplier.employee_count.is_none() {
            supplier.employee_count = field("companyNumberOfEmployees")
                .or_else(|| extract_profile_table_field(&document, "Total employees"));
        }
        if supplier.company_description.is_none() {
            supplier.company_description = field("companyDescription").map(|d| {
                d.replace("<br>", "\n")
                    .replace("<br/>", "\n")
                    .replace("<br />", "\n")
            });
        }

        supplier
    }

    fn parse_listing(&self, html: &str, listing_url: &str) -> B2bListingProfile {
        let document = Html::parse_document(html);

        let detail = extract_detail_data(html);
        // Title and description fall back to window.detailData when the
        // visible markup is missing (same values, just not drawn).
        let title = select_attr_or_text(&document, "h1[title]").or_else(|| {
            detail
                .as_ref()
                .and_then(|d| d.pointer("/globalData/product"))
                .and_then(|p| json_str(p, "subject"))
        });
        let description = build_description_from_attributes(&document)
            .or_else(|| detail.as_ref().and_then(detail_description));
        // "Packaging and delivery" (selling unit, package size, gross
        // weight) is only in window.detailData.
        let packaging_details = detail.as_ref().and_then(detail_packaging);
        let mut image_urls = extract_image_urls(&document);
        if image_urls.is_empty() {
            if let Some(data) = detail.as_ref() {
                image_urls = detail_image_urls(data);
            }
        }

        // Try ladder pricing first (multiple tiers); fall back to
        // range pricing (one price + separate MOQ) if that's genuinely
        // what this listing uses instead.
        let (mut unit_price, mut minimum_order_quantity) = extract_price(&document);
        let mut delivery_timeframe = extract_lead_time(&document);

        // Some layouts (e.g. logistics/service listings, or a page whose
        // price block is drawn by JavaScript) have no price, minimum
        // order or lead-time markup - the same values are in the page's
        // window.detailData script, so they are read from there. These
        // only fill gaps; a value found on the visible page always wins.
        if let Some(data) = detail.as_ref() {
            if unit_price.is_none() {
                unit_price = detail_price(data);
            }
            if minimum_order_quantity.is_none() {
                minimum_order_quantity = detail_moq(data);
            }
            if delivery_timeframe.is_none() {
                delivery_timeframe = detail_lead_time(data);
            }
        }

        B2bListingProfile {
            title,
            description,
            image_urls,
            unit_price,
            fob_price: None,
            minimum_order_quantity,
            payment_type: None,
            preferred_port: None,
            reference: None,
            production_capacity: None,
            delivery_timeframe,
            incoterms: None,
            packaging_details,
            listing_url: listing_url.to_string(),
            source_platform: "alibaba".to_string(),
        }
    }
}

/// Every `module-data` attribute on the page, percent-decoded and parsed,
/// reduced to its `mds.moduleData.data` object. Modules that fail to
/// decode are skipped, never fatal.
fn collect_module_data(document: &Html) -> Vec<Value> {
    let Ok(sel) = Selector::parse("[module-data]") else {
        return Vec::new();
    };
    document
        .select(&sel)
        .filter_map(|el| el.value().attr("module-data"))
        .filter_map(|raw| serde_json::from_str::<Value>(&percent_decode(raw)).ok())
        .filter_map(|v| v.get("mds")?.get("moduleData")?.get("data").cloned())
        .filter(|v| v.is_object())
        .collect()
}

/// Minimal %XX decoder. Invalid sequences are kept as-is. '+' is left
/// alone on purpose: Alibaba encodes spaces as %20 or leaves them raw.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            // Work on bytes, not &str slices: slicing a str mid-character
            // would panic on non-ASCII input.
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn extract_profile_table_field(document: &Html, label: &str) -> Option<String> {
    let title_sel = Selector::parse("td.field-title").ok()?;
    let value_sel = Selector::parse(".content-value").ok()?;
    for title in document.select(&title_sel) {
        if !title
            .text()
            .collect::<String>()
            .trim()
            .eq_ignore_ascii_case(label)
        {
            continue;
        }
        let value_td = title
            .next_siblings()
            .filter_map(scraper::ElementRef::wrap)
            .next()?;
        let value = value_td
            .select(&value_sel)
            .next()
            .map(|v| v.text().collect::<String>().trim().to_string())
            .filter(|v| !v.is_empty());
        if value.is_some() {
            return value;
        }
    }
    None
}

fn extract_price(document: &Html) -> (Option<String>, Option<String>) {
    // Current markup (2026): tiers live in
    // [data-testid='pc-purchase-price-tiers'], one
    // [data-testid='pc-purchase-price-tier'] per tier holding the price
    // and its quantity range, e.g. "US$250" + "5-99 cartons". Alibaba
    // renders the whole block twice (main + sticky panel), so only the
    // first block is read.
    if let (Ok(block_sel), Ok(tier_sel), Ok(price_sel)) = (
        Selector::parse("[data-testid='pc-purchase-price-tiers']"),
        Selector::parse("[data-testid='pc-purchase-price-tier']"),
        Selector::parse("[data-testid='pc-purchase-price-tier-current']"),
    ) {
        if let Some(block) = document.select(&block_sel).next() {
            let mut tiers = Vec::new();
            let mut first_range = None;
            for tier in block.select(&tier_sel) {
                let price = tier
                    .select(&price_sel)
                    .next()
                    .map(|p| p.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();
                let full = tier.text().collect::<String>();
                let range = full
                    .trim()
                    .strip_prefix(price.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if price.is_empty() {
                    continue;
                }
                if first_range.is_none() && !range.is_empty() {
                    first_range = Some(range.clone());
                }
                tiers.push(if range.is_empty() {
                    price
                } else {
                    format!("{} ({})", price, range)
                });
            }
            if !tiers.is_empty() {
                let moq = first_range.as_deref().and_then(moq_from_range);
                return (Some(tiers.join(" | ")), moq);
            }
        }
    }

    // Current markup, single price: [data-testid='pc-purchase-current-price']
    // with the MOQ in [data-testid='pc-purchase-effective-moq'].
    if let Ok(sel) = Selector::parse("[data-testid='pc-purchase-current-price']") {
        if let Some(el) = document.select(&sel).next() {
            let price = el.text().collect::<String>().trim().to_string();
            if !price.is_empty() {
                let moq = Selector::parse("[data-testid='pc-purchase-effective-moq']")
                    .ok()
                    .and_then(|s| document.select(&s).next())
                    .map(|m| m.text().collect::<String>())
                    .map(|t| t.replace("Minimum order quantity:", "").trim().to_string())
                    .filter(|t| !t.is_empty());
                return (Some(price), moq);
            }
        }
    }

    // Older markup, kept as a fallback.
    // Ladder pricing - real, multiple price tiers.
    if let Ok(sel) = Selector::parse("[data-testid='ladder-price'] .price-item") {
        let tiers: Vec<String> = document
            .select(&sel)
            .filter_map(|el| {
                let text = el.text().collect::<String>();
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect();
        if !tiers.is_empty() {
            return (Some(tiers.join(" | ")), None);
        }
    }

    // Range pricing - one price, MOQ stated separately.
    if let Ok(sel) = Selector::parse("[data-testid='range-price']") {
        if let Some(el) = document.select(&sel).next() {
            let text = el.text().collect::<String>();
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                // The MOQ is embedded in the same block's text, e.g.
                // "US$0.05-0.30 Minimum order quantity: 100 pieces" -
                // split it out into its own, real, separate field.
                let moq = trimmed
                    .split("Minimum order quantity:")
                    .nth(1)
                    .map(|s| s.trim().to_string());
                let price = trimmed
                    .split("Minimum order quantity:")
                    .next()
                    .map(|s| s.trim().to_string());
                return (price, moq);
            }
        }
    }

    (None, None)
}

/// The attribute groups of window.detailData: the first (untitled) group
/// is "Key attributes", the "Packaging and delivery" group is packaging.
fn detail_attribute_groups(data: &Value) -> Vec<(String, Vec<(String, String)>)> {
    let Some(groups) = data
        .pointer("/nodeMap/module_sorted_attribute/privateData/productSortedProperties")
        .and_then(|g| g.as_array())
    else {
        return Vec::new();
    };
    groups
        .iter()
        .map(|g| {
            let title = json_str(g, "title").unwrap_or_default();
            let pairs = g
                .get("attributeList")
                .and_then(|l| l.as_array())
                .map(|list| {
                    list.iter()
                        .filter_map(|a| Some((json_str(a, "attribute")?, json_str(a, "value")?)))
                        .collect()
                })
                .unwrap_or_default();
            (title, pairs)
        })
        .collect()
}

fn join_pairs(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}: {}", k, v))
        .collect::<Vec<_>>()
        .join(". ")
}

/// "Selling Units: Single item. Single package size: 16X9X8 cm. Single
/// gross weight: 0.06 kg"
fn detail_packaging(data: &Value) -> Option<String> {
    detail_attribute_groups(data)
        .into_iter()
        .find(|(title, _)| title.eq_ignore_ascii_case("Packaging and delivery"))
        .map(|(_, pairs)| join_pairs(&pairs))
        .filter(|s| !s.is_empty())
}

/// Key attributes (same "Name: value" format as the visible grid), then
/// the seller's own product description lines.
fn detail_description(data: &Value) -> Option<String> {
    let mut parts = Vec::new();
    let attributes: Vec<(String, String)> = detail_attribute_groups(data)
        .into_iter()
        .filter(|(title, _)| !title.eq_ignore_ascii_case("Packaging and delivery"))
        .flat_map(|(_, pairs)| pairs)
        .collect();
    if !attributes.is_empty() {
        parts.push(join_pairs(&attributes));
    }
    if let Some(details) = data
        .pointer("/nodeMap/module_description/privateData/productDescription/details")
        .and_then(|d| d.as_array())
    {
        let lines: Vec<String> = details.iter().filter_map(|d| json_str(d, "text")).collect();
        if !lines.is_empty() {
            parts.push(lines.join("\n"));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

/// The sample/price module of window.detailData.
fn detail_sample_module(data: &Value) -> Option<&Value> {
    data.get("nodeMap")?
        .get("module_sample_new")?
        .get("privateData")
}

/// "US$0.02 (≥1 kilograms)", tiers joined with " | " - the same format
/// as the visible price tiers.
fn detail_price(data: &Value) -> Option<String> {
    let list = detail_sample_module(data)?.get("priceList")?.as_array()?;
    let tiers: Vec<String> = list
        .iter()
        .filter_map(|t| {
            let price = t.get("formatPrice")?.as_str()?.trim();
            if price.is_empty() {
                return None;
            }
            Some(
                match t
                    .get("formatLadder")
                    .and_then(|l| l.as_str())
                    .map(str::trim)
                {
                    Some(l) if !l.is_empty() => format!("{} ({})", price, l),
                    _ => price.to_string(),
                },
            )
        })
        .collect();
    if tiers.is_empty() {
        None
    } else {
        Some(tiers.join(" | "))
    }
}

/// "1 kilogram" from the minimum-order block of window.detailData.
fn detail_moq(data: &Value) -> Option<String> {
    detail_sample_module(data)?
        .get("orderQuantity")?
        .get("minOrder")?
        .get("formatMinOrderQuantity")?
        .as_str()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Lead time from window.detailData, e.g. "1 - 1000 kilogram: 3 days".
fn detail_lead_time(data: &Value) -> Option<String> {
    let global = data.get("globalData")?;
    let list = global
        .get("trade")?
        .get("leadTimeInfo")?
        .get("ladderPeriodList")?
        .as_array()?;
    let unit = global
        .get("product")
        .and_then(|p| p.get("price").or_else(|| p.get("customPrice")))
        .and_then(|p| p.get("unit"))
        .and_then(|u| u.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let parts: Vec<String> = list
        .iter()
        .filter_map(|t| {
            let days = t.get("processPeriod")?.as_i64()?;
            let min = t.get("minQuantity").and_then(|v| v.as_i64());
            let max = t
                .get("maxQuantity")
                .and_then(|v| v.as_i64())
                .filter(|m| *m > 0);
            let range = match (min, max) {
                (Some(a), Some(b)) => format!("{} - {}", a, b),
                (Some(a), None) => format!("> {}", a),
                _ => return Some(format!("{} days", days)),
            };
            let qty = if unit.is_empty() {
                range
            } else {
                format!("{} {}", range, unit)
            };
            Some(format!("{}: {} days", qty, days))
        })
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

/// The "Lead time" table: a quantity row and a days row, e.g.
/// "Quantity (pieces) | 1 - 2,000 | 2,001 - 5,000 | > 5,000" and
/// "Lead time (days) | 32 | 35 | To be negotiated" ->
/// "1 - 2,000 pieces: 32 days; 2,001 - 5,000 pieces: 35 days;
/// > 5,000 pieces: To be negotiated".
fn extract_lead_time(document: &Html) -> Option<String> {
    let table_sel = Selector::parse(".lead-list table").ok()?;
    let row_sel = Selector::parse("tr").ok()?;
    let cell_sel = Selector::parse("td, th").ok()?;
    let table = document.select(&table_sel).next()?;
    let rows: Vec<Vec<String>> = table
        .select(&row_sel)
        .map(|r| {
            r.select(&cell_sel)
                .map(|c| {
                    c.text()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect()
        })
        .collect();
    let quantity = rows.iter().find(|r| {
        r.first()
            .map_or(false, |l| l.to_lowercase().starts_with("quantity"))
    })?;
    let days = rows.iter().find(|r| {
        r.first()
            .map_or(false, |l| l.to_lowercase().starts_with("lead time"))
    })?;
    // "Quantity (pieces)" -> "pieces"
    let unit = quantity[0]
        .split('(')
        .nth(1)
        .and_then(|u| u.split(')').next())
        .unwrap_or("")
        .trim()
        .to_string();
    let parts: Vec<String> = quantity
        .iter()
        .skip(1)
        .zip(days.iter().skip(1))
        .filter(|(q, d)| !q.is_empty() && !d.is_empty())
        .map(|(q, d)| {
            let qty = if unit.is_empty() {
                q.clone()
            } else {
                format!("{} {}", q, unit)
            };
            let when = if d.chars().all(|c| c.is_ascii_digit()) {
                format!("{} days", d)
            } else {
                d.clone()
            };
            format!("{}: {}", qty, when)
        })
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

/// "5-99 cartons" -> "5 cartons"; "≥500 pieces" -> "500 pieces".
fn moq_from_range(range: &str) -> Option<String> {
    let trimmed = range.trim().trim_start_matches('≥').trim();
    let qty: String = trimmed
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ',')
        .collect();
    if qty.is_empty() {
        return None;
    }
    let unit = trimmed.split_whitespace().last().unwrap_or("");
    if unit.is_empty() || unit.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        Some(qty)
    } else {
        Some(format!("{} {}", qty, unit))
    }
}

fn build_description_from_attributes(document: &Html) -> Option<String> {
    // Alibaba doesn't have one, single free-text description block -
    // its real content is the "Key attributes" grid (Material,
    // Gender, Place of Origin, etc.). Genuinely combining these into
    // one readable string gives Claude the same, real substance a
    // free-text description would.
    let mut parts = Vec::new();
    if let Ok(row_sel) = Selector::parse("[data-testid='three-column-key-attributes-row']") {
        for row in document.select(&row_sel) {
            if let Ok(pair_sel) = Selector::parse("p") {
                let texts: Vec<String> = row
                    .select(&pair_sel)
                    .map(|p| p.text().collect::<String>().trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
                for chunk in texts.chunks(2) {
                    if chunk.len() == 2 {
                        parts.push(format!("{}: {}", chunk[0], chunk[1]));
                    }
                }
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(". "))
    }
}

fn extract_image_urls(document: &Html) -> Vec<String> {
    let mut urls = Vec::new();
    if let Ok(sel) = Selector::parse("[data-testid='main-image-thumbnail']") {
        for el in document.select(&sel) {
            if let Some(style) = el.value().attr("style") {
                if let Some(start) = style.find("url(\"") {
                    let rest = &style[start + 5..];
                    if let Some(end) = rest.find('"') {
                        let url = &rest[..end];
                        if !url.contains("icon-play") {
                            urls.push(if url.starts_with("//") {
                                format!("https:{}", url)
                            } else {
                                url.to_string()
                            });
                        }
                    }
                }
            }
        }
    }
    urls
}

fn select_attr_or_text(document: &Html, selector: &str) -> Option<String> {
    let sel = Selector::parse(selector).ok()?;
    document.select(&sel).next().and_then(|el| {
        el.value().attr("title").map(|s| s.to_string()).or_else(|| {
            let text = el.text().collect::<String>().trim().to_string();
            if text.is_empty() { None } else { Some(text) }
        })
    })
}

fn select_attr(document: &Html, selector: &str, attr: &str) -> Option<String> {
    let sel = Selector::parse(selector).ok()?;
    document
        .select(&sel)
        .next()
        .and_then(|el| el.value().attr(attr))
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Snippets below are trimmed from real Alibaba pages (Sep 2026).

    fn product_page(card: &str, price: &str, script: &str) -> String {
        format!(
            "<!DOCTYPE html><html><head></head><body>\
             <h1 title=\"Test Product\">Test Product</h1>\
             <div data-testid=\"three-column-mini-company-card\">{card}</div>\
             {price}{script}</body></html>"
        )
    }

    const BADGED_CARD: &str = r#"<img data-testid="three-column-mini-company-card-verify-icon" src="//s.alicdn.com/@img/imgextra/i2/O1CN01PkPfW11Tz4AyzYFnc_!!6000000002452-2-tps-145-42.png" alt="">
    <img src="//s.alicdn.com/@sc04/kf/H8eabaf2a5a4a441eb3af28c88c779b09z.png" alt="Ningbo Youhuan Automation Technology Co., Ltd. logo">
    <a class="id-underline" title="Ningbo Youhuan Automation Technology Co., Ltd.">Ningbo Youhuan Automation Technology Co., Ltd.</a>
    <div class="id-mt-1"><span>Ningbo, CN</span><span>7 yrs</span><span>Multispecialty Supplier</span></div>"#;

    const TIERS: &str = r#"<div data-testid="pc-purchase-price"><div data-testid="pc-purchase-price-tiers">
    <div data-testid="pc-purchase-price-tier"><div data-testid="pc-purchase-price-tier-prices"><strong data-testid="pc-purchase-price-tier-current">US$250</strong></div><div>5-99 cartons</div></div>
    <div data-testid="pc-purchase-price-tier"><div data-testid="pc-purchase-price-tier-prices"><strong data-testid="pc-purchase-price-tier-current">US$240</strong></div><div>100-499 cartons</div></div>
    <div data-testid="pc-purchase-price-tier"><div data-testid="pc-purchase-price-tier-prices"><strong data-testid="pc-purchase-price-tier-current">US$230</strong></div><div>≥500 cartons</div></div>
    </div></div>
    <div data-testid="pc-purchase-price"><div data-testid="pc-purchase-price-tiers">
    <div data-testid="pc-purchase-price-tier"><div data-testid="pc-purchase-price-tier-prices"><strong data-testid="pc-purchase-price-tier-current">US$250</strong></div><div>5-99 cartons</div></div>
    </div></div>"#;

    const SINGLE_PRICE: &str = r#"<div data-testid="pc-purchase-price"><div><div><span data-testid="pc-purchase-current-price">US$0.73</span></div><div data-testid="pc-purchase-effective-moq">Minimum order quantity: 2 pieces</div></div></div>"#;

    const DETAIL_SCRIPT: &str = r#"<script>
    window.__global_config__ = { deviceType: 'pc' };
    window.detailData = {"globalData":{"seller":{"companyName":"Ningbo Youhuan Automation Technology Co., Ltd.","contactName":"Mr. Xu","employeesCount":"11-50","companyLogoFileUrlSmall":"https://sc04.alicdn.com/kf/H8eabaf2a5a4a441eb3af28c88c779b09z.png_80x80.png","tradeHalfYear":{"ordAmt":"1,900,000+","ordAmt6m":1970280.07,"ordCnt6m":87}}},"nodeMap":{}};
    window.somethingElse = 1;
    </script>"#;

    #[test]
    fn contact_name_comes_from_detail_data_script() {
        let html = product_page(BADGED_CARD, TIERS, DETAIL_SCRIPT);
        let s =
            AlibabaScraper.parse_supplier(&html, "https://www.alibaba.com/product-detail/x.html");
        assert_eq!(s.contact_name.as_deref(), Some("Mr. Xu"));
    }

    #[test]
    fn contact_name_is_none_without_detail_data() {
        let html = product_page(BADGED_CARD, TIERS, "");
        let s = AlibabaScraper.parse_supplier(&html, "u");
        assert_eq!(s.contact_name, None);
    }

    #[test]
    fn blank_contact_name_is_treated_as_missing() {
        let script = r#"<script>window.detailData = {"globalData":{"seller":{"contactName":"  "}}};</script>"#;
        let s = AlibabaScraper.parse_supplier(&product_page(BADGED_CARD, TIERS, script), "u");
        assert_eq!(s.contact_name, None);
    }

    #[test]
    fn malformed_detail_data_does_not_panic() {
        let script =
            r#"<script>window.detailData = {"globalData":{"seller":{"contactName":</script>"#;
        let s = AlibabaScraper.parse_supplier(&product_page(BADGED_CARD, TIERS, script), "u");
        assert_eq!(s.contact_name, None);
        assert_eq!(
            s.company_name.as_deref(),
            Some("Ningbo Youhuan Automation Technology Co., Ltd.")
        );
    }

    #[test]
    fn logo_is_company_logo_not_verified_badge() {
        let s = AlibabaScraper.parse_supplier(&product_page(BADGED_CARD, TIERS, ""), "u");
        assert_eq!(
            s.logo_url.as_deref(),
            Some("https://s.alicdn.com/@sc04/kf/H8eabaf2a5a4a441eb3af28c88c779b09z.png")
        );
        assert!(s.platform_verified_badge);
    }

    #[test]
    fn years_on_alibaba_is_not_used_as_founding_year() {
        let s = AlibabaScraper.parse_supplier(&product_page(BADGED_CARD, TIERS, ""), "u");
        assert_eq!(s.year_established, None);
    }

    #[test]
    fn employees_and_revenue_fall_back_to_detail_data() {
        let s =
            AlibabaScraper.parse_supplier(&product_page(BADGED_CARD, TIERS, DETAIL_SCRIPT), "u");
        assert_eq!(s.employee_count.as_deref(), Some("11-50"));
        assert_eq!(s.sales_revenue.as_deref(), Some("US$ 1,900,000+"));
    }

    #[test]
    fn tiered_price_reads_first_block_only_and_derives_moq() {
        let l = AlibabaScraper.parse_listing(&product_page(BADGED_CARD, TIERS, ""), "u");
        assert_eq!(
            l.unit_price.as_deref(),
            Some("US$250 (5-99 cartons) | US$240 (100-499 cartons) | US$230 (≥500 cartons)")
        );
        assert_eq!(l.minimum_order_quantity.as_deref(), Some("5 cartons"));
    }

    #[test]
    fn single_price_reads_effective_moq() {
        let l = AlibabaScraper.parse_listing(&product_page(BADGED_CARD, SINGLE_PRICE, ""), "u");
        assert_eq!(l.unit_price.as_deref(), Some("US$0.73"));
        assert_eq!(l.minimum_order_quantity.as_deref(), Some("2 pieces"));
    }

    const PROFILE_TABLE: &str = r#"<!DOCTYPE html><html><body><table><tbody>
    <tr><td class="field-title">Business type</td><td class="field-content-wrap"><div class="field-content"><div class="content-value" title="Manufacturer, Trading Company">Manufacturer, Trading Company</div></div></td>
    <td class="field-title">Country / Region</td><td class="field-content-wrap"><div class="field-content"><div class="content-value">Guangdong, China</div></div></td></tr>
    <tr><td class="field-title">Total employees</td><td class="field-content-wrap"><div class="field-content"><div class="content-value">11 - 50 People</div></div></td>
    <td class="field-title">Year established</td><td class="field-content-wrap"><div class="field-content"><div class="content-value">2013</div></div></td></tr>
    </tbody></table></body></html>"#;

    #[test]
    fn profile_table_fills_year_and_employees() {
        let supplier = AlibabaScraper.parse_supplier(&product_page(BADGED_CARD, TIERS, ""), "u");
        let s = AlibabaScraper.enrich_from_company_profile(supplier, PROFILE_TABLE);
        assert_eq!(s.year_established.as_deref(), Some("2013"));
        assert_eq!(s.employee_count.as_deref(), Some("11 - 50 People"));
    }

    #[test]
    fn profile_table_year_overrides_listing_year_but_keeps_listing_employees() {
        let card = BADGED_CARD;
        let html = product_page(card, TIERS, DETAIL_SCRIPT);
        let mut supplier = AlibabaScraper.parse_supplier(&html, "u");
        supplier.year_established = Some("2019".into());
        let s = AlibabaScraper.enrich_from_company_profile(supplier, PROFILE_TABLE);
        assert_eq!(s.year_established.as_deref(), Some("2013"));
        assert_eq!(s.employee_count.as_deref(), Some("11-50"));
    }

    // Trimmed from the raw (unrendered) source of a real company page:
    // the data lives only in the URL-encoded module-data attribute.
    const PROFILE_MODULE_DATA: &str = r#"<!DOCTYPE html><html><body>
<div module-name="icbu-pc-cpCompanyOverview" module-data='%7B%22mds%22%3A%7B%22moduleData%22%3A%7B%22data%22%3A%7B%22companyEstablishedYear%22%3A%7B%22authType%22%3A%22onsite%22%2C%22title%22%3A%22Year Established%22%2C%22value%22%3A2013%7D%2C%22companyNumberOfEmployees%22%3A%7B%22title%22%3A%22Total Employees%22%2C%22value%22%3A%2211 - 50 People%22%7D%2C%22companyDescription%22%3A%7B%22title%22%3A%22Company Description%22%2C%22value%22%3A%22Founded in 2013%2C the company is a SSD%2F memory manufacturer.%3Cbr%3E%3Cbr%3EBrand %5C%22Wicgtyp%5C%22 %E2%89%A4 FCC%2C CE.%22%7D%7D%7D%7D%7D' render="false"></div>
<div module-name="broken" module-data='%7B%22mds%22%3A%7Bnot json'></div>
</body></html>"#;

    #[test]
    fn profile_module_data_fills_year_employees_and_description() {
        let supplier = AlibabaScraper.parse_supplier(&product_page(BADGED_CARD, TIERS, ""), "u");
        let s = AlibabaScraper.enrich_from_company_profile(supplier, PROFILE_MODULE_DATA);
        assert_eq!(s.year_established.as_deref(), Some("2013"));
        assert_eq!(s.employee_count.as_deref(), Some("11 - 50 People"));
        let d = s.company_description.unwrap();
        assert!(d.starts_with("Founded in 2013"));
        assert!(d.contains("\n\nBrand \"Wicgtyp\" ≤ FCC"));
        assert!(!d.contains("<br>"));
    }

    #[test]
    fn percent_decode_handles_utf8_and_bad_sequences() {
        assert_eq!(percent_decode("%E2%89%A42h"), "≤2h");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz≤%4"), "%zz≤%4");
    }

    const DETAIL_YEAR_AND_IMAGES: &str = r#"<script>window.detailData = {"globalData":{"seller":{},"product":{"mediaItems":[{"group":"photos","imageUrl":{"big":"https://sc04.alicdn.com/kf/A.jpg"}},{"group":"video","imageUrl":{"big":"https://x/v.jpg"}},{"group":"photos","imageUrl":{"big":"//sc04.alicdn.com/kf/B.jpg"}}]}},"nodeMap":{"module_unifed_company_card":{"privateData":{"onlinePerformance":{"fields":[{"title":"Response time","value":"≤2h"},{"title":"Year founded","value":"2019"}]}}}}};</script>"#;

    #[test]
    fn year_falls_back_to_detail_data_when_overview_panel_missing() {
        let s = AlibabaScraper.parse_supplier(
            &product_page(BADGED_CARD, TIERS, DETAIL_YEAR_AND_IMAGES),
            "u",
        );
        assert_eq!(s.year_established.as_deref(), Some("2019"));
    }

    #[test]
    fn images_fall_back_to_detail_data_photos_only() {
        let l = AlibabaScraper.parse_listing(
            &product_page(BADGED_CARD, TIERS, DETAIL_YEAR_AND_IMAGES),
            "u",
        );
        assert_eq!(
            l.image_urls,
            vec![
                "https://sc04.alicdn.com/kf/A.jpg",
                "https://sc04.alicdn.com/kf/B.jpg"
            ]
        );
    }

    #[test]
    fn company_key_from_detail_data() {
        let script = r#"<script>window.detailData = {"globalData":{"seller":{"subDomain":"youhuan.en.alibaba.com"}}};</script>"#;
        let html = product_page(BADGED_CARD, TIERS, script);
        assert_eq!(
            AlibabaScraper.company_key(&html).as_deref(),
            Some("youhuan.en.alibaba.com")
        );
    }

    #[test]
    fn company_key_falls_back_to_company_profile_link() {
        let link = r#"<a href="https://youhuan.en.alibaba.com/company_profile.html?spm=a2700.details.0.0">Company profile</a>"#;
        let html = product_page(BADGED_CARD, TIERS, link);
        assert_eq!(
            AlibabaScraper.company_key(&html).as_deref(),
            Some("youhuan.en.alibaba.com")
        );
    }

    #[test]
    fn company_key_is_same_from_both_sources() {
        assert_eq!(
            normalize_shop_host("youhuan.en.alibaba.com"),
            normalize_shop_host("https://YouHuan.m.en.alibaba.com/company_profile.html?x=1")
        );
    }

    #[test]
    fn company_key_rejects_non_shop_hosts() {
        assert_eq!(
            normalize_shop_host("https://www.alibaba.com/product-detail/x.html"),
            None
        );
        assert_eq!(
            normalize_shop_host("https://evil.com/youhuan.en.alibaba.com"),
            None
        );
        assert_eq!(normalize_shop_host("a.b.en.alibaba.com"), None);
        assert_eq!(normalize_shop_host(""), None);
    }

    #[test]
    fn company_key_none_when_page_has_neither() {
        assert_eq!(
            AlibabaScraper.company_key(&product_page(BADGED_CARD, TIERS, "")),
            None
        );
    }

    #[test]
    fn lead_time_table_becomes_delivery_timeframe() {
        let html = r#"<div class="lead-list"><table><tbody><tr><td>Quantity (pieces)</td><td>1 - 2,000</td><td>2,001 - 5,000</td><td> &gt; 5,000 </td></tr><tr><td>Lead time (days)</td><td>32</td><td>35</td><td>To be negotiated</td></tr></tbody></table></div>"#;
        assert_eq!(
            AlibabaScraper
                .parse_listing(html, "u")
                .delivery_timeframe
                .as_deref(),
            Some(
                "1 - 2,000 pieces: 32 days; 2,001 - 5,000 pieces: 35 days; > 5,000 pieces: To be negotiated"
            )
        );
        assert_eq!(
            AlibabaScraper
                .parse_listing("<html></html>", "u")
                .delivery_timeframe,
            None
        );
    }

    // Trimmed from a real logistics listing (Oct 2026): no price,
    // minimum-order or lead-time markup - only the page's data script.
    const SHUNQI_DATA: &str = r#"<script>window.detailData = {"globalData":{"product":{"moq":1,"price":{"unit":"kilogram"}},"trade":{"leadTimeInfo":{"ladderPeriodList":[{"maxQuantity":1000,"minQuantity":1,"processPeriod":3}]}}},"nodeMap":{"module_sample_new":{"privateData":{"orderQuantity":{"minOrder":{"minOrderQuantity":1.0,"quantityUnit":"kilogram","formatMinOrderQuantity":"1 kilogram"}},"priceList":[{"minQuantity":1,"maxQuantity":-1,"price":0.02,"formatLadder":"≥1 kilograms","formatPrice":"US$0.02"}]}}}};</script>"#;

    // Trimmed from the real Wenzhou Mike Optical listing (Oct 2026).
    const MIKE_DATA: &str = r#"<script>window.detailData = {"globalData":{"seller":{},"product":{"subject":"MK91283 Anti Blue Light Metal Glasses"}},"nodeMap":{"module_sorted_attribute":{"privateData":{"productSortedProperties":[{"title":"","attributeList":[{"attribute":"Frame Type","value":"Semi-Rimless"},{"attribute":"Model Number","value":"MK91283"}]},{"title":"Packaging and delivery","attributeList":[{"attribute":"Selling Units","value":"Single item"},{"attribute":"Single package size","value":"16X9X8 cm"},{"attribute":"Single gross weight","value":"0.06 kg"}]}]}},"module_description":{"privateData":{"productDescription":{"details":[{"type":"text","text":"Anti Blue Light: Protects eyes."},{"type":"text","text":"Unisex Design."}]}}}}};</script>"#;

    #[test]
    fn packaging_title_and_description_come_from_detail_data() {
        let l = AlibabaScraper.parse_listing(MIKE_DATA, "u");
        assert_eq!(
            l.packaging_details.as_deref(),
            Some(
                "Selling Units: Single item. Single package size: 16X9X8 cm. Single gross weight: 0.06 kg"
            )
        );
        assert_eq!(
            l.title.as_deref(),
            Some("MK91283 Anti Blue Light Metal Glasses")
        );
        assert_eq!(
            l.description.as_deref(),
            Some(
                "Frame Type: Semi-Rimless. Model Number: MK91283\n\nAnti Blue Light: Protects eyes.\nUnisex Design."
            )
        );
    }

    #[test]
    fn visible_title_and_price_win_over_detail_data() {
        let html = product_page(BADGED_CARD, TIERS, MIKE_DATA);
        let l = AlibabaScraper.parse_listing(&html, "u");
        assert_eq!(l.title.as_deref(), Some("Test Product"));
        assert!(l.unit_price.as_deref().unwrap().starts_with("US$250"));
    }

    #[test]
    fn no_packaging_group_means_none() {
        let l = AlibabaScraper.parse_listing(SHUNQI_DATA, "u");
        assert_eq!(l.packaging_details, None);
    }

    #[test]
    fn price_moq_and_lead_time_fall_back_to_detail_data() {
        let l = AlibabaScraper.parse_listing(SHUNQI_DATA, "u");
        assert_eq!(l.unit_price.as_deref(), Some("US$0.02 (≥1 kilograms)"));
        assert_eq!(l.minimum_order_quantity.as_deref(), Some("1 kilogram"));
        assert_eq!(
            l.delivery_timeframe.as_deref(),
            Some("1 - 1000 kilogram: 3 days")
        );
    }
}
