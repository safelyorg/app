//! EU VIES: is an EU VAT number real and active? Works for every EU
//! country. Free, no key, run by the European Commission:
//! https://ec.europa.eu/taxation_customs/vies/rest-api/ms/{country}/vat/{number}
//!
//! Only numbers that start with an EU country code ("FR44732829320",
//! "DE 123456789") are checked, because VIES needs the country. A
//! French number that fails its own check digits is already a
//! "Company number" warning (france.rs), so it is not sent.
//!
//! - Active, and the name VIES gives is the seller's: a good sign.
//! - Active but under another company's name: a warning.
//! - Not active: a warning, but a smaller one - VIES only lists numbers
//!   that may trade with other EU countries, so a small shop that only
//!   sells at home can be real and still "not active".
//! - Some countries (Germany, Spain...) never give the name: then only
//!   "active" is shown.
//! - VIES is often down for one country for a while: then no card.
//!
//! Off unless REGISTERS_CHECK_ENABLED is "true" in .env (the same switch
//! as the Brazil and France registers).

use crate::{
    models::analysis::Signal,
    services::sources::databases::{
        OUTSIDE_RECORDS, france,
        numbers::{names_match, normalize, register_client, registers_enabled},
    },
};
use serde_json::Value;

/// The card's name, as in standards/databases/global.json.
pub const LABEL: &str = "EU VAT number";
pub const ACTIVE: &str = "Active VAT number";
pub const OTHER_NAME: &str = "VAT number of another company";
pub const NOT_ACTIVE: &str = "VAT number not active";

const VIES_URL: &str = "https://ec.europa.eu/taxation_customs/vies/rest-api/ms";

/// The EU's country codes as VAT numbers use them (Greece is "EL";
/// "XI" is Northern Ireland, still in VIES for goods).
const EU_CODES: [&str; 28] = [
    "AT", "BE", "BG", "CY", "CZ", "DE", "DK", "EE", "EL", "ES", "FI", "FR", "HR", "HU", "IE", "IT",
    "LT", "LU", "LV", "MT", "NL", "PL", "PT", "RO", "SE", "SI", "SK", "XI",
];

/// A VAT number split for VIES: "FR44732829320" -> ("FR", "44732829320").
#[derive(Debug, Clone, PartialEq)]
pub struct VatNumber {
    /// As the page writes it.
    pub written: String,
    pub country: &'static str,
    pub number: String,
}

/// The first EU VAT number among the company numbers. Greek numbers
/// written "GR..." are sent as "EL...".
pub fn eu_vat_number(numbers: &[String]) -> Option<VatNumber> {
    numbers.iter().find_map(|written| {
        let n = normalize(written);
        if n.len() < 4 {
            return None;
        }
        let (code, rest) = n.split_at(2);
        let code = if code == "GR" { "EL" } else { code };
        let country = EU_CODES.iter().find(|c| **c == code)?;
        // 2 to 12 letters/digits after the code, at least 2 of them digits.
        if !(2..=12).contains(&rest.len()) || rest.chars().filter(char::is_ascii_digit).count() < 2
        {
            return None;
        }
        if *country == "FR" && !france::vat_is_valid(&n) {
            return None;
        }
        Some(VatNumber {
            written: written.clone(),
            country,
            number: rest.to_string(),
        })
    })
}

/// What VIES said about one number.
#[derive(Debug, Clone, PartialEq)]
pub struct ViesAnswer {
    pub valid: bool,
    /// Empty when the country does not give it (VIES writes "---").
    pub name: String,
    pub address: String,
}

fn text(answer: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|k| answer.get(*k).and_then(Value::as_str))
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty() && t != "---")
        .unwrap_or_default()
}

/// The answer, or None when VIES could not check (the country's own
/// service down, too many requests, an error).
pub fn answer_from(answer: &Value) -> Option<ViesAnswer> {
    // "VALID" / "INVALID" are answers; anything else
    // ("MS_UNAVAILABLE", "TIMEOUT", "MS_MAX_CONCURRENT_REQ"...) is not.
    if let Some(state) = answer.get("userError").and_then(Value::as_str)
        && state != "VALID"
        && state != "INVALID"
    {
        return None;
    }
    let valid = answer
        .get("isValid")
        .or_else(|| answer.get("valid"))
        .and_then(Value::as_bool)?;
    Some(ViesAnswer {
        valid,
        name: text(answer, &["name", "traderName"]),
        address: text(answer, &["address", "traderAddress"]),
    })
}

/// The card from VIES's answer.
pub fn vat_card(seller_name: Option<&str>, vat: &VatNumber, answer: &ViesAnswer) -> Signal {
    let number = &vat.written;
    let under = if answer.name.is_empty() {
        String::new()
    } else if answer.address.is_empty() {
        format!(" under the name \"{}\"", answer.name)
    } else {
        format!(" under the name \"{}\" ({})", answer.name, answer.address)
    };
    let (value, signal_type, sub) = if !answer.valid {
        (
            NOT_ACTIVE,
            "caution",
            format!(
                "The EU VAT number \"{number}\" given by the supplier is not active in the EU's VIES register. It may be made up, closed, or only used inside its own country. Ask the supplier for its VAT certificate before paying."
            ),
        )
    } else if let Some(seller) = seller_name
        .filter(|s| !answer.name.is_empty() && !names_match(s, std::slice::from_ref(&answer.name)))
    {
        (
            OTHER_NAME,
            "caution",
            format!(
                "The EU VAT number \"{number}\" is active in the EU's VIES register{under}, not under \"{seller}\". A supplier using another company's number is a common scam sign - ask why before paying."
            ),
        )
    } else {
        let hidden = if answer.name.is_empty() {
            " This country does not share the company's name, so check that the number is on the supplier's invoice and business papers."
        } else {
            ""
        };
        (
            ACTIVE,
            "good",
            format!(
                "The EU VAT number \"{number}\" given by the supplier is active in the EU's VIES register{under}.{hidden}"
            ),
        )
    };
    Signal {
        label: LABEL.to_string(),
        sub,
        value: value.to_string(),
        signal_type: signal_type.to_string(),
        category: OUTSIDE_RECORDS.to_string(),
        check_type: "existence".to_string(),
    }
}

