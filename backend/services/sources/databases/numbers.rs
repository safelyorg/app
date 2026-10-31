//! Company numbers (China's 18-character code, Brazil's CNPJ, France's
//! SIREN / SIRET / VAT number): reading them, the "Company number" card
//! (is the number real, or made up?) and the "Official register" card
//! (what the country's own register says about it).
//!
//! The checks of each number are in its country's folder (china/,
//! brazil/, france/); this file holds what they share.

use crate::{
    models::analysis::Signal,
    services::sources::databases::{OUTSIDE_RECORDS, sanctions::compared_name},
};

/// The cards' names, as in standards/databases/global.json.
pub const NUMBER_LABEL: &str = "Company number";
pub const REGISTER_LABEL: &str = "Official register";

/// "Company number" values.
pub const VALID_FORMAT: &str = "Valid format";
pub const INVALID_NUMBER: &str = "Invalid number";

/// "Official register" values.
pub const REGISTER_ACTIVE: &str = "Active";
pub const REGISTER_CLOSED: &str = "Closed";
pub const REGISTER_OTHER_NAME: &str = "Other company name";
pub const REGISTER_NOT_FOUND: &str = "Not found in the register";

/// How a scraper writes a company number into the company description,
/// so the AI sees it too and Safely can read it back
/// (company_numbers_in). One line each: "Registration number: X."
pub const REGISTRATION_PREFIX: &str = "Registration number: ";
pub const VAT_PREFIX: &str = "VAT number: ";

/// Upper-case letters and digits only: "12.345.678/0001-95" ->
/// "12345678000195", "fr 40 303 265 045" -> "FR40303265045".
pub fn normalize(raw: &str) -> String {
    raw.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// The company numbers written in a description (see REGISTRATION_PREFIX).
pub fn company_numbers_in(text: &str) -> Vec<String> {
    let mut numbers: Vec<String> = Vec::new();
    for line in text.lines() {
        for prefix in [REGISTRATION_PREFIX, VAT_PREFIX] {
            if let Some(rest) = line.trim().strip_prefix(prefix) {
                let number = rest.trim().trim_end_matches('.').trim().to_string();
                if !number.is_empty() && !numbers.contains(&number) {
                    numbers.push(number);
                }
            }
        }
    }
    numbers
}

/// One number checked: what kind it is and whether it is real.
#[derive(Debug, Clone, PartialEq)]
pub struct NumberCheck {
    /// As the page writes it.
    pub number: String,
    /// "CNPJ", "SIREN", "SIRET", "EU VAT number", "Unified Social Credit Code".
    pub kind: &'static str,
    pub valid: bool,
}

/// The "Company number" card. A number that fails its check digits is
/// a warning (made up or mistyped); one that passes is a good sign, but
/// only that the number is not made up - not that it belongs to this
/// company (the "Official register" card checks that). None when no
/// number of a known kind was found.
pub fn number_card(checks: &[NumberCheck]) -> Option<Signal> {
    let (value, signal_type, sub) = if let Some(bad) = checks.iter().find(|c| !c.valid) {
        (
            INVALID_NUMBER,
            "caution",
            format!(
                "The {} \"{}\" given by the supplier is not a valid number: its check digits do not add up, so it is made up or mistyped. Ask the supplier for its business licence and check the number yourself.",
                bad.kind, bad.number
            ),
        )
    } else {
        let first = checks.first()?;
        (
            VALID_FORMAT,
            "good",
            format!(
                "The {} \"{}\" given by the supplier is a valid number (its check digits add up). This shows it is not made up - not that it belongs to this company.",
                first.kind, first.number
            ),
        )
    };
    Some(outside_card(NUMBER_LABEL, value, signal_type, sub))
}

/// What a country's register says about one company number.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RegisterEntry {
    /// The register's name ("Receita Federal (Brazil)").
    pub register: &'static str,
    pub number: String,
    /// Every name the register gives (legal name, trade name).
    pub names: Vec<String>,
    pub active: bool,
    /// "2010-05-03" - when the company was opened.
    pub opened: String,
    /// "São Paulo, SP".
    pub place: String,
}

/// true when the seller's name and one of the register's names are the
/// same company name: equal, or one inside the other ("Acme" and "Acme
/// Comercio Ltda"), after legal forms and punctuation are removed.
pub fn names_match(seller_name: &str, register_names: &[String]) -> bool {
    let Some(seller) = compared_name(&without_accents(seller_name)) else {
        return false;
    };
    register_names
        .iter()
        .filter_map(|n| compared_name(&without_accents(n)))
        .any(|r| {
            r == seller
                || format!(" {r} ").contains(&format!(" {seller} "))
                || format!(" {seller} ").contains(&format!(" {r} "))
        })
}

/// "Comércio" -> "Comercio": registers often write names without accents.
fn without_accents(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            'ñ' | 'Ñ' => 'n',
            other => other,
        })
        .collect()
}

