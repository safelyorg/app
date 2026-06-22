use crate::services::claude::ClaudeAnalysis;

pub fn calculate_risk_score(analysis: &ClaudeAnalysis) -> i16 {
    let mut score: i16 = 0;

    if analysis.urgency_language.found {
        score += 15;
    }
    if analysis.advance_payment_request.found {
        score += 20;
    }
    if analysis.duplicate_listing.found {
        score += 15;
    }
    if analysis.fraud_pattern_match.found {
        score += 30;
    }
    if analysis.contact_info_in_listing.found {
        score += 10;
    }
    if analysis.price_assessment.verdict != "normal" {
        score += 20;
    }
    if analysis.image_authenticity.verdict != "original" {
        score += 10;
    }

    score.min(100)
}
