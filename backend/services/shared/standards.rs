//! Safely's standards: what each kind of source reads and checks, and
//! how much each check adds to the score. One file per kind, in
//! backend/standards/ (b2b.json, websites.json, social.json,
//! databases.json). Checks that two kinds share are written in both
//! files on purpose, so each kind can be changed on its own.
//!
//! The files are built into the program (like config/platform_domains.json),
//! so a change takes effect at the next deploy. A broken file stops the
//! tests (see the tests below), never a running server.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

/// The rule names used in "platform_rules" of b2b.json. Each one is
/// read by the code; the tests check every one is in the file.
pub const NO_FOUNDING_YEAR_FIELD: &str = "no_founding_year_field";
pub const HIDDEN_COMPANY_FIGURES: &str = "hidden_company_figures";
pub const NO_ORDER_DETAILS: &str = "no_order_details";
pub const PRICES_FOR_SIGNED_IN: &str = "prices_for_signed_in";
pub const SUPPLIER_PAGE_SCANS: &str = "supplier_page_scans";

/// Every platform rule the code reads.
pub const PLATFORM_RULES: [&str; 5] = [
    NO_FOUNDING_YEAR_FIELD,
    HIDDEN_COMPANY_FIGURES,
    NO_ORDER_DETAILS,
    PRICES_FOR_SIGNED_IN,
    SUPPLIER_PAGE_SCANS,
];

#[derive(Debug, Deserialize)]
pub struct Standards {
    pub category: String,
    /// "live", "planned" or "not built yet".
    pub status: String,
    pub about: String,
    pub scoring: Scoring,
    #[serde(default)]
    pub platform_rules: HashMap<String, PlatformRule>,
    #[serde(default)]
    pub platforms: Vec<PlatformStandard>,
    #[serde(default)]
    pub rules: Vec<String>,
    pub cards: Vec<CardStandard>,
}

#[derive(Debug, Deserialize)]
pub struct Scoring {
    /// Points for a warning card with no "warning_points" of its own.
    pub default_warning_points: i16,
    /// Points each "info" card adds, and the most all of them can add.
    pub info_points_each: i16,
    pub info_points_max: i16,
    /// The lowest score when Safely could check too little.
    pub not_enough_information_floor: i16,
}

#[derive(Debug, Deserialize)]
pub struct PlatformRule {
    pub about: String,
    pub platforms: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PlatformStandard {
    pub id: String,
    pub name: String,
    pub pages_read: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CardStandard {
    /// The card's name as shown to the user ("Contact info").
    pub card: String,
    /// false: the card is left out of every scan and the score.
    #[serde(default = "yes")]
    pub enabled: bool,
    pub what_it_checks: String,
    #[serde(default)]
    pub reads: Vec<String>,
    /// Points this card adds when it is a warning. None: the default.
    pub warning_points: Option<i16>,
    /// A different number for one exact card value ("Not provided": 10).
    #[serde(default)]
    pub points_when_value: HashMap<String, i16>,
    /// websites.json only: the build step that adds the card.
    pub step: Option<u8>,
}

fn yes() -> bool {
    true
}

impl Standards {
    fn parse(name: &str, raw: &str) -> Standards {
        serde_json::from_str(raw)
            .unwrap_or_else(|e| panic!("backend/standards/{name} is not valid: {e}"))
    }

    pub fn card(&self, label: &str) -> Option<&CardStandard> {
        self.cards.iter().find(|c| c.card == label)
    }

    /// true unless the file switches the card off. A card the file does
    /// not list stays on (the tests make sure every card is listed).
    pub fn card_enabled(&self, label: &str) -> bool {
        self.card(label).is_none_or(|c| c.enabled)
    }

    /// Points a warning card adds to the score.
    pub fn warning_points(&self, label: &str, value: &str) -> i16 {
        let Some(card) = self.card(label) else {
            return self.scoring.default_warning_points;
        };
        card.points_when_value
            .get(value)
            .copied()
            .or(card.warning_points)
            .unwrap_or(self.scoring.default_warning_points)
    }

    /// true when `platform` is in the list of `rule` (see PLATFORM_RULES).
    pub fn platform_has(&self, rule: &str, platform: &str) -> bool {
        self.platform_rules
            .get(rule)
            .is_some_and(|r| r.platforms.iter().any(|p| p == platform))
    }
}

static B2B: LazyLock<Standards> =
    LazyLock::new(|| Standards::parse("b2b.json", include_str!("../../standards/b2b.json")));
static WEBSITES: LazyLock<Standards> = LazyLock::new(|| {
    Standards::parse(
        "websites.json",
        include_str!("../../standards/websites.json"),
    )
});
static SOCIAL: LazyLock<Standards> =
    LazyLock::new(|| Standards::parse("social.json", include_str!("../../standards/social.json")));
static DATABASES: LazyLock<Standards> = LazyLock::new(|| {
    Standards::parse(
        "databases.json",
        include_str!("../../standards/databases.json"),
    )
});

/// B2B marketplaces (backend/standards/b2b.json).
pub fn b2b() -> &'static Standards {
    &B2B
}

/// Sellers' own websites (backend/standards/websites.json).
pub fn websites() -> &'static Standards {
    &WEBSITES
}