/// The "Official register" card from a register's answer. None in
/// `entry` means the register has no company with this number.
pub fn register_card(
    seller_name: Option<&str>,
    number: &str,
    register: &'static str,
    entry: Option<&RegisterEntry>,
) -> Signal {
    let (value, signal_type, sub) = match entry {
        None => (
            REGISTER_NOT_FOUND,
            "info",
            format!(
                "The number \"{number}\" was not found in the {register}. Some registers hide small companies, so ask the supplier for its business licence."
            ),
        ),
        Some(e) => {
            let official = e.names.first().cloned().unwrap_or_default();
            let details = [
                (!e.place.is_empty()).then(|| e.place.clone()),
                (!e.opened.is_empty()).then(|| format!("opened {}", e.opened)),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ");
            let details = if details.is_empty() {
                String::new()
            } else {
                format!(" ({details})")
            };
            let same_name = seller_name.is_none_or(|s| names_match(s, &e.names));
            if !same_name {
                (
                    REGISTER_OTHER_NAME,
                    "caution",
                    format!(
                        "The number \"{number}\" belongs to \"{official}\"{details} in the {register}, not to \"{}\". A supplier using another company's number is a common scam sign - ask why before paying.",
                        seller_name.unwrap_or_default()
                    ),
                )
            } else if !e.active {
                (
                    REGISTER_CLOSED,
                    "caution",
                    format!(
                        "\"{official}\"{details} is listed as CLOSED in the {register}. Ask the supplier why it sells under a closed company before paying."
                    ),
                )
            } else {
                (
                    REGISTER_ACTIVE,
                    "good",
                    format!(
                        "\"{official}\"{details} is an active company in the {register}, under the number \"{number}\" the supplier gave."
                    ),
                )
            }
        }
    };
    outside_card(REGISTER_LABEL, value, signal_type, sub)
}

fn outside_card(label: &str, value: &str, signal_type: &str, sub: String) -> Signal {
    Signal {
        label: label.to_string(),
        sub,
        value: value.to_string(),
        signal_type: signal_type.to_string(),
        category: OUTSIDE_RECORDS.to_string(),
        check_type: "existence".to_string(),
    }
}

/// The official registers (Brazil, France) are asked only when
/// REGISTERS_CHECK_ENABLED is "true" in .env, so tests and local runs
/// never call them. The number check itself needs no switch: it runs
/// inside Safely.
pub fn registers_enabled() -> bool {
    std::env::var("REGISTERS_CHECK_ENABLED")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(false)
}

/// A client for the registers: 10 seconds at most per answer.
pub fn register_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Safely company check (safely.sh)")
        .build()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_read_back_from_the_description() {
        let d = "Address: 1 Rue X, Paris.\nRegistration number: 732 829 320 00074.\nVAT number: FR44732829320.\nWe make tools.";
        assert_eq!(
            company_numbers_in(d),
            vec!["732 829 320 00074", "FR44732829320"]
        );
        assert!(company_numbers_in("We make tools.").is_empty());
        assert_eq!(normalize("12.ABC.345/01DE-35"), "12ABC34501DE35");
    }

    #[test]
    fn a_bad_number_is_a_warning_and_a_good_one_a_good_sign() {
        let good = NumberCheck {
            number: "11.222.333/0001-81".into(),
            kind: "CNPJ",
            valid: true,
        };
        let bad = NumberCheck {
            valid: false,
            number: "11.222.333/0001-82".into(),
            ..good.clone()
        };
        let card = number_card(std::slice::from_ref(&good)).unwrap();
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (VALID_FORMAT, "good")
        );
        assert_eq!(card.category, OUTSIDE_RECORDS);
        let card = number_card(&[good, bad]).unwrap();
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (INVALID_NUMBER, "caution")
        );
        assert!(card.sub.contains("\"11.222.333/0001-82\""));
        assert!(number_card(&[]).is_none());
    }

    #[test]
    fn register_names_are_compared_without_legal_forms() {
        let names = vec![
            "ACME COMERCIO DE FERRAMENTAS LTDA".to_string(),
            "ACME FERRAMENTAS".to_string(),
        ];
        assert!(names_match("Acme Ferramentas", &names));
        assert!(names_match("ACME Comércio de Ferramentas Ltda.", &names));
        assert!(!names_match("Golden Steel Mills", &names));
        // A single word inside another word does not count.
        assert!(!names_match("Acmes", &names));
    }

    #[test]
    fn register_cards() {
        let entry = RegisterEntry {
            register: "Receita Federal (Brazil)",
            number: "11222333000181".into(),
            names: vec!["ACME FERRAMENTAS LTDA".into()],
            active: true,
            opened: "2010-05-03".into(),
            place: "São Paulo, SP".into(),
        };
        let reg = "Receita Federal (Brazil) company register";
        let card = register_card(
            Some("Acme Ferramentas"),
            "11222333000181",
            reg,
            Some(&entry),
        );
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (REGISTER_ACTIVE, "good")
        );
        assert!(card.sub.contains("(São Paulo, SP, opened 2010-05-03)"));

        let card = register_card(
            Some("Golden Steel Mills"),
            "11222333000181",
            reg,
            Some(&entry),
        );
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (REGISTER_OTHER_NAME, "caution")
        );

        let closed = RegisterEntry {
            active: false,
            ..entry
        };
        let card = register_card(
            Some("Acme Ferramentas"),
            "11222333000181",
            reg,
            Some(&closed),
        );
        assert_eq!(card.value, REGISTER_CLOSED);

        let card = register_card(Some("Acme"), "11222333000181", reg, None);
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (REGISTER_NOT_FOUND, "info")
        );
    }
}