async fn ask_vies(vat: &VatNumber) -> Option<ViesAnswer> {
    let url = format!(
        "{VIES_URL}/{}/vat/{}",
        vat.country,
        urlencoding::encode(&vat.number)
    );
    match register_client()?.get(url).send().await {
        Ok(r) if r.status().is_success() => match r.json::<Value>().await {
            Ok(answer) => {
                let read = answer_from(&answer);
                if read.is_none() {
                    eprintln!(
                        "Safely: VIES could not check a {} VAT number just now",
                        vat.country
                    );
                }
                read
            }
            Err(e) => {
                eprintln!("Safely: VIES answer could not be read: {}", e);
                None
            }
        },
        Ok(r) => {
            eprintln!("Safely: VIES answered {}", r.status());
            None
        }
        Err(e) => {
            eprintln!("Safely: VIES could not be reached: {}", e);
            None
        }
    }
}

/// The "EU VAT number" card, or None when there is no EU VAT number,
/// the check is off, or VIES did not answer.
pub async fn vat_signal(seller_name: Option<&str>, numbers: &[String]) -> Option<Signal> {
    if !registers_enabled() {
        return None;
    }
    let vat = eu_vat_number(numbers)?;
    let answer = ask_vies(&vat).await?;
    Some(vat_card(seller_name, &vat, &answer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn list(numbers: &[&str]) -> Vec<String> {
        numbers.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn only_numbers_with_an_eu_country_code_are_sent() {
        let vat = eu_vat_number(&list(&["732 829 320", "DE 123 456 789"])).unwrap();
        assert_eq!((vat.country, vat.number.as_str()), ("DE", "123456789"));
        assert_eq!(vat.written, "DE 123 456 789");
        let greek = eu_vat_number(&list(&["GR094014201"])).unwrap();
        assert_eq!(greek.country, "EL");
        assert!(eu_vat_number(&list(&["FR44732829320"])).is_some());
        // A French number with wrong check digits is already a warning.
        assert!(eu_vat_number(&list(&["FR45732829320"])).is_none());
        // CNPJ, China's code, a UK number, too short.
        assert!(
            eu_vat_number(&list(&[
                "11.222.333/0001-81",
                "91440300MA5DC1JX2B",
                "GB123456789",
                "DE1"
            ]))
            .is_none()
        );
    }

    #[test]
    fn vies_answers_are_read_in_both_shapes() {
        let get = json!({ "isValid": true, "userError": "VALID", "name": "ACME OUTILS",
            "address": "1 RUE X\n69002 LYON" });
        assert_eq!(
            answer_from(&get),
            Some(ViesAnswer {
                valid: true,
                name: "ACME OUTILS".into(),
                address: "1 RUE X 69002 LYON".into()
            })
        );
        let post = json!({ "valid": false, "name": "---", "address": "---" });
        let read = answer_from(&post).unwrap();
        assert!(!read.valid && read.name.is_empty() && read.address.is_empty());
        // The country's service is down: no answer, so no card.
        assert!(answer_from(&json!({ "isValid": false, "userError": "MS_UNAVAILABLE" })).is_none());
        assert!(answer_from(&json!({ "actionSucceed": false })).is_none());
    }

    #[test]
    fn vat_cards() {
        let vat = eu_vat_number(&list(&["FR44732829320"])).unwrap();
        let active = ViesAnswer {
            valid: true,
            name: "ACME OUTILS SAS".into(),
            address: "LYON".into(),
        };
        let card = vat_card(Some("Acme Outils"), &vat, &active);
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (ACTIVE, "good")
        );
        assert_eq!(card.category, OUTSIDE_RECORDS);
        assert!(card.sub.contains("\"ACME OUTILS SAS\" (LYON)"));

        let card = vat_card(Some("Golden Steel Mills"), &vat, &active);
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (OTHER_NAME, "caution")
        );

        let hidden = ViesAnswer {
            valid: true,
            name: String::new(),
            address: String::new(),
        };
        let card = vat_card(Some("Golden Steel Mills"), &vat, &hidden);
        assert_eq!(card.value, ACTIVE);
        assert!(card.sub.contains("does not share the company's name"));

        let card = vat_card(
            None,
            &vat,
            &ViesAnswer {
                valid: false,
                ..hidden
            },
        );
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (NOT_ACTIVE, "caution")
        );
    }

    #[tokio::test]
    async fn the_check_is_off_unless_switched_on() {
        if registers_enabled() {
            return;
        }
        assert!(
            vat_signal(Some("Acme"), &list(&["FR44732829320"]))
                .await
                .is_none()
        );
    }
}
