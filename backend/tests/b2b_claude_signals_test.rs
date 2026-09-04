use backend::services::{
    claude::{B2bClaudeAnalysis, Finding, ImageAssessment, PriceAssessment},
    signals::build_b2b_claude_signals,
};

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
        urgency_language: Finding {
            found: false,
            evidence: "no urgency detected".to_string(),
        },
        advance_payment_request: Finding {
            found: false,
            evidence: "no advance payment detected".to_string(),
        },
        image_authenticity: ImageAssessment {
            verdict: "not verified".to_string(),
            reasoning: "no images provided".to_string(),
        },
        overall_risk_notes: "overall notes".to_string(),
    }
}

#[test]
fn produces_exactly_eight_signals() {
    let analysis = make_analysis(true, true, true, true);
    let signals = build_b2b_claude_signals(&analysis);
    assert_eq!(signals.len(), 8);
}

#[test]
fn found_true_maps_to_good_for_b2b_finding_signals() {
    let analysis = make_analysis(true, true, true, true);
    let signals = build_b2b_claude_signals(&analysis);

    for label in [
        "Overall legitimacy check",
        "Registration consistency",
        "Duplicate listing",
        "Contact info",
    ] {
        let signal = signals.iter().find(|s| s.label == label).unwrap();
        assert_eq!(signal.signal_type, "good", "expected 'good' for {}", label);
        assert_eq!(signal.value, "Confirmed");
    }
}

#[test]
fn found_false_maps_to_caution_for_b2b_finding_signals() {
    let analysis = make_analysis(false, false, false, false);
    let signals = build_b2b_claude_signals(&analysis);

    for label in [
        "Overall legitimacy check",
        "Registration consistency",
        "Duplicate listing",
        "Contact info",
    ] {
        let signal = signals.iter().find(|s| s.label == label).unwrap();
        assert_eq!(signal.signal_type, "caution", "expected 'caution' for {}", label);
        assert_eq!(signal.value, "Not confirmed");
    }
}

#[test]
fn mixed_results_are_each_scored_independently() {
    let analysis = make_analysis(true, false, true, false);
    let signals = build_b2b_claude_signals(&analysis);

    let legitimacy = signals.iter().find(|s| s.label == "Overall legitimacy check").unwrap();
    let consistency = signals.iter().find(|s| s.label == "Registration consistency").unwrap();
    let specificity = signals.iter().find(|s| s.label == "Duplicate listing").unwrap();
    let contact = signals.iter().find(|s| s.label == "Contact info").unwrap();

    assert_eq!(legitimacy.signal_type, "good");
    assert_eq!(consistency.signal_type, "caution");
    assert_eq!(specificity.signal_type, "good");
    assert_eq!(contact.signal_type, "caution");
}

#[test]
fn each_signal_carries_the_real_evidence_text() {
    let analysis = make_analysis(true, true, true, true);
    let signals = build_b2b_claude_signals(&analysis);

    let legitimacy = signals.iter().find(|s| s.label == "Overall legitimacy check").unwrap();
    assert_eq!(legitimacy.sub, "legitimacy evidence");
}

#[test]
fn each_signal_has_the_correct_real_category_and_check_type() {
    let analysis = make_analysis(true, true, true, true);
    let signals = build_b2b_claude_signals(&analysis);

    let legitimacy = signals.iter().find(|s| s.label == "Overall legitimacy check").unwrap();
    assert_eq!(legitimacy.category, "company");
    assert_eq!(legitimacy.check_type, "existence");

    let consistency = signals.iter().find(|s| s.label == "Registration consistency").unwrap();
    assert_eq!(consistency.category, "company");
    assert_eq!(consistency.check_type, "consistency");

    let specificity = signals.iter().find(|s| s.label == "Duplicate listing").unwrap();
    assert_eq!(specificity.category, "listing");
    assert_eq!(specificity.check_type, "pattern");

    let contact = signals.iter().find(|s| s.label == "Contact info").unwrap();
    assert_eq!(contact.category, "identity");
    assert_eq!(contact.check_type, "existence");
}

#[test]
fn price_analysis_signal_uses_pricing_transparency_verdict_directly() {
    let analysis = make_analysis(true, true, true, true);
    let signals = build_b2b_claude_signals(&analysis);
    let price = signals.iter().find(|s| s.label == "Price analysis").unwrap();
    assert_eq!(price.value, "normal");
    assert_eq!(price.signal_type, "good");
    assert_eq!(price.sub, "pricing reasoning");
}

#[test]
fn urgency_and_advance_payment_use_the_opposite_polarity_from_the_other_four() {
    let mut analysis = make_analysis(true, true, true, true);
    analysis.urgency_language.found = true;
    analysis.advance_payment_request.found = true;
    let signals = build_b2b_claude_signals(&analysis);

    let urgency = signals.iter().find(|s| s.label == "Urgency language").unwrap();
    assert_eq!(urgency.signal_type, "caution", "found=true must mean CAUTION here, not good");
    assert_eq!(urgency.value, "Detected");

    let advance_payment = signals.iter().find(|s| s.label == "Advance payment request").unwrap();
    assert_eq!(advance_payment.signal_type, "caution");
    assert_eq!(advance_payment.value, "Detected");
}

#[test]
fn urgency_and_advance_payment_show_good_when_not_found() {
    let analysis = make_analysis(true, true, true, true);
    let signals = build_b2b_claude_signals(&analysis);
    let urgency = signals.iter().find(|s| s.label == "Urgency language").unwrap();
    assert_eq!(urgency.signal_type, "good");
    assert_eq!(urgency.value, "None found");
}

#[test]
fn image_authenticity_signal_correctly_maps_not_verified_to_caution() {
    let analysis = make_analysis(true, true, true, true);
    let signals = build_b2b_claude_signals(&analysis);
    let image = signals.iter().find(|s| s.label == "Image authenticity").unwrap();
    assert_eq!(image.value, "not verified");
    assert_eq!(image.signal_type, "caution");
}

#[test]
fn image_authenticity_signal_maps_original_to_good() {
    let mut analysis = make_analysis(true, true, true, true);
    analysis.image_authenticity.verdict = "original".to_string();
    let signals = build_b2b_claude_signals(&analysis);
    let image = signals.iter().find(|s| s.label == "Image authenticity").unwrap();
    assert_eq!(image.signal_type, "good");
}
