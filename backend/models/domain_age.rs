use serde::{Deserialize, Serialize};

/// GET /api/v1/tools/domain-age?domain=supplier.com
#[derive(Debug, Deserialize)]
pub struct DomainAgeQuery {
    /// What the visitor typed: "supplier.com", "https://www.supplier.com/about", ...
    #[serde(default)]
    pub domain: String,
}

/// How old a domain is, in a few words. The page shows a matching
/// title, text and color for each one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Registered less than 6 months ago.
    New,
    /// Registered 6 months to 2 years ago.
    Young,
    /// Registered more than 2 years ago.
    Established,
    /// The registry does not share the date.
    Unknown,
}

/// The answer the Domain Age Checker shows. Dates are "2026-10-18".
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DomainAge {
    /// The domain that was really looked up, for example "supplier.com"
    /// when the visitor typed "https://shop.supplier.com/about".
    pub domain: String,
    pub registered_on: Option<String>,
    pub expires_on: Option<String>,
    pub updated_on: Option<String>,
    pub registrar: Option<String>,
    /// Days between the registration date and today.
    pub age_days: Option<i64>,
    pub verdict: Verdict,
    /// True when the domain expires in the next 3 months. Scam sites
    /// are often registered for one year only.
    pub expires_soon: bool,
}
