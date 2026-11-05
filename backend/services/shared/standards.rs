//! Safely's standards: what each kind of source reads and checks, and
//! how much each check adds to the score. One file per kind, in
//! backend/standards/ (b2b.json, websites.json, social.json), plus the
//! databases/ folder: global.json for every seller and one file per
//! country (china.json, usa.json...) for the outside records only that
//! country has. Checks that two kinds share are written in both files on
//! purpose, so each kind can be changed on its own.
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
    /// databases/global.json only: the outside records Safely reads.
    #[serde(default)]
    pub sources: Vec<SourceStandard>,
    pub cards: Vec<CardStandard>,
}

/// One country's outside records (backend/standards/databases/{country}.json).
/// Scoring comes from databases/global.json.
#[derive(Debug, Deserialize)]
pub struct CountryStandards {
    /// The file name without .json, e.g. "hong_kong".
    pub country: String,
    pub name: String,
    /// How the country is written on seller pages ("China", "CN").
    pub names: Vec<String>,
    #[serde(default)]
    pub company_ids: Vec<CompanyIdFormat>,
    pub sources: Vec<SourceStandard>,
    pub cards: Vec<CardStandard>,
}

/// A company number used in one country (China's 18-character code,
/// Brazil's CNPJ, France's SIREN).
#[derive(Debug, Deserialize)]
pub struct CompanyIdFormat {
    pub name: String,
    /// Number of letters/digits, without spaces, dots or dashes.
    pub length: u8,
    /// true: the last character(s) can be checked inside Safely.
    pub has_check_digit: bool,
    pub about: String,
}

/// One outside record: a register, a chamber, a fair list, a ranking.
#[derive(Debug, Deserialize)]
pub struct SourceStandard {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub url: String,
    pub what_it_does: String,
    /// "offline", "api", "download", "scrape" or "paid_api".
    pub access: String,
    /// "free", "small_fee" or "paid".
    pub cost: String,
    /// The build step that adds it (1 is built first).
    pub step: u8,
    /// "planned" (not built yet) or "live".
    pub status: String,
    /// The name of its key in .env. The key itself is never in the file.
    pub key_env: Option<String>,
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
    /// The country folder its reader is in: services/sources/b2b/{country}/.
    pub country: String,
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
    /// The section the card is shown in. Database cards use
    /// "outside_records"; None: the normal list.
    pub group: Option<String>,
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
        "databases/global.json",
        include_str!("../../standards/databases/global.json"),
    )
});

/// One file per country, in backend/standards/databases/. A new country:
/// add its file there and one line here.
const COUNTRY_FILES: [(&str, &str); 5] = [
    // Hong Kong comes before China, so "Hong Kong, China" is Hong Kong.
    (
        "hong_kong",
        include_str!("../../standards/databases/hong_kong.json"),
    ),
    (
        "china",
        include_str!("../../standards/databases/china.json"),
    ),
    ("usa", include_str!("../../standards/databases/usa.json")),
    (
        "brazil",
        include_str!("../../standards/databases/brazil.json"),
    ),
    (
        "france",
        include_str!("../../standards/databases/france.json"),
    ),
];

static COUNTRIES: LazyLock<Vec<CountryStandards>> = LazyLock::new(|| {
    COUNTRY_FILES
        .iter()
        .map(|(name, raw)| {
            serde_json::from_str(raw).unwrap_or_else(|e| {
                panic!("backend/standards/databases/{name}.json is not valid: {e}")
            })
        })
        .collect()
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

/// Outside records for every seller (backend/standards/databases/global.json).
/// Also holds the scoring for the country files.
pub fn databases() -> &'static Standards {
    &DATABASES
}

/// An outside-records card from databases/global.json or a country file.
pub fn database_card(label: &str) -> Option<&'static CardStandard> {
    databases().card(label).or_else(|| {
        database_countries()
            .iter()
            .find_map(|c| c.cards.iter().find(|x| x.card == label))
    })
}

