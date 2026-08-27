use wasm::{
    analyze_signals, build_activity_bars, build_signal_rows, escape_html, risk_desc, risk_label,
    risk_level, verification_badge,
};

#[test]
fn risk_level_boundaries() {
    assert_eq!(risk_level(0), "low");
    assert_eq!(risk_level(33), "low");
    assert_eq!(risk_level(34), "caution");
    assert_eq!(risk_level(66), "caution");
    assert_eq!(risk_level(67), "high");
    assert_eq!(risk_level(100), "high");
}

#[test]
fn risk_label_all_branches() {
    assert_eq!(risk_label("low"), "Low risk");
    assert_eq!(risk_label("caution"), "Caution");
    assert_eq!(risk_label("high"), "High risk");
    assert_eq!(risk_label("anything_else"), "High risk");
}

#[test]
fn risk_desc_all_branches() {
    assert_eq!(risk_desc("low"), "Safe to proceed");
    assert_eq!(risk_desc("caution"), "Review before proceeding");
    assert_eq!(risk_desc("high"), "High risk detected");
    assert_eq!(risk_desc("anything_else"), "High risk detected");
}

#[test]
fn escape_html_escapes_ampersand() {
    assert_eq!(escape_html("a & b"), "a &amp; b");
}

#[test]
fn escape_html_escapes_angle_brackets() {
    assert_eq!(escape_html("<script>"), "&lt;script&gt;");
}

