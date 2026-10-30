use crate::models::analysis::Signal;

/// Results that mean "nothing could be checked here", not a real
/// finding: the platform didn't show it, the supplier left it out, or
/// the page couldn't be read. These never count as real, usable data.
const NO_DATA_VALUES: [&str; 11] = [
    "Unknown",
    "unknown",
    "Not provided",
    "Not checked",
    "Not offered",
    "Not shown on this platform",
    "Couldn't be loaded",
    "Invalid date",
    "Not found",
    "No website found",
    "No store page found",
];

/// True when this check came back with a real finding (good or bad),
/// false when there was nothing to check.
pub fn has_real_data(signal: &Signal) -> bool {
    !NO_DATA_VALUES.contains(&signal.value.as_str())
}

/// Measures how confident Safely actually is in a given result,
/// separate from the score itself - based on how many of the real
/// signals came back with genuine, meaningful values rather than
/// missing / not-shown / unknown ones.
///
/// It counts how many signals have real data, then buckets that count
/// into High, Medium, or Low, and writes a plain, honest sentence
/// explaining the real number behind the bucket.
pub fn calculate_confidence(signals: &[Signal]) -> (String, String) {
    let meaningful_count = signals.iter().filter(|s| has_real_data(s)).count();
    let total = signals.len();

    let level = if meaningful_count >= 8 {
        "high"
    } else if meaningful_count >= 5 {
        "medium"
    } else {
        "low"
    };

    let reasoning = format!(
        "Based on {} of {} signals returning real, usable data.",
        meaningful_count, total
    );

    (level.to_string(), reasoning)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(value: &str) -> Signal {
        Signal {
            label: "x".to_string(),
            sub: String::new(),
            value: value.to_string(),
            signal_type: "info".to_string(),
            category: String::new(),
            check_type: String::new(),
        }
    }

    #[test]
    fn missing_or_hidden_details_are_not_real_data() {
        for value in [
            "Not provided",
            "Not checked",
            "Not offered",
            "Not shown on this platform",
            "Couldn't be loaded",
            "No website found",
        ] {
            assert!(!has_real_data(&sig(value)), "{value}");
        }
        assert!(has_real_data(&sig("Verified")));
        assert!(has_real_data(&sig("Vague")));
        assert!(has_real_data(&sig("No scam reports")));
    }

    #[test]
    fn a_supplier_that_hides_everything_gets_low_confidence() {
        let signals = vec![
            sig("Not provided"),
            sig("Not checked"),
            sig("No website found"),
            sig("Not offered"),
            sig("Verified"),
            sig("Specific"),
        ];
        let (level, reasoning) = calculate_confidence(&signals);
        assert_eq!(level, "low");
        assert_eq!(
            reasoning,
            "Based on 2 of 6 signals returning real, usable data."
        );
    }
}
