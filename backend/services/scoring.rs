use crate::services::claude::ClaudeAnalysis;

/// It adds up points for every warning sign found, then caps the total at 100,
/// producing the final risk score shown on the dashboard.
///
/// It starts at zero, adds points for each Claude finding, if it was detected,
/// adds points for the two "verdict-based" checks, adds points based on
/// real fraud report history, and caps the final total at 100.
pub fn calculate_risk_score(analysis: &ClaudeAnalysis, fraud_count: i64) -> i16 {
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

    score += match fraud_count {
        0 => 0,
        1 => 20,
        2 => 35,
        _ => 50,
    };

    score.min(100)
}
