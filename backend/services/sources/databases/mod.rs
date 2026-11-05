//! Outside records about a seller: company registers, chambers of
//! commerce, trade fairs, sanctions lists and website traffic, used to
//! look a seller up outside the page it sells on.
//!
//! One checker for every kind of scan. A marketplace scan, a website
//! check, or any source added later fills in SellerFacts and calls
//! check_outside_records; the checker never reads a marketplace or a
//! website itself. Every card it returns has category "outside_records",
//! so the extension and dashboard can show them in their own section.
//!
//! What is read is listed in backend/standards/databases/: global.json
//! for every seller, and one file per country (china.json, usa.json...)
//! for what only that country has (standards::databases_for_country).
//! Checks for every country are files here (sanctions.rs, gleif.rs,
//! numbers.rs, eu_vat.rs, chambers.rs, trade_shows.rs, website_records.rs). Checks for one country are in a file
//! of that country's name (china.rs, brazil.rs, france.rs, usa.rs; USA's SAM.gov
//! is in sam_gov.rs); when a country gets several
//! sources, its file becomes a folder (databases/china/), like
//! sources/b2b/{country}/.
//!
//! Found = a good sign. Not found = not a warning. Only a false claim or
//! a sanctions match is a warning.
//!
//! A check that ran and found nothing gives a "Not found" card
//! (information only, no points), so the buyer sees what was looked at.
//! A check that could not finish (a site did not answer) gives no card:
//! "not found" must only mean the list really was searched. A check
//! Safely has no list for in the seller's country (chambers, trade
//! fairs) gives a "Not checked" card that says so - never "Not found".

pub mod bbb;
pub mod brazil;
pub mod chambers;
pub mod china;
pub mod eu_vat;
pub mod france;
pub mod gleif;
pub mod numbers;
pub mod reclame_aqui;
pub mod sam_gov;
pub mod sanctions;
pub mod trade_shows;
pub mod usa;
pub mod website_records;

use crate::{models::analysis::Signal, services::shared::standards::databases_for_country};

/// The category of every outside-records card.
pub const OUTSIDE_RECORDS: &str = "outside_records";

/// The value of a check that ran and found nothing.
pub const NOT_FOUND: &str = "Not found";
/// The value of a check Safely has no list for in the seller's country.
pub const NOT_CHECKED: &str = "Not checked";

/// A "Not found" card: information only, never adds to the score
/// (analysis.rs and websites/mod.rs leave outside "info" cards out).
pub fn not_found_card(label: &str, sub: String) -> Signal {
    Signal {
        label: label.to_string(),
        sub,
        value: NOT_FOUND.to_string(),
        signal_type: "info".to_string(),
        category: OUTSIDE_RECORDS.to_string(),
        check_type: "existence".to_string(),
    }
}

/// A "Not checked" card: Safely has nothing to search for this seller's
/// country. Information only, no points.
pub fn not_checked_card(label: &str, sub: String) -> Signal {
    Signal {
        value: NOT_CHECKED.to_string(),
        ..not_found_card(label, sub)
    }
}

/// The seller's country in words for a card: the country file's name
/// ("Brazil"), else the last part of what the page wrote ("Naju-si,
/// South Korea" -> "South Korea"). None when the page gives none.
pub fn country_in_words(country: Option<&str>) -> Option<String> {
    let location = country.map(str::trim).filter(|c| !c.is_empty())?;
    if let Some(file) = databases_for_country(location) {
        return Some(file.name.clone());
    }
    location
        .rsplit([',', '|', '/', ';'])
        .map(str::trim)
        .find(|p| !p.is_empty())
        .map(str::to_string)
}

/// "for South Korea", or "for this seller's country" when the page
/// does not say.
pub fn for_country(country: Option<&str>) -> String {
    match country_in_words(country) {
        Some(c) => format!("for {c}"),
        None => "for this seller's country (the page does not say which)".to_string(),
    }
}

/// The name the lists are searched with. A short code in front of the
/// full name in brackets ("AT(Korea Agro-Fisheries & Food Trade
/// Corporation)", Kompass) or after it ("Korea ... Corporation (aT)")
/// gives the full name; anything else is kept as written ("Huawei
/// Technologies (Shenzhen) Co., Ltd.", "ABC (Hong Kong)").
pub fn name_for_lists(name: &str) -> &str {
    let name = name.trim();
    let Some(open) = name.find('(') else {
        return name;
    };
    let Some(inner) = name.strip_suffix(')').map(|n| &n[open + 1..]) else {
        return name;
    };
    let (outer, inner) = (name[..open].trim(), inner.trim());
    if inner.contains(['(', ')']) {
        return name;
    }
    let is_code = |s: &str| !s.is_empty() && s.chars().count() <= 6 && !s.contains(' ');
    let is_full = |s: &str| s.split_whitespace().count() >= 3;
    if is_code(outer) && is_full(inner) {
        inner
    } else if is_code(inner) && is_full(outer) {
        outer
    } else {
        name
    }
}