/// Points an outside-records warning card adds to the score
/// (databases/global.json scoring when the card has none).
pub fn database_warning_points(label: &str, value: &str) -> i16 {
    let default = databases().scoring.default_warning_points;
    database_card(label).map_or(default, |card| {
        card.points_when_value
            .get(value)
            .copied()
            .or(card.warning_points)
            .unwrap_or(default)
    })
}

/// true unless a databases file switches the card off.
pub fn database_card_enabled(label: &str) -> bool {
    database_card(label).is_none_or(|c| c.enabled)
}

/// Every country file in backend/standards/databases/.
pub fn database_countries() -> &'static [CountryStandards] {
    &COUNTRIES
}

/// The country file for a seller's country as the page writes it
/// ("Guangdong, China", "Shenzhen, CN", "Brasil"). Each part between
/// commas is compared with the file's "names", ignoring case. None: no
/// file for that country (only global.json is used).
pub fn databases_for_country(location: &str) -> Option<&'static CountryStandards> {
    let parts: Vec<String> = std::iter::once(location)
        .chain(location.split(','))
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    database_countries().iter().find(|c| {
        c.names
            .iter()
            .any(|n| parts.contains(&n.trim().to_lowercase()))
    })
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
            // Its reader is in the country folder the file names.
            let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("services/sources/b2b")
                .join(&p.country)
                .join(format!("{}.rs", p.id));
            assert!(file.exists(), "{} is not at {}", p.id, file.display());
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
    fn every_country_file_reads_and_matches_its_name() {
        let ids: Vec<&str> = database_countries()
            .iter()
            .map(|c| c.country.as_str())
            .collect();
        assert_eq!(ids, ["hong_kong", "china", "usa", "brazil", "france"]);
        for (c, (file, _)) in database_countries().iter().zip(COUNTRY_FILES) {
            assert_eq!(c.country, file);
            assert!(!c.names.is_empty(), "{file}");
            for id in &c.company_ids {
                assert!(id.length > 0 && !id.about.trim().is_empty(), "{file}");
            }
            for card in &c.cards {
                assert!(!card.what_it_checks.trim().is_empty(), "{file}");
            }
        }
    }

    /// Every country Safely reads B2B platforms from has a databases file.
    #[test]
    fn every_b2b_country_has_a_databases_file() {
        for p in &b2b().platforms {
            assert!(
                database_countries().iter().any(|c| c.country == p.country),
                "standards/databases/{}.json is missing",
                p.country
            );
        }
    }

    #[test]
    fn every_source_is_described_the_same_way() {
        let all_sources = databases()
            .sources
            .iter()
            .chain(database_countries().iter().flat_map(|c| c.sources.iter()));
        let mut seen: Vec<&str> = Vec::new();
        for s in all_sources {
            assert!(!seen.contains(&s.id.as_str()), "{} is listed twice", s.id);
            seen.push(&s.id);
            assert!(
                !s.name.trim().is_empty() && !s.what_it_does.trim().is_empty(),
                "{}",
                s.id
            );
            assert!(
                ["offline", "api", "download", "scrape", "paid_api"].contains(&s.access.as_str()),
                "{}: access {}",
                s.id,
                s.access
            );
            assert!(
                ["free", "small_fee", "paid"].contains(&s.cost.as_str()),
                "{}",
                s.id
            );
            assert!(["planned", "live"].contains(&s.status.as_str()), "{}", s.id);
            assert!((1..=5).contains(&s.step), "{}", s.id);
            assert!(s.url.is_empty() || s.url.starts_with("http"), "{}", s.id);
            // Only the NAME of a key, never a key.
            if let Some(k) = &s.key_env {
                assert!(
                    k.chars().all(|ch| ch.is_ascii_uppercase() || ch == '_'),
                    "{}: key_env must be a name like SAM_GOV_API_KEY",
                    s.id
                );
            }
        }
        assert!(!databases().sources.is_empty());
    }

    #[test]
    fn database_cards_are_in_the_outside_records_group() {
        let cards = databases()
            .cards
            .iter()
            .chain(database_countries().iter().flat_map(|c| c.cards.iter()));
        for c in cards {
            assert_eq!(c.group.as_deref(), Some("outside_records"), "{}", c.card);
        }
    }

    /// Every outside-records card the code can make. A new card must be
    /// added to a databases file too (with its points), or this fails.
    const DATABASE_CARDS: [&str; 19] = [
        "Sanctions lists",
        "Company register (GLEIF)",
        "Company number",
        "Official register",
        "EU VAT number",
        "US stock market (SEC)",
        "US government register (SAM.gov)",
        "Chamber of commerce",
        "Canton Fair",
        "China Customs AEO",
        "HKTDC fair",
        "US trade show",
        "Brazil trade show",
        "France trade show",
        "Reclame Aqui",
        "BBB rating",
        "Website history",
        "Website traffic",
        "Security certificate",
    ];

    #[test]
    fn every_database_card_is_in_a_databases_file() {
        for card in DATABASE_CARDS {
            assert!(database_card(card).is_some(), "{card} is missing");
        }
        assert_eq!(
            database_warning_points("Sanctions lists", "Possible match"),
            70
        );
        assert_eq!(
            database_warning_points("Company register (GLEIF)", "Closed in the register"),
            25
        );
        assert_eq!(
            database_warning_points("Company number", "Invalid number"),
            40
        );
        assert_eq!(
            database_warning_points("Official register", "Other company name"),
            40
        );
        assert_eq!(database_warning_points("Official register", "Closed"), 25);
        assert_eq!(
            database_warning_points("EU VAT number", "VAT number of another company"),
            40
        );
        assert_eq!(
            database_warning_points("EU VAT number", "VAT number not active"),
            20
        );
        assert_eq!(
            database_warning_points("Canton Fair", "Claim not confirmed"),
            20
        );
        assert_eq!(
            database_warning_points("Reclame Aqui", "Not recommended"),
            40
        );
        assert_eq!(
            database_warning_points("Reclame Aqui", "Bad reputation"),
            20
        );
        assert_eq!(database_warning_points("BBB rating", "F rating"), 40);
        assert_eq!(database_warning_points("BBB rating", "D rating"), 20);
        assert_eq!(
            database_warning_points(
                "US government register (SAM.gov)",
                "Excluded by the US government"
            ),
            40
        );
        assert_eq!(database_warning_points("Not a card", "x"), 15);
        assert!(database_card_enabled("Sanctions lists"));
        // An outside-records card never shares a name with a B2B card.
        for card in DATABASE_CARDS {
            assert!(b2b().card(card).is_none(), "{card}");
        }
    }

    #[test]
    fn seller_country_picks_the_right_file() {
        let country = |l: &str| databases_for_country(l).map(|c| c.country.as_str());
        assert_eq!(country("Guangdong, China"), Some("china"));
        assert_eq!(country("Shenzhen, CN"), Some("china"));
        assert_eq!(country("CN"), Some("china"));
        assert_eq!(country("Hong Kong"), Some("hong_kong"));
        assert_eq!(country("Hong Kong, China"), Some("hong_kong"));
        assert_eq!(country("Brasil"), Some("brazil"));
        assert_eq!(country("São Paulo, Brazil"), Some("brazil"));
        assert_eq!(country("Paris, France"), Some("france"));
        assert_eq!(country("New York, USA"), Some("usa"));
        assert_eq!(country("united states"), Some("usa"));
        // Only whole parts count: "Chinatown" is not China.
        assert_eq!(country("Chinatown, Singapore"), None);
        assert_eq!(country("Pakistan"), None);
        assert_eq!(country(""), None);
    }

    #[test]
    fn website_cards_have_a_build_step() {
        for c in &websites().cards {
            assert!(c.step.is_some_and(|s| (1..=4).contains(&s)), "{}", c.card);
        }
    }
}
