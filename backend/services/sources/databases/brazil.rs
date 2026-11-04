//! Brazil: the CNPJ, the company number of every Brazilian company, 14
//! characters (written 12.345.678/0001-95). The last two are check
//! digits, so a made-up CNPJ is spotted inside Safely. Since July 2026
//! the first 12 may also be letters ("12.ABC.345/01DE-35"); the same
//! rule covers both (each character counts as its code minus 48).
//!
//! The Receita Federal register is read through BrasilAPI (free, no
//! key): https://brasilapi.com.br/api/cnpj/v1/{cnpj}

use crate::services::sources::databases::numbers::{
    NumberCheck, RegisterEntry, normalize, register_client,
};
use serde_json::Value;

pub const KIND: &str = "CNPJ";
pub const REGISTER: &str = "Receita Federal (Brazil) company register";
const LOOKUP_URL: &str = "https://brasilapi.com.br/api/cnpj/v1";

fn check_digit(values: &[u32]) -> u32 {
    // Weights 2..=9 from the right, starting again after 9.
    let sum: u32 = values
        .iter()
        .rev()
        .enumerate()
        .map(|(i, v)| v * (2 + (i as u32 % 8)))
        .sum();
    match sum % 11 {
        0 | 1 => 0,
        r => 11 - r,
    }
}

/// true when the CNPJ's two check digits are right.
pub fn cnpj_is_valid(raw: &str) -> bool {
    let cnpj = normalize(raw);
    if cnpj.len() != 14 || !cnpj[12..].chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    // "00000000000000" and the like pass the sum but are not real.
    if cnpj
        .chars()
        .all(|c| c == cnpj.chars().next().unwrap_or('0'))
    {
        return false;
    }
    let values: Vec<u32> = cnpj.chars().map(|c| c as u32 - 48).collect();
    let first = check_digit(&values[..12]);
    let mut with_first = values[..12].to_vec();
    with_first.push(first);
    let second = check_digit(&with_first);
    values[12] == first && values[13] == second
}

/// The CNPJs among `numbers` (14 letters and digits).
pub fn check_numbers(numbers: &[String]) -> Vec<NumberCheck> {
    numbers
        .iter()
        .filter(|n| normalize(n).len() == 14)
        .map(|n| NumberCheck {
            number: n.clone(),
            kind: KIND,
            valid: cnpj_is_valid(n),
        })
        .collect()
}

/// The register entry from BrasilAPI's answer.
pub fn entry_from(answer: &Value, number: &str) -> RegisterEntry {
    let text = |key: &str| {
        answer
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string()
    };
    let names = [text("razao_social"), text("nome_fantasia")]
        .into_iter()
        .filter(|n| !n.is_empty())
        .collect();
    let place = [text("municipio"), text("uf")]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    RegisterEntry {
        register: REGISTER,
        number: number.to_string(),
        names,
        active: text("descricao_situacao_cadastral").eq_ignore_ascii_case("ATIVA"),
        opened: text("data_inicio_atividade"),
        place,
    }
}

/// Looks the CNPJ up. Some(None): the register has no such company;
/// None: the register did not answer (no card is shown).
pub async fn look_up(cnpj: &str) -> Option<Option<RegisterEntry>> {
    let number = normalize(cnpj);
    let response = register_client()?
        .get(format!("{LOOKUP_URL}/{number}"))
        .send()
        .await;
    match response {
        Ok(r) if r.status().as_u16() == 404 => Some(None),
        Ok(r) if r.status().is_success() => match r.json::<Value>().await {
            Ok(answer) => Some(Some(entry_from(&answer, &number))),
            Err(e) => {
                eprintln!("Safely: BrasilAPI answer could not be read: {}", e);
                None
            }
        },
        Ok(r) => {
            eprintln!("Safely: BrasilAPI answered {} for a CNPJ", r.status());
            None
        }
        Err(e) => {
            eprintln!("Safely: BrasilAPI could not be reached: {}", e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn check_digits_of_numeric_and_letter_cnpjs() {
        assert!(cnpj_is_valid("11.222.333/0001-81"));
        assert!(!cnpj_is_valid("11.222.333/0001-82"));
        // The example of the new letter CNPJ (Receita Federal, 2026).
        assert!(cnpj_is_valid("12.ABC.345/01DE-35"));
        assert!(!cnpj_is_valid("12.ABC.345/01DE-36"));
        assert!(!cnpj_is_valid("00.000.000/0000-00"));
        assert!(!cnpj_is_valid("11.222.333/0001"));
    }

    #[test]
    fn the_register_answer_is_read() {
        let answer = json!({
            "cnpj": "11222333000181",
            "razao_social": "ACME FERRAMENTAS LTDA",
            "nome_fantasia": "",
            "descricao_situacao_cadastral": "ATIVA",
            "data_inicio_atividade": "2010-05-03",
            "municipio": "SAO PAULO",
            "uf": "SP"
        });
        let e = entry_from(&answer, "11222333000181");
        assert_eq!(e.names, vec!["ACME FERRAMENTAS LTDA"]);
        assert!(e.active);
        assert_eq!(e.place, "SAO PAULO, SP");
        assert_eq!(e.opened, "2010-05-03");
        let closed = entry_from(
            &json!({"razao_social": "X", "descricao_situacao_cadastral": "BAIXADA"}),
            "1",
        );
        assert!(!closed.active);
    }
}
