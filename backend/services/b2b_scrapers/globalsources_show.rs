//! Used by globalsources.rs (declared in mod.rs).
//!
//! Global Sources' trade-show site ("Global Sources Hong Kong Shows"):
//! suppliers that rent a booth at its fairs in Hong Kong.
//!
//! - Product page (scanned): www.globalsources.com/products/HK/{name}_{id}/
//! - Exhibitor page (the supplier's page, read with the product page,
//!   never scanned on its own):
//!   www.globalsources.com/exhibitors/HK/{company}_{supplier id}/
//!
//! Everything is in the page data (Nuxt's "__NUXT_DATA__" script), not
//! in the page text:
//! - product page: the product (name, model, photos, text) and a short
//!   supplier record (name, country, business types, trade show booth,
//!   company text);
//! - exhibitor page: the same supplier record, and its product list with
//!   each product's price and minimum order. The product page has no
//!   price, so the price comes from here.
//!
//! "verifiedFlag" on a business type means an outside company
//! (Addcredit) checked it, the same kind of check as "Verified
//! Supplier / Manufacturer" on the main site.
//!
//! These pages show no website, phone, address or founding year field;
//! the founding year is read from the company's own text
//! ("established in 2021").

// This file is loaded by globalsources.rs (#[path = "globalsources_show.rs"]),
// so `super` is globalsources.rs and its helpers can be used directly.
use super::{
    B2bListingProfile, B2bSupplierProfile, OTHER_NAMES_PREFIX, OWN_TEXT_PREFIX, PLATFORM,
    PLATFORM_NAME, PLATFORM_PREFIX, PRODUCT_DETAILS_PREFIX, SupplierRecord, absolute_url, cut,
    other_company_names, selector, squash,
};
use regex::Regex;
use scraper::Html;
use serde_json::{Map, Value};
use std::sync::LazyLock;

const MAX_PROFILE_CHARS: usize = 3000;
const MAX_IMAGES: usize = 6;

static STATED_YEAR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:established|founded|set up|operating since|since)\s+(?:in\s+)?((?:19|20)\d\d)\b",
    )
    .unwrap()
});
static PRODUCT_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"_(\d+)/?(?:[?#].*)?$").unwrap());
static SHOW_PATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)/(?:products|exhibitors)/([a-z]{2})/").unwrap());

/// One value from Nuxt's page data. The data is a flat list where every
/// object field and list item is the number of another entry; special
/// values are ["Reactive", n], ["ShallowReactive", n], ["Ref", n]...
fn resolve(items: &[Value], index: &Value, depth: usize) -> Value {
    if depth > 60 {
        return Value::Null;
    }
    let Some(i) = index.as_i64() else {
        return index.clone();
    };
    if i < 0 {
        return Value::Null;
    }
    let Some(item) = items.get(i as usize) else {
        return Value::Null;
    };
    match item {
        Value::Object(fields) => {
            let mut out = Map::new();
            for (key, value) in fields {
                out.insert(key.clone(), resolve(items, value, depth + 1));
            }
            Value::Object(out)
        }
        Value::Array(list) => match list.first() {
            Some(Value::String(_)) => match list.get(1) {
                Some(inner) => resolve(items, inner, depth + 1),
                None => Value::Null,
            },
            _ => Value::Array(list.iter().map(|v| resolve(items, v, depth + 1)).collect()),
        },
        other => other.clone(),
    }
}

/// The page's own data ("data" of __NUXT_DATA__), or None.
fn nuxt_data(html: &str) -> Option<Value> {
    let document = Html::parse_document(html);
    let script = document.select(&selector("script#__NUXT_DATA__")).next()?;
    let raw = script.text().collect::<String>();
    let items: Vec<Value> = serde_json::from_str(raw.trim()).ok()?;
    let root = resolve(&items, &Value::from(0), 0);
    root.get("data").cloned()
}

/// What one trade-show page gives.
#[derive(Debug, Default)]
pub(super) struct ShowPage {
    /// The product (product page only).
    product: Option<Value>,
    supplier: Option<Value>,
    /// The supplier's products with prices (exhibitor page only).
    products: Vec<Value>,
    /// "HK" from the page address.
    show: String,
}

