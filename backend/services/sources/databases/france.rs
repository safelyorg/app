//! France: the SIREN (9 digits), the SIRET (14 digits: SIREN + 5 for the
//! site) and the EU VAT number (FR + 2 check characters + SIREN). SIREN
//! and SIRET end with a Luhn check digit; the VAT number's 2 characters
//! are worked out from the SIREN. So a made-up number is spotted inside
//! Safely.
//!
//! The official register is read through the government's free search
//! (no key): https://recherche-entreprises.api.gouv.fr/search?q={siren}

use crate::services::sources::databases::numbers::{
    NumberCheck, RegisterEntry, normalize, register_client,
};
use serde_json::Value;

pub const REGISTER: &str = "French company register (INSEE / data.gouv.fr)";
const SEARCH_URL: &str = "https://recherche-entreprises.api.gouv.fr/search";

/// Luhn check, used by SIREN and SIRET.
fn luhn_ok(digits: &str) -> bool {
    let mut sum = 0;
    for (i, c) in digits.chars().rev().enumerate() {
        let Some(mut d) = c.to_digit(10) else {
            return false;
        };
        if i % 2 == 1 {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        sum += d;
    }
    sum % 10 == 0
}

pub fn siren_is_valid(raw: &str) -> bool {
    let n = normalize(raw);
    n.len() == 9 && n.chars().all(|c| c.is_ascii_digit()) && luhn_ok(&n)
}

pub fn siret_is_valid(raw: &str) -> bool {
    let n = normalize(raw);
    // La Poste's sites (SIREN 356000000) use another rule.
    n.len() == 14
        && n.chars().all(|c| c.is_ascii_digit())
        && (luhn_ok(&n) || n.starts_with("356000000"))
        && siren_is_valid(&n[..9])
}

/// FR + 2 digits + SIREN, the 2 digits being (12 + 3 x (SIREN mod 97)) mod 97.
pub fn vat_is_valid(raw: &str) -> bool {
    let n = normalize(raw);
    let Some(rest) = n.strip_prefix("FR") else {
        return false;
    };
    if rest.len() != 11 || !rest.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let (key, siren) = rest.split_at(2);
    let Ok(siren_number) = siren.parse::<u64>() else {
        return false;
    };
    let expected = (12 + 3 * (siren_number % 97)) % 97;
    key.parse::<u64>().ok() == Some(expected) && siren_is_valid(siren)
}

/// The SIREN inside a SIREN, SIRET or VAT number.
pub fn siren_of(raw: &str) -> Option<String> {
    let n = normalize(raw);
    match n.len() {
        9 => Some(n),
        14 if !n.starts_with("FR") => Some(n[..9].to_string()),
        13 if n.starts_with("FR") => Some(n[4..].to_string()),
        _ => None,
    }
}

/// The French numbers among `numbers`.
pub fn check_numbers(numbers: &[String]) -> Vec<NumberCheck> {
    numbers
        .iter()
        .filter_map(|n| {
            let norm = normalize(n);
            let (kind, valid) = if norm.starts_with("FR") && norm.len() == 13 {
                ("EU VAT number", vat_is_valid(n))
            } else if norm.len() == 14 && norm.chars().all(|c| c.is_ascii_digit()) {
                ("SIRET", siret_is_valid(n))
            } else if norm.len() == 9 && norm.chars().all(|c| c.is_ascii_digit()) {
                ("SIREN", siren_is_valid(n))
            } else {
                return None;
            };
            Some(NumberCheck {
                number: n.clone(),
                kind,
                valid,
            })
        })
        .collect()
}

/// The register entry for `siren` in the search answer, or None when the
/// answer has no company with exactly that SIREN.
pub fn entry_from(answer: &Value, siren: &str) -> Option<RegisterEntry> {
    let found = answer
        .get("results")?
        .as_array()?
        .iter()
        .find(|r| r.get("siren").and_then(Value::as_str) == Some(siren))?;
    let text = |v: &Value, key: &str| {
        v.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string()
    };
    let names = [
        text(found, "nom_raison_sociale"),
        text(found, "nom_complet"),
    ]
    .into_iter()
    .filter(|n| !n.is_empty())
    .collect();
    let place = found
        .get("siege")
        .map(|s| {
            [text(s, "libelle_commune"), text(s, "code_postal")]
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    Some(RegisterEntry {
        register: REGISTER,
        number: siren.to_string(),
        names,
        // "A" active, "C" closed (cessée).
        active: text(found, "etat_administratif") == "A",
        opened: text(found, "date_creation"),
        place,
    })
}

/// Looks the SIREN up. Some(None): the register has no such company;
/// None: the register did not answer (no card is shown).
pub async fn look_up(siren: &str) -> Option<Option<RegisterEntry>> {
    let response = register_client()?
        .get(format!("{SEARCH_URL}?q={siren}&page=1&per_page=5"))
        .send()
        .await;
    match response {
        Ok(r) if r.status().is_success() => match r.json::<Value>().await {
            Ok(answer) => Some(entry_from(&answer, siren)),
            Err(e) => {
                eprintln!("Safely: French register answer could not be read: {}", e);
                None
            }
        },
        Ok(r) => {
            eprintln!("Safely: French register answered {}", r.status());
            None
        }
        Err(e) => {
            eprintln!("Safely: French register could not be reached: {}", e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn siren_siret_and_vat_check_digits() {
        assert!(siren_is_valid("732 829 320"));
        assert!(!siren_is_valid("123 456 789"));
        assert!(siret_is_valid("732 829 320 00074"));
        assert!(!siret_is_valid("732 829 320 00075"));
        assert!(vat_is_valid("FR 44 732829320"));
        assert!(vat_is_valid("FR40303265045"));
        assert!(!vat_is_valid("FR45732829320"));
        assert_eq!(siren_of("FR44732829320").as_deref(), Some("732829320"));
        assert_eq!(siren_of("73282932000074").as_deref(), Some("732829320"));
    }

    #[test]
    fn only_french_shapes_are_checked() {
        let checks = check_numbers(&["FR44732829320".to_string(), "24AABCE2957Q1Z8".to_string()]);
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].kind, "EU VAT number");
        assert!(checks[0].valid);
    }

    #[test]
    fn the_register_answer_is_read_for_the_exact_siren_only() {
        let answer = json!({ "results": [
            { "siren": "111111118", "nom_complet": "OTHER" },
            { "siren": "732829320", "nom_complet": "ACME OUTILS (ACME)", "nom_raison_sociale": "ACME OUTILS",
              "etat_administratif": "A", "date_creation": "1973-01-01",
              "siege": { "libelle_commune": "LYON", "code_postal": "69002" } }
        ]});
        let e = entry_from(&answer, "732829320").unwrap();
        assert_eq!(e.names, vec!["ACME OUTILS", "ACME OUTILS (ACME)"]);
        assert!(e.active);
        assert_eq!(e.place, "LYON 69002");
        assert!(entry_from(&answer, "542065479").is_none());
    }
}