/// What a scan knows about the seller. Every field is optional: each
/// check uses what it needs and is skipped without it.
#[derive(Debug, Clone, Default)]
pub struct SellerFacts {
    /// The company's name ("Guangdong Boyue Trading Co., Ltd.").
    pub company_name: Option<String>,
    /// The country or location as the page writes it ("Guangdong, China").
    pub country: Option<String>,
    /// The company's own website.
    pub website: Option<String>,
    /// Company numbers as the page writes them (CNPJ, SIREN, VAT number,
    /// China's 18-character code). Which kind is checked depends on the
    /// country.
    pub company_numbers: Vec<String>,
    /// What the seller writes about itself (a company description, the
    /// text of its own website), to spot claims like "Canton Fair
    /// exhibitor" that a check can confirm or not.
    pub about: Option<String>,
}

/// Runs every outside-records check that is built and returns its
/// cards. A check with nothing to go on returns no card.
pub async fn check_outside_records(facts: &SellerFacts) -> Vec<Signal> {
    let mut cards = Vec::new();
    let name = facts.company_name.as_deref().map(name_for_lists);
    let country = facts.country.as_deref();
    let (
        sanctions,
        register,
        numbers,
        vat,
        stock_market,
        government,
        chamber,
        canton,
        hktdc,
        us_show,
        swapcard,
        algolia,
        complaints,
        bbb_rating,
        website,
    ) = tokio::join!(
        sanctions::sanctions_signal(name),
        gleif::gleif_signal(name, country),
        company_number_cards(facts),
        eu_vat::vat_signal(name, &facts.company_numbers),
        usa::sec_signal(name, country),
        sam_gov::sam_signal(name, country),
        chambers::chamber_signal(name, country, facts.website.as_deref()),
        trade_shows::canton_signal(name, country, facts.about.as_deref()),
        trade_shows::hktdc_signal(name, country),
        trade_shows::mys_signal(name, country),
        trade_shows::swapcard_signal(name),
        trade_shows::algolia_signal(name),
        reclame_aqui::ra_signal(name, country, &facts.company_numbers),
        bbb::bbb_signal(name, country),
        website_records::website_record_cards(facts.website.as_deref()),
    );
    cards.extend(sanctions);
    cards.extend(register);
    cards.extend(numbers);
    cards.extend(vat);
    cards.extend(stock_market);
    cards.extend(government);
    cards.extend(chamber);
    cards.extend(canton);
    cards.extend(trade_shows::fair_cards(
        name,
        country,
        vec![hktdc, us_show, swapcard, algolia],
    ));
    cards.extend(complaints);
    cards.extend(bbb_rating);
    cards.extend(website);
    cards
}

/// The "Company number" card (check digits, inside Safely) and, when
/// REGISTERS_CHECK_ENABLED is on, the "Official register" card (the
/// country's own register). Only for countries with a number check:
/// China, Brazil and France.
async fn company_number_cards(facts: &SellerFacts) -> Vec<Signal> {
    let Some(country) = facts.country.as_deref().and_then(databases_for_country) else {
        return Vec::new();
    };
    let checks = match country.country.as_str() {
        "china" => china::check_numbers(&facts.company_numbers),
        "brazil" => brazil::check_numbers(&facts.company_numbers),
        "france" => france::check_numbers(&facts.company_numbers),
        _ => Vec::new(),
    };
    let mut cards: Vec<Signal> = numbers::number_card(&checks).into_iter().collect();
    if !numbers::registers_enabled() {
        return cards;
    }
    let Some(valid) = checks.iter().find(|c| c.valid) else {
        return cards;
    };
    let name = facts.company_name.as_deref().map(name_for_lists);
    let register = match country.country.as_str() {
        "brazil" => brazil::look_up(&valid.number).await.map(|entry| {
            numbers::register_card(name, &valid.number, brazil::REGISTER, entry.as_ref())
        }),
        "france" => match france::siren_of(&valid.number) {
            Some(siren) => france::look_up(&siren).await.map(|entry| {
                numbers::register_card(name, &valid.number, france::REGISTER, entry.as_ref())
            }),
            None => None,
        },
        _ => None,
    };
    cards.extend(register);
    cards
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_code_with_the_full_name_in_brackets_gives_the_full_name() {
        assert_eq!(
            name_for_lists("AT(Korea Agro-Fisheries & Food Trade Corporation)"),
            "Korea Agro-Fisheries & Food Trade Corporation"
        );
        assert_eq!(
            name_for_lists("Korea Agro-Fisheries & Food Trade Corporation (aT)"),
            "Korea Agro-Fisheries & Food Trade Corporation"
        );
        for kept in [
            "Huawei Technologies (Shenzhen) Co., Ltd.",
            "ABC (Hong Kong)",
            "Baseativa (Brazil)",
            "Guangdong Boyue Trading Co., Ltd.",
            "Baseativa",
        ] {
            assert_eq!(name_for_lists(kept), kept);
        }
    }

    #[test]
    fn the_country_in_words() {
        assert_eq!(
            country_in_words(Some("Caxias do Sul / Rio Grande do Sul | Brazil")).as_deref(),
            Some("Brazil")
        );
        assert_eq!(
            country_in_words(Some("Naju-si, South Korea")).as_deref(),
            Some("South Korea")
        );
        assert_eq!(country_in_words(Some("  ")), None);
        assert_eq!(for_country(Some("Pakistan")), "for Pakistan");
        assert!(for_country(None).contains("the page does not say"));
    }
}
