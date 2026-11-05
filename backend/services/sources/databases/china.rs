//! China: the Unified Social Credit Code (统一社会信用代码), 18 letters
//! and digits on every business licence since 2015. The last character
//! is a check character (standard GB 32100-2015), so a made-up code can
//! be spotted inside Safely, without asking anyone.
//!
//! China's own register (gsxt.gov.cn) has no API: looking the code up
//! there is a paid step for later (standards/databases/china.json).

use crate::services::sources::databases::numbers::{NumberCheck, normalize};

/// The 31 characters the code uses (no I, O, S, V or Z).
const CHARACTERS: &str = "0123456789ABCDEFGHJKLMNPQRTUWXY";
const WEIGHTS: [u32; 17] = [
    1, 3, 9, 27, 19, 26, 16, 17, 20, 29, 25, 13, 8, 24, 10, 30, 28,
];
pub const KIND: &str = "Unified Social Credit Code";

/// true when the 18-character code's check character is right.
pub fn uscc_is_valid(code: &str) -> bool {
    let code = normalize(code);
    if code.len() != 18 {
        return false;
    }
    let values: Option<Vec<u32>> = code
        .chars()
        .map(|c| CHARACTERS.find(c).map(|i| i as u32))
        .collect();
    let Some(values) = values else {
        return false;
    };
    let sum: u32 = values[..17].iter().zip(WEIGHTS).map(|(v, w)| v * w).sum();
    let check = (31 - sum % 31) % 31;
    values[17] == check
}

/// The China numbers among `numbers`: only 18-character ones are the
/// code (older 15- or 21-digit registration numbers are left out).
pub fn check_numbers(numbers: &[String]) -> Vec<NumberCheck> {
    numbers
        .iter()
        .filter(|n| normalize(n).len() == 18)
        .map(|n| NumberCheck {
            number: n.clone(),
            kind: KIND,
            valid: uscc_is_valid(n),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_codes_pass_and_changed_ones_fail() {
        // From a real Kompass company page (Oct 2026).
        assert!(uscc_is_valid("91321322MACQ3B3D4L"));
        assert!(!uscc_is_valid("91321322MACQ3B3D4M"));
        assert!(!uscc_is_valid("91321322MACQ3B3D4"));
        // I, O, S, V and Z are never used.
        assert!(!uscc_is_valid("91321322MACQ3B3DIL"));
    }

    #[test]
    fn only_18_character_numbers_are_checked() {
        let checks = check_numbers(&[
            "321322000202308100316".to_string(),
            "91321322MACQ3B3D4L".to_string(),
        ]);
        assert_eq!(checks.len(), 1);
        assert!(checks[0].valid);
        assert_eq!(checks[0].kind, KIND);
    }

    #[test]
    fn real_codes_from_global_sources_pass() {
        // Fujian New Century Communications (Nov 5) and Boxin (Huizhou).
        assert!(uscc_is_valid("913505037513705716"));
        assert!(uscc_is_valid("91441303594089865C"));
    }
}