#[test]
fn escape_html_escapes_quotes() {
    assert_eq!(escape_html(r#"say "hi""#), "say &quot;hi&quot;");
    assert_eq!(escape_html("it's"), "it&#39;s");
}

#[test]
fn escape_html_leaves_normal_text_unchanged() {
    assert_eq!(
        escape_html("Suzuki GR 150 - Excellent Condition"),
        "Suzuki GR 150 - Excellent Condition"
    );
}

#[test]
fn escape_html_does_not_double_escape_ampersand_from_other_replacements() {
    let result = escape_html("<");
    assert_eq!(result, "&lt;");
    assert!(!result.contains("&amp;lt;"));
}

#[test]
fn escape_html_handles_empty_string() {
    assert_eq!(escape_html(""), "");
}

#[test]
fn build_activity_bars_all_zeros_uses_minimum_height_and_low_opacity() {
    let result = build_activity_bars(&[0, 0, 0]);
    assert_eq!(result.matches("height:2px").count(), 3);
    assert_eq!(result.matches("opacity:0.15").count(), 3);
}

#[test]
fn build_activity_bars_max_value_reaches_full_height_and_opacity() {
    let result = build_activity_bars(&[10, 100]);
    assert!(result.contains("height:44px;opacity:1.00"));
    assert!(result.contains("height:4px;opacity:0.37"));
}

#[test]
fn build_activity_bars_single_nonzero_value_becomes_the_max() {
    let result = build_activity_bars(&[0, 0, 5, 0]);
    assert!(result.contains("height:44px;opacity:1.00"));
    assert_eq!(result.matches("height:2px;opacity:0.15").count(), 3);
}

#[test]
fn build_activity_bars_handles_empty_input_without_panicking() {
    let result = build_activity_bars(&[]);
    assert_eq!(result, "");
}

#[test]
fn build_activity_bars_includes_the_real_visit_count_in_each_bar() {
    let result = build_activity_bars(&[7]);
    assert!(result.contains(">7<"));
}

#[test]
fn analyze_signals_zero_bad_returns_low() {
    let json = r#"[{"label":"a","value":"v","type":"good"}]"#;
    let result = analyze_signals(json);
    assert!(result.contains(r#""level":"low""#));
    assert!(result.contains("All 1 signals checked. No red flags detected."));
}

#[test]
fn analyze_signals_one_bad_returns_caution() {
    let json =
        r#"[{"label":"a","value":"v","type":"good"},{"label":"b","value":"v","type":"bad"}]"#;
    let result = analyze_signals(json);
    assert!(result.contains(r#""level":"caution""#));
    assert!(result.contains("1 of 2 signals need your attention"));
}

#[test]
fn analyze_signals_two_or_more_bad_returns_high() {
    let json = r#"[{"label":"a","value":"v","type":"bad"},{"label":"b","value":"v","type":"bad"}]"#;
    let result = analyze_signals(json);
    assert!(result.contains(r#""level":"high""#));
    assert!(result.contains("2 of 2 signals need your attention"));
}

#[test]
fn analyze_signals_caution_type_counts_as_bad_too() {
    let json = r#"[{"label":"a","value":"v","type":"caution"}]"#;
    let result = analyze_signals(json);
    assert!(result.contains(r#""level":"caution""#));
    assert!(result.contains("1 of 1 signals"));
}

#[test]
fn analyze_signals_malformed_json_falls_back_to_empty() {
    let result = analyze_signals("not valid json{{{");
    assert!(result.contains(r#""level":"low""#));
    assert!(result.contains("All 0 signals checked"));
}

#[test]
fn analyze_signals_output_is_genuinely_valid_json() {
    let json = r#"[{"label":"a","value":"v","type":"good"}]"#;
    let result = analyze_signals(json);
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("expected genuinely valid JSON output");
    assert_eq!(parsed["level"], "low");
}

#[test]
fn verification_badge_verified() {
    let result = verification_badge("verified");
    assert!(result.contains("safely-dot-blue"));
    assert!(result.contains("safely-badge-verified"));
    assert!(result.contains("Safely Verified"));
}

#[test]
fn verification_badge_reported() {
    let result = verification_badge("reported");
    assert!(result.contains("safely-dot-red"));
    assert!(result.contains("safely-badge-reported"));
    assert!(result.contains("Reported"));
}

#[test]
fn verification_badge_unknown_fallback() {
    let result = verification_badge("something_genuinely_unrecognized");
    assert!(result.contains("safely-dot-gray"));
    assert!(result.contains("safely-badge-unknown"));
    assert!(result.contains("Unknown"));
}

#[test]
fn verification_badge_never_echoes_the_raw_input_directly() {
    let malicious_input = "<script>alert(1)</script>";
    let result = verification_badge(malicious_input);
    assert!(!result.contains(malicious_input));
    assert!(result.contains("Unknown"));
}

#[test]
fn build_signal_rows_uses_correct_color_per_type() {
    let json = r#"[
        {"label":"a","value":"v","type":"good"},
        {"label":"b","value":"v","type":"caution"},
        {"label":"c","value":"v","type":"info"},
        {"label":"d","value":"v","type":"bad"}
    ]"#;
    let result = build_signal_rows(json);
    assert!(result.contains("#35d0a6"));
    assert!(result.contains("#f2b84c"));
    assert!(result.contains("#6fb3ef"));
    assert!(result.contains("#ff5d5d"));
}

#[test]
fn build_signal_rows_escapes_dangerous_label_value_and_sub() {
    let json = r#"[{"label":"<script>bad</script>","value":"<img>","sub":"a & b","type":"good"}]"#;
    let result = build_signal_rows(json);
    assert!(!result.contains("<script>bad</script>"));
    assert!(result.contains("&lt;script&gt;bad&lt;/script&gt;"));
    assert!(!result.contains("<img>"));
    assert!(result.contains("&lt;img&gt;"));
    assert!(result.contains("a &amp; b"));
}

#[test]
fn build_signal_rows_joins_multiple_signals_together() {
    let json = r#"[
        {"label":"first","value":"v1","type":"good"},
        {"label":"second","value":"v2","type":"bad"}
    ]"#;
    let result = build_signal_rows(json);
    assert!(result.contains("first"));
    assert!(result.contains("second"));
    assert_eq!(result.matches("safely-check-card").count(), 2);
}

#[test]
fn build_signal_rows_handles_empty_input() {
    let result = build_signal_rows("[]");
    assert_eq!(result, "");
}

#[test]
fn build_signal_rows_malformed_json_falls_back_to_empty() {
    let result = build_signal_rows("not valid json");
    assert_eq!(result, "");
}