/// Social media sellers (backend/standards/social.json).
pub fn social() -> &'static Standards {
    &SOCIAL
}

/// Company databases (backend/standards/databases.json).
pub fn databases() -> &'static Standards {
    &DATABASES
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> [&'static Standards; 4] {
        [b2b(), websites(), social(), databases()]
    }

    #[test]
    fn every_standards_file_reads_and_names_its_category() {
        let names = ["b2b", "websites", "social", "databases"];
        for (s, name) in all().iter().zip(names) {
            assert_eq!(s.category, name);
            assert!(["live", "planned", "not built yet"].contains(&s.status.as_str()));
            assert!(!s.about.trim().is_empty(), "{name}");
            assert!(s.scoring.default_warning_points > 0, "{name}");
            assert!(
                s.scoring.info_points_max >= s.scoring.info_points_each,
                "{name}"
            );
        }
    }

    #[test]
    fn every_card_is_explained_once() {
        for s in all() {
            for (i, c) in s.cards.iter().enumerate() {
                assert!(
                    !c.what_it_checks.trim().is_empty(),
                    "{}: {}",
                    s.category,
                    c.card
                );
                assert!(
                    s.cards[..i].iter().all(|o| o.card != c.card),
                    "{}: {} is listed twice",
                    s.category,
                    c.card
                );
            }
        }
    }

    #[test]
    fn every_platform_rule_the_code_reads_is_in_the_b2b_file() {
        for rule in PLATFORM_RULES {
            let r = b2b()
                .platform_rules
                .get(rule)
                .unwrap_or_else(|| panic!("{rule} is missing"));
            assert!(!r.about.trim().is_empty(), "{rule}");
        }
        for name in b2b().platform_rules.keys() {
            assert!(
                PLATFORM_RULES.contains(&name.as_str()),
                "{name} is not read by the code"
            );
        }
    }

    #[test]
    fn every_b2b_platform_in_the_file_has_a_reader() {
        use crate::services::sources::b2b::get_scraper_for_platform;
        for p in &b2b().platforms {
            assert!(get_scraper_for_platform(&p.id).is_some(), "{}", p.id);
            assert!(!p.pages_read.is_empty(), "{}", p.id);
        }
        for rule in b2b().platform_rules.values() {
            for id in &rule.platforms {
                assert!(
                    b2b().platforms.iter().any(|p| &p.id == id),
                    "{id} is not a platform"
                );
            }
        }
    }

    #[test]
    fn points_come_from_the_file() {
        let s = b2b();
        assert_eq!(s.warning_points("Contact info", "Not confirmed"), 25);
        assert_eq!(s.warning_points("Account age", "Not provided"), 10);
        assert_eq!(s.warning_points("Account age", "Founded this year"), 15);
        assert_eq!(
            s.warning_points("Social presence check", "Scam mentions found"),
            25
        );
        assert_eq!(s.warning_points("A card not in the file", "x"), 15);
        assert!(s.platform_has(PRICES_FOR_SIGNED_IN, "wholesalecentral"));
        assert!(!s.platform_has(PRICES_FOR_SIGNED_IN, "alibaba"));
        assert!(s.card_enabled("Contact info"));
        assert!(s.card_enabled("A card not in the file"));
    }

    /// Every card a B2B scan can show. A new card must be added to
    /// b2b.json too (with its points), or this test fails.
    const B2B_CARDS: [&str; 21] = [
        "Domain check",
        "Safely history",
        "Seller website check",
        "Overall legitimacy check",
        "Registration consistency",
        "Listing detail",
        "Price analysis",
        "Contact info",
        "Urgency language",
        "Advance payment request",
        "Image authenticity",
        "Regulated product",
        "Platform verification",
        "Account age",
        "Seller track record",
        "Product range",
        "Certificates",
        "Company details",
        "Company profile completeness",
        "Listing completeness",
        "Social presence check",
    ];

    #[test]
    fn every_b2b_card_is_in_the_b2b_file_and_nothing_else() {
        for card in B2B_CARDS {
            assert!(
                b2b().card(card).is_some(),
                "{card} is missing from b2b.json"
            );
        }
        for c in &b2b().cards {
            assert!(
                B2B_CARDS.contains(&c.card.as_str()),
                "{} is not a B2B card",
                c.card
            );
        }
    }

    #[test]
    fn website_cards_have_a_build_step() {
        for c in &websites().cards {
            assert!(c.step.is_some_and(|s| (1..=4).contains(&s)), "{}", c.card);
        }
    }
}
