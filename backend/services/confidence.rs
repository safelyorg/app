use crate::models::analysis::Signal;

/// Measures how confident Safely actually is in a given result,
/// separate from the score itself - based on how many of the real
/// signals came back with genuine, meaningful values rather than
/// unknown/fallback defaults.
///
/// It counts how many signals have a real, non-"Unknown" value, then
/// buckets that count into High, Medium, or Low, and writes a plain,
/// honest sentence explaining the real number behind the bucket.
pub fn calculate_confidence(signals: &[Signal]) -> (String, String) {
    let meaningful_count = signals.iter().filter(|s| s.value != "Unknown").count();
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