/// The trade-show page in `html`, or None for any other page.
pub(super) fn read(html: &str) -> Option<ShowPage> {
    let data = nuxt_data(html)?;
    let show = SHOW_PATH
        .captures(html)
        .map(|c| c[1].to_uppercase())
        .unwrap_or_else(|| "HK".to_string());
    if let Some(detail) = data
        .get("exhibitorProductDetail")
        .and_then(|d| d.get("data"))
    {
        return Some(ShowPage {
            product: detail.get("productInfo").cloned(),
            supplier: detail.get("supplierInfo").cloned(),
            products: Vec::new(),
            show,
        });
    }
    let parts = data.get("ExhibitorDetail")?.as_array()?;
    let part = |i: usize| {
        parts
            .get(i)
            .and_then(|p| p.get("value"))
            .and_then(|v| v.get("data"))
    };
    let supplier = part(0).cloned();
    supplier.as_ref()?;
    Some(ShowPage {
        product: None,
        supplier,
        products: part(1)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        show,
    })
}

fn text(value: Option<&Value>, key: &str) -> Option<String> {
    match value?.get(key)? {
        Value::String(s) => {
            let s = squash(s);
            (!s.is_empty()).then_some(s)
        }
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn flag(value: Option<&Value>, key: &str) -> bool {
    value
        .and_then(|v| v.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn strings(value: Option<&Value>, key: &str) -> Vec<String> {
    value
        .and_then(|v| v.get(key))
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(squash)
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// The text a piece of HTML shows ("<p>Angle rock</p><p>...</p>").
fn html_text(html: &str) -> String {
    let fragment = Html::parse_fragment(html);
    squash(&fragment.root_element().text().collect::<Vec<_>>().join(" "))
}

/// "Anhui Dujia Office Supplies Manufacturing Co.,Ltd" ->
/// "anhui-dujia-office-supplies-manufacturing-co-ltd", as in the
/// exhibitor page's address.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

impl ShowPage {
    pub(super) fn is_product_page(&self) -> bool {
        self.product.is_some()
    }

    fn supplier_id(&self) -> Option<String> {
        text(self.supplier.as_ref(), "supplierId")
            .or_else(|| text(self.product.as_ref(), "supplierId"))
    }

    fn company_name(&self) -> Option<String> {
        text(self.supplier.as_ref(), "companyName")
    }

    /// Business types an outside company checked, e.g. ["Manufacturer"],
    /// and who checked them ("Addcredit").
    fn checked_business_types(&self) -> (Vec<String>, Option<String>) {
        let mut types = Vec::new();
        let mut vendor = None;
        let list = self
            .supplier
            .as_ref()
            .and_then(|s| s.get("businessTypeList"))
            .and_then(Value::as_array);
        for entry in list.into_iter().flatten() {
            if entry.get("verifiedFlag").and_then(Value::as_bool) == Some(true)
                && let Some(t) = text(Some(entry), "businessType")
            {
                types.push(t);
                if vendor.is_none() {
                    vendor = text(Some(entry), "vendorName");
                }
            }
        }
        (types, vendor)
    }

    /// "Verified Manufacturer" / "Verified Supplier" when an outside
    /// company checked the business type (see signals.rs).
    pub(super) fn badge(&self) -> Option<String> {
        let (types, _) = self.checked_business_types();
        if types.is_empty() {
            None
        } else if types.iter().any(|t| t.eq_ignore_ascii_case("manufacturer")) {
            Some("Verified Manufacturer".to_string())
        } else {
            Some("Verified Supplier".to_string())
        }
    }

    fn profile_text(&self) -> Option<String> {
        text(self.supplier.as_ref(), "orgDesc")
            .map(|d| html_text(&d))
            .filter(|t| !t.is_empty())
            .map(|t| cut(&t, MAX_PROFILE_CHARS))
    }

    /// The year the company says it was founded, from its own text.
    fn stated_year(&self) -> Option<String> {
        STATED_YEAR
            .captures(&self.profile_text()?)
            .map(|c| c[1].to_string())
    }

    /// "https://www.globalsources.com/exhibitors/HK/anhui-dujia-..._2008801135123/".
    pub(super) fn exhibitor_url(&self) -> Option<String> {
        let id = self.supplier_id()?;
        let name = slug(&self.company_name()?);
        Some(format!(
            "https://www.globalsources.com/exhibitors/{}/{}_{}/",
            self.show, name, id
        ))
    }

    /// The same for the supplier's product page and its exhibitor page.
    pub(super) fn company_key(&self) -> Option<String> {
        self.supplier_id()
            .map(|id| format!("globalsources-exhibitor-{id}"))
    }

    fn lines(&self) -> Vec<String> {
        let s = self.supplier.as_ref();
        let mut lines = Vec::new();
        if let Some(c) = text(s, "country") {
            lines.push(format!("Country: {c}."));
        }
        let types = strings(s, "businessTypes");
        let (checked, vendor) = self.checked_business_types();
        if !types.is_empty() {
            let note = match (&vendor, checked.len()) {
                (_, 0) => String::new(),
                (Some(v), n) if n == types.len() => {
                    format!(" (each checked by {v}, an outside company used by Global Sources)")
                }
                (Some(v), _) => format!(
                    " ({} checked by {v}, an outside company used by Global Sources)",
                    checked.join(", ")
                ),
                (None, _) => " (checked by an outside company)".to_string(),
            };
            lines.push(format!("Business type: {}{note}.", types.join(", ")));
        }
        let services: Vec<&str> = [("oemFlag", "OEM"), ("odmFlag", "ODM"), ("obmFlag", "OBM")]
            .iter()
            .filter(|(key, _)| flag(s, key))
            .map(|(_, name)| *name)
            .collect();
        if !services.is_empty() {
            lines.push(format!("Services: {}.", services.join(", ")));
        }
        let shows: Vec<String> = s
            .and_then(|v| v.get("tradeShowList"))
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .map(|t| {
                        let part = |k: &str| text(Some(t), k);
                        let mut place = Vec::new();
                        place.extend(part("tsName"));
                        place.extend(part("showVenue"));
                        place.extend(part("hall").map(|h| format!("Hall {h}")));
                        place.extend(part("boothNum").map(|b| format!("Booth {b}")));
                        place.extend(part("showDate"));
                        place.join(", ")
                    })
                    .filter(|p| !p.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        if shows.is_empty() {
            lines.push(format!(
                "{PLATFORM_PREFIX}listed as an exhibitor at its Hong Kong trade shows."
            ));
        } else {
            lines.push(format!(
                "{PLATFORM_PREFIX}exhibitor at its Hong Kong trade shows ({}).",
                shows.join("; ")
            ));
        }
        let products = strings(s, "categoryNames");
        if !products.is_empty() {
            lines.push(format!("Main products: {}.", products.join(", ")));
        }
        if let Some(year) = self.stated_year() {
            lines.push(format!("Founded (in the company's own text): {year}."));
        }
        if let Some(profile) = self.profile_text() {
            let others = other_company_names(&profile, self.company_name().as_deref());
            if !others.is_empty() {
                lines.push(format!(
                    "{OTHER_NAMES_PREFIX}{}. Ask how they are related.",
                    others.join("; ").trim_end_matches('.')
                ));
            }
            lines.push(profile);
        }
        lines
    }

    pub(super) fn supplier_profile(&self, profile_url: &str) -> B2bSupplierProfile {
        let lines = self.lines();
        B2bSupplierProfile {
            company_name: self.company_name(),
            logo_url: text(self.supplier.as_ref(), "logoImageUrl").map(|u| absolute_url(&u)),
            year_established: self.stated_year(),
            country: text(self.supplier.as_ref(), "country"),
            platform_verified_badge: self.badge().is_some(),
            profile_url: profile_url.to_string(),
            source_platform: PLATFORM.to_string(),
            badge_honorific: self.badge(),
            company_description: (!lines.is_empty()).then(|| lines.join("\n")),
            ..Default::default()
        }
    }

    pub(super) fn record(&self) -> Option<SupplierRecord> {
        let checked = self.badge().is_some();
        let types = strings(self.supplier.as_ref(), "businessTypes");
        (checked || !types.is_empty()).then(|| SupplierRecord {
            platform: PLATFORM_NAME.to_string(),
            checked_by_platform: checked,
            business_types: types,
            ..Default::default()
        })
    }

    pub(super) fn listing(&self, listing_url: &str) -> B2bListingProfile {
        let mut listing = B2bListingProfile {
            listing_url: listing_url.to_string(),
            source_platform: PLATFORM.to_string(),
            ..Default::default()
        };
        let Some(p) = self.product.as_ref() else {
            return listing;
        };
        listing.title = text(Some(p), "productName");
        listing.reference = text(Some(p), "modelNumber");

        let mut images: Vec<String> = Vec::new();
        let primary = p.get("productPrimaryImage");
        let first = text(primary, "largeImageUrl")
            .or_else(|| text(primary, "bigImageUrl"))
            .or_else(|| text(Some(p), "productPrimaryImageUrl"));
        images.extend(first);
        for extra in p
            .get("productAddlImageList")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let url = text(Some(extra), "largeImageUrl")
                .or_else(|| text(Some(extra), "bigImageUrl"))
                .or_else(|| extra.as_str().map(str::to_string));
            if let Some(u) = url
                && !images.contains(&u)
            {
                images.push(u);
            }
        }
        images.truncate(MAX_IMAGES);
        listing.image_urls = images.iter().map(|u| absolute_url(u)).collect();

        let mut details: Vec<String> = Vec::new();
        details.extend(
            listing
                .reference
                .as_ref()
                .map(|m| format!("Model Number: {m}")),
        );
        details.extend(text(Some(p), "brandName").map(|b| format!("Brand Name: {b}")));
        details.extend(text(Some(p), "countryOrigin").map(|c| format!("Origin: {c}")));
        if flag(Some(p), "smallOrderAcceptFlag") {
            details.push("Small Orders: Accepted".to_string());
        }
        let certificates = strings(Some(p), "productCertStandardList");
        if !certificates.is_empty() {
            details.push(format!("Certificates: {}", certificates.join(", ")));
        }
        let mut parts: Vec<String> = Vec::new();
        if !details.is_empty() {
            parts.push(format!("{PRODUCT_DETAILS_PREFIX}{}.", details.join("; ")));
        }
        let own: Vec<String> = ["productOutline", "productDescription"]
            .iter()
            .filter_map(|k| text(Some(p), k))
            .map(|h| html_text(&h))
            .filter(|t| !t.is_empty())
            .collect();
        if !own.is_empty() {
            parts.push(format!("{OWN_TEXT_PREFIX}{}", cut(&own.join(" "), 2500)));
        }
        listing.description = (!parts.is_empty()).then(|| parts.join("\n"));
        listing
    }

    /// The price and minimum order of the product at `listing_url`, from
    /// the exhibitor page's product list.
    pub(super) fn add_price(&self, listing: &mut B2bListingProfile) {
        let Some(id) = PRODUCT_ID
            .captures(&listing.listing_url)
            .map(|c| c[1].to_string())
        else {
            return;
        };
        let Some(p) = self
            .products
            .iter()
            .find(|p| text(Some(p), "productId").as_deref() == Some(id.as_str()))
        else {
            return;
        };
        if listing.unit_price.is_none() {
            let low = text(Some(p), "productFOBMin");
            let high = text(Some(p), "productFOBMax");
            listing.unit_price = match (low, high) {
                (Some(l), Some(h)) if l != h => Some(format!("US$ {l}-{h}")),
                (Some(l), _) => Some(format!("US$ {l}")),
                (None, Some(h)) => Some(format!("US$ {h}")),
                (None, None) => None,
            };
            if listing.unit_price.is_some() && listing.incoterms.is_none() {
                listing.incoterms = Some("FOB".to_string());
            }
        }
        if listing.minimum_order_quantity.is_none() {
            listing.minimum_order_quantity = text(Some(p), "minOrderQuantity").map(|q| {
                // "1 Piece", "1000 Pieces".
                let unit = if q == "1" {
                    text(Some(p), "minOrderSingleUnit").or_else(|| text(Some(p), "minOrderUnit"))
                } else {
                    text(Some(p), "minOrderUnit")
                };
                match unit {
                    Some(unit) => format!("{q} {unit}"),
                    None => q,
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exhibitor_address_is_built_from_the_company_name() {
        assert_eq!(
            slug("Anhui Dujia Office Supplies Manufacturing Co.,Ltd"),
            "anhui-dujia-office-supplies-manufacturing-co-ltd"
        );
        assert_eq!(slug("A & B Co., Ltd."), "a-b-co-ltd");
    }

    #[test]
    fn nuxt_data_is_resolved() {
        let items: Vec<Value> = serde_json::from_str(
            r#"[["ShallowReactive",1],{"data":2},["ShallowReactive",3],{"x":4,"list":5},"hello",[4,6],true]"#,
        )
        .unwrap();
        let root = resolve(&items, &Value::from(0), 0);
        assert_eq!(root["data"]["x"], "hello");
        assert_eq!(root["data"]["list"], serde_json::json!(["hello", true]));
    }

    #[test]
    fn the_founding_year_comes_from_the_company_text() {
        assert_eq!(
            STATED_YEAR
                .captures("Anhui Dujia Co., Ltd., established in 2021, is the new base")
                .map(|c| c[1].to_string())
                .as_deref(),
            Some("2021")
        );
        assert!(STATED_YEAR.captures("We have 2021 workers").is_none());
    }
}
