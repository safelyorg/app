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
//! numbers.rs, website_records.rs). Checks for one country are in a file of that country's
//! name (china.rs, brazil.rs, france.rs); when a country gets several
//! sources, its file becomes a folder (databases/china/), like
//! sources/b2b/{country}/.
//!
//! Found = a good sign. Not found = not a warning. Only a false claim or
//! a sanctions match is a warning.

pub mod brazil;
pub mod china;
pub mod france;
pub mod gleif;
pub mod numbers;
pub mod sanctions;
pub mod website_records;

use crate::{models::analysis::Signal, services::shared::standards::databases_for_country};

/// The category of every outside-records card.
pub const OUTSIDE_RECORDS: &str = "outside_records";

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
}

/// Runs every outside-records check that is built and returns its
/// cards. A check with nothing to go on returns no card.
pub async fn check_outside_records(facts: &SellerFacts) -> Vec<Signal> {
    let mut cards = Vec::new();
    let name = facts.company_name.as_deref();
    let (sanctions, register, numbers, website) = tokio::join!(
        sanctions::sanctions_signal(name),
        gleif::gleif_signal(name, facts.country.as_deref()),
        company_number_cards(facts),
        website_records::website_record_cards(facts.website.as_deref()),
    );
    cards.extend(sanctions);
    cards.extend(register);
    cards.extend(numbers);
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
    let name = facts.company_name.as_deref();
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
