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
//! Checks for every country are files here (sanctions.rs, gleif.rs); checks for
//! one country will go in a folder of that country's name
//! (databases/china/, databases/brazil/...), like sources/b2b/{country}/.
//!
//! Found = a good sign. Not found = not a warning. Only a false claim or
//! a sanctions match is a warning.

pub mod gleif;
pub mod sanctions;

use crate::models::analysis::Signal;

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
}

/// Runs every outside-records check that is built and returns its
/// cards. A check with nothing to go on returns no card.
pub async fn check_outside_records(facts: &SellerFacts) -> Vec<Signal> {
    let mut cards = Vec::new();
    let name = facts.company_name.as_deref();
    let (sanctions, register) = tokio::join!(
        sanctions::sanctions_signal(name),
        gleif::gleif_signal(name, facts.country.as_deref()),
    );
    cards.extend(sanctions);
    cards.extend(register);
    cards
}
