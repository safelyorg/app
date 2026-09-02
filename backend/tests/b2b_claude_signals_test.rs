use backend::services::claude::{B2bClaudeAnalysis, Finding, PriceAssessment};
use backend::services::signals::build_b2b_claude_signals;

fn make_analysis(
    legitimacy_found: bool,
    consistency_found: bool,
    specificity_found: bool,
    contact_found: bool,
) -> B2bClaudeAnalysis {
    B2bClaudeAnalysis {
        business_legitimacy: Finding {
            found: legitimacy_found,
            evidence: "legitimacy evidence".to_string(),
        },
        registration_consistency: Finding {
            found: consistency_found,
            evidence: "consistency evidence".to_string(),
        },
        listing_specificity: Finding {
            found: specificity_found,
            evidence: "specificity evidence".to_string(),
        },
        pricing_transparency: PriceAssessment {
            verdict: "normal".to_string(),
            reasoning: "pricing reasoning".to_string(),
        },
        contact_verifiability: Finding {
            found: contact_found,
            evidence: "contact evidence".to_string(),
        },
        overall_risk_notes: "overall notes".to_string(),
    }
}

#[test]
fn produces_exactly_four_signals() {
    let analysis = make_analysis(true, true, true, true);
    let signals = build_b2b_claude_signals(&analysis);
    assert_eq!(signals.len(), 4);
}

#[test]
fn found_true_maps_to_good_not_caution() {
    let analysis = make_analysis(true, true, true, true);
    let signals = build_b2b_claude_signals(&analysis);
    for signal in &signals {
        assert_eq!(
            signal.signal_type, "good",
            "expected 'good' for {}",
            signal.label
        );
        assert_eq!(signal.value, "Confirmed");
    }
}

#[test]
fn found_false_maps_to_caution_not_bad() {
    let analysis = make_analysis(false, false, false, false);
    let signals = build_b2b_claude_signals(&analysis);
    for signal in &signals {
        assert_eq!(
            signal.signal_type, "caution",
            "expected 'caution' for {}",
            signal.label
        );
        assert_eq!(signal.value, "Not confirmed");
    }
}

#[test]
fn mixed_results_are_each_scored_independently() {
    let analysis = make_analysis(true, false, true, false);
    let signals = build_b2b_claude_signals(&analysis);

    let legitimacy = signals
        .iter()
        .find(|s| s.label == "Business legitimacy")
        .unwrap();
    let consistency = signals
        .iter()
        .find(|s| s.label == "Registration consistency")
        .unwrap();
    let specificity = signals
        .iter()
        .find(|s| s.label == "Listing specificity")
        .unwrap();
    let contact = signals
        .iter()
        .find(|s| s.label == "Contact verifiability")
        .unwrap();

    assert_eq!(legitimacy.signal_type, "good");
    assert_eq!(consistency.signal_type, "caution");
    assert_eq!(specificity.signal_type, "good");
    assert_eq!(contact.signal_type, "caution");
}

#[test]
fn each_signal_carries_the_real_evidence_text() {
    let analysis = make_analysis(true, true, true, true);
    let signals = build_b2b_claude_signals(&analysis);

    let legitimacy = signals
        .iter()
        .find(|s| s.label == "Business legitimacy")
        .unwrap();
    assert_eq!(legitimacy.sub, "legitimacy evidence");
}

#[test]
fn each_signal_has_the_correct_real_category_and_check_type() {
    let analysis = make_analysis(true, true, true, true);
    let signals = build_b2b_claude_signals(&analysis);

    let legitimacy = signals
        .iter()
        .find(|s| s.label == "Business legitimacy")
        .unwrap();
    assert_eq!(legitimacy.category, "company");
    assert_eq!(legitimacy.check_type, "existence");

    let consistency = signals
        .iter()
        .find(|s| s.label == "Registration consistency")
        .unwrap();
    assert_eq!(consistency.category, "company");
    assert_eq!(consistency.check_type, "consistency");

    let specificity = signals
        .iter()
        .find(|s| s.label == "Listing specificity")
        .unwrap();
    assert_eq!(specificity.category, "listing");
    assert_eq!(specificity.check_type, "pattern");

    let contact = signals
        .iter()
        .find(|s| s.label == "Contact verifiability")
        .unwrap();
    assert_eq!(contact.category, "identity");
    assert_eq!(contact.check_type, "existence");
}
