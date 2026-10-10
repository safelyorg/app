use crate::models::{analysis::Signal, risk_factors::RiskFactor};
use crate::services::confidence::calculate_confidence;
use crate::services::signals::{
    COMMODITY_SCAM_PATTERN, FULL_PREPAYMENT, REGULATED_NOT_MAKER, UNTRACEABLE_PAYMENT,
};

/// The risk factor shown when Safely could check too little about a
/// supplier to trust a low score (see not_enough_information_factor).
pub const NOT_ENOUGH_INFORMATION: &str = "not_enough_information";

/// True only for a genuinely young account/company - an age measured in
/// days, weeks or months ("This month", "3 months"), or a company
/// "Founded this year". Any other age with "year" in it is not new, and "Unknown", "Not provided" or
/// "Invalid date" mean the age is missing, which is not the same as new.
pub fn is_new_account(account_age: &str) -> bool {
    let age = account_age.to_lowercase();
    if age.contains("year") && !age.contains("this year") {
        return false;
    }
    age == "this month"
        || age.contains("this year")
        || age.contains("month")
        || age.contains("week")
        || age.contains("day")
}

pub fn find_signal<'a>(signals: &'a [Signal], label: &str) -> Option<&'a Signal> {
    signals.iter().find(|s| s.label == label)
}

/// A signal counts as a problem only when it is "caution" or "bad".
/// "info" (e.g. an image that was not checked, a missing founding
/// year) is neutral and must never trigger a risk factor.
fn is_flagged(signal: &Signal) -> bool {
    signal.signal_type == "caution" || signal.signal_type == "bad"
}

/// How many scam reports from Safely users make the "Safely history"
/// line a Serious problem. Two matches the seller's fraud-report line,
/// which already calls 2 or more reports a "High risk seller".
const MIN_REPORTS_FOR_SERIOUS: u32 = 2;

/// Reads the number from the "Safely history" line:
/// "Reported once" -> 1, "Reported 3 times" -> 3, "No scam reports" -> 0.
/// Older saved scans counted earlier checks instead ("Checked once
/// before", "Checked 4 times before", "4 prior checks"); those are still
/// read the same way. Returns 0 when no number can be found.
fn history_count(signal: &Signal) -> u32 {
    let text = format!("{} {}", signal.value, signal.sub).to_lowercase();
    if text.contains("reported once") || text.contains("checked once before") {
        return 1;
    }
    if let Some(pos) = text.find("reported ") {
        let after = &text[pos + "reported ".len()..];
        if let Some(n) = after
            .split_whitespace()
            .next()
            .and_then(|w| w.parse::<u32>().ok())
        {
            return n;
        }
    }
    let marker = if text.contains(" times before") {
        " times before"
    } else if text.contains("prior check") {
        "prior check"
    } else {
        return 0;
    };
    let pos = text.find(marker).unwrap_or(0);
    text[..pos]
        .split_whitespace()
        .last()
        .and_then(|w| w.parse::<u32>().ok())
        .unwrap_or(0)
}

/// Claude's own explanation when there is one, otherwise the fallback.
fn evidence_or(signal: &Signal, fallback: &str) -> String {
    if signal.sub.trim().is_empty() {
        fallback.to_string()
    } else {
        signal.sub.clone()
    }
}

/// Translates the now-labeled, categorized signals into real, named
/// conclusions - the actual "what does this mean" layer, sitting on
/// top of the raw "what did we find" signals.
///
/// Uses signal_type ("good"/"caution"/"bad") rather than exact value
/// text to decide severity - this is what makes these rules genuinely
/// platform-agnostic, since OLX and B2B use different literal words
/// ("Detected" vs "Not confirmed") for the same underlying bad outcome.
///
/// On B2B scans the severity decides the lowest score (b2b_risk_score
/// in analysis.rs): any Serious factor -> at least 67 (High), any
/// Pattern match -> at least 45 (Moderate).
pub fn derive_risk_factors(signals: &[Signal]) -> Vec<RiskFactor> {
    let mut factors = Vec::new();
    let mut covered_labels: Vec<&str> = Vec::new();

    // Hard factors

    // The buyer is not on the marketplace's real website (a look-alike
    // address such as "a1ibaba.com"). Anything paid or typed in here
    // can be stolen, whatever the supplier looks like.
    if let Some(s) = find_signal(signals, "Domain check") {
        if is_flagged(s) {
            factors.push(RiskFactor {
                severity: "hard".to_string(),
                name: "fake_marketplace_domain".to_string(),
                description: evidence_or(
                    s,
                    "This page is not on the marketplace's real website. It may be a fake copy made to take payments or logins.",
                ),
                contributing_signals: vec!["Domain check".to_string()],
            });
            covered_labels.push("Domain check");
        }
    }

    if let Some(s) = find_signal(signals, "Overall legitimacy check") {
        if is_flagged(s) {
            // A very young account on top of a legitimacy concern is a
            // well-known scam combination - named in the same factor
            // rather than as a second one.
            let young =
                find_signal(signals, "Account age").filter(|age| is_new_account(&age.value));
            let mut contributing = vec!["Overall legitimacy check".to_string()];
            let mut description = evidence_or(
                s,
                "A specific, known fraud pattern or legitimacy concern was identified in this listing.",
            );
            if let Some(age) = young {
                contributing.push("Account age".to_string());
                description = format!(
                    "{} The account is also very new ({}), a combination often seen in scam listings.",
                    description, age.value
                );
                covered_labels.push("Account age");
            }
            factors.push(RiskFactor {
                severity: "hard".to_string(),
                name: "confirmed_legitimacy_concern".to_string(),
                description,
                contributing_signals: contributing,
            });
            covered_labels.push("Overall legitimacy check");
        }
    }

    // Scam reports from Safely users. Two or more reports make this a
    // Serious problem. A single report stays "Worth noting" (it falls
    // through to the soft factors below) - one person's report could be
    // a mistake or a dispute.
    if let Some(s) = find_signal(signals, "Safely history") {
        if is_flagged(s) && history_count(s) >= MIN_REPORTS_FOR_SERIOUS {
            factors.push(RiskFactor {
                severity: "hard".to_string(),
                name: "network_confirmed_high_risk_seller".to_string(),
                description: evidence_or(
                    s,
                    "Several Safely users have reported this seller as a scam.",
                ),
                contributing_signals: vec!["Safely history".to_string()],
            });
            covered_labels.push("Safely history");
        }
    }

    // Compound factors
    // B2C calls this card "Duplicate listing"; B2B calls it "Listing
    // detail" (vague / templated description).
    let duplicate = find_signal(signals, "Duplicate listing")
        .or_else(|| find_signal(signals, "Listing detail"));
    let image_auth = find_signal(signals, "Image authenticity");
    if let (Some(d), Some(i)) = (duplicate, image_auth) {
        // The listing must be flagged, and the photos must be ones
        // Claude looked at but could not confirm ("not verified" - an
        // "info" card on its own). Photos that were simply not checked
        // (images switched off) never count. is_flagged keeps older
        // saved results, where "not verified" was a caution, working.
        let photos_unconfirmed = i.value.eq_ignore_ascii_case("not verified") || is_flagged(i);
        if is_flagged(d) && photos_unconfirmed {
            // B2B "Listing detail" only means the description is vague -
            // no copy of the listing was searched for, so the text must
            // not say "duplicate".
            let description = if d.label == "Listing detail" {
                "The listing is vague and its photos can't be confirmed as the supplier's own, so the product may not really exist or may not be what is shown."
            } else {
                "A templated, duplicate-style listing combined with unverifiable images suggests the product itself may not genuinely exist or be authentic."
            };
            factors.push(RiskFactor {
                severity: "compound".to_string(),
                name: "likely_counterfeit_or_nonexistent_product".to_string(),
                description: description.to_string(),
                contributing_signals: vec![d.label.clone(), "Image authenticity".to_string()],
            });
            covered_labels.push(d.label.as_str());
            covered_labels.push("Image authenticity");
        }
    }

    // Pressure to hurry + payment before delivery. When the payment is
    // untraceable (Western Union, MoneyGram, crypto...), that payment is
    // Serious on its own, so the pair is Serious too - adding pressure
    // must never make a supplier look safer than the payment alone.
    let urgency = find_signal(signals, "Urgency language");
    let advance_payment = find_signal(signals, "Advance payment request");
    if let (Some(u), Some(a)) = (urgency, advance_payment) {
        if is_flagged(u) && is_flagged(a) {
            let severity = if a.value == UNTRACEABLE_PAYMENT {
                "hard"
            } else {
                "compound"
            };
            factors.push(RiskFactor {
                severity: severity.to_string(),
                name: "advance_fee_scam_pattern".to_string(),
                description: "This listing combines pressure/urgency language with a request for payment before delivery - a classic advance-fee scam pattern.".to_string(),
                contributing_signals: vec!["Urgency language".to_string(), "Advance payment request".to_string()],
            });
            covered_labels.push("Urgency language");
            covered_labels.push("Advance payment request");
        }
    }

    // Full payment before shipment (by a normal method such as bank
    // transfer) to a brand-new company: a combined flag - the buyer
    // carries all the risk with a company that has no track record.
    if let Some(a) = advance_payment {
        if is_flagged(a)
            && a.value == FULL_PREPAYMENT
            && !covered_labels.contains(&"Advance payment request")
        {
            let young =
                find_signal(signals, "Account age").filter(|age| is_new_account(&age.value));
            if let Some(age) = young {
                factors.push(RiskFactor {
                    severity: "compound".to_string(),
                    name: "full_prepayment_to_new_company".to_string(),
                    description: format!(
                        "{} The company is also very new ({}), so there is no track record to rely on if the goods never arrive.",
                        evidence_or(a, "The supplier asks for full payment before shipment."),
                        age.value
                    ),
                    contributing_signals: vec![
                        "Advance payment request".to_string(),
                        "Account age".to_string(),
                    ],
                });
                covered_labels.push("Advance payment request");
                covered_labels.push("Account age");
            }
        }
    }

    // Untraceable payment methods (Western Union / MoneyGram / crypto /
    // gift cards / a personal account) are serious even without urgency
    // language: once paid, the money cannot be recovered. Full
    // prepayment by bank transfer is NOT treated this way - on its own
    // it stays "Worth noting" (it falls through to the soft factors).
    if let Some(a) = advance_payment {
        if is_flagged(a)
            && a.value != FULL_PREPAYMENT
            && !covered_labels.contains(&"Advance payment request")
        {
            factors.push(RiskFactor {
                severity: "hard".to_string(),
                name: "unsafe_payment_terms".to_string(),
                description: evidence_or(
                    a,
                    "The payment terms asked for would leave the buyer with no way to recover the money.",
                ),
                contributing_signals: vec!["Advance payment request".to_string()],
            });
            covered_labels.push("Advance payment request");
        }
    }

    // A price that doesn't add up (far too low, or otherwise
    // implausible) AND the full price paid before shipment: the most
    // common supplier scam - "cheap price, pay first". Only reached when
    // the payment is not already covered by a stronger rule above
    // (untraceable method, urgency, or a brand-new company).
    if let (Some(p), Some(a)) = (find_signal(signals, "Price analysis"), advance_payment) {
        if is_flagged(p)
            && is_flagged(a)
            && !covered_labels.contains(&"Advance payment request")
            && !covered_labels.contains(&"Price analysis")
        {
            factors.push(RiskFactor {
                severity: "compound".to_string(),
                name: "implausible_price_with_full_prepayment".to_string(),
                description: format!(
                    "{} {} Paying everything upfront at a price that doesn't add up is the most common supplier scam.",
                    evidence_or(p, "The price does not look right for this product."),
                    evidence_or(a, "The supplier asks for full payment before shipment."),
                ),
                contributing_signals: vec![
                    "Price analysis".to_string(),
                    "Advance payment request".to_string(),
                ],
            });
            covered_labels.push("Price analysis");
            covered_labels.push("Advance payment request");
        }
    }

    // A licence-only product (botox, fillers, prescription medicine)
    // sold by a company that calls itself the maker of another company's
    // brand: a combined flag. Fakes of these products are dangerous, and
    // a false manufacturer claim is a common sign of a fake source. A
    // regulated product on its own stays "Worth noting" (soft factor).
    if let Some(r) = find_signal(signals, "Regulated product") {
        if is_flagged(r) && r.value == REGULATED_NOT_MAKER {
            factors.push(RiskFactor {
                severity: "compound".to_string(),
                name: "regulated_product_from_non_maker".to_string(),
                description: format!(
                    "{} Products like this should only be bought from the brand owner or its authorised distributor.",
                    evidence_or(r, "This product needs a licence to sell, and the seller is not its maker.")
                ),
                contributing_signals: vec!["Regulated product".to_string()],
            });
            covered_labels.push("Regulated product");
        }
    }

    // Classic bait products of fake commodity deals (ICUMSA 45 sugar,
    // EN590 diesel, Urea 46...) offered together, or by a company that
    // says it sells everything: the buyer pays fees or a deposit and
    // nothing ships. Serious on its own. One bait product alone is only
    // an "info" card and never reaches here.
    if let Some(p) = find_signal(signals, "Product range") {
        if is_flagged(p) && p.value == COMMODITY_SCAM_PATTERN {
            factors.push(RiskFactor {
                severity: "hard".to_string(),
                name: "commodity_scam_pattern".to_string(),
                description: evidence_or(
                    p,
                    "This company offers products that are common bait in fake commodity deals.",
                ),
                contributing_signals: vec!["Product range".to_string()],
            });
            covered_labels.push("Product range");
        }
    }

    // Soft factors - anything caution/bad-type not already covered above
    for signal in signals {
        if is_flagged(signal) && !covered_labels.contains(&signal.label.as_str()) {
            factors.push(RiskFactor {
                severity: "soft".to_string(),
                name: format!("{}_flagged", signal.label.to_lowercase().replace(' ', "_")),
                description: signal.sub.clone(),
                contributing_signals: vec![signal.label.clone()],
            });
        }
    }

    factors
}

/// Shown on a B2B scan when Safely could check too little about the
/// supplier: most checks came back empty because the page shows almost
/// nothing (no founding year, no details, nothing to verify). A low
/// score would then only mean "nothing was found", not "this supplier
/// is fine", so the scan is lifted to Moderate (see analysis.rs) and
/// this note says why. Uses the same count as the confidence level.
pub fn not_enough_information_factor(signals: &[Signal]) -> Option<RiskFactor> {
    let (level, reasoning) = calculate_confidence(signals);
    if level != "low" {
        return None;
    }
    Some(RiskFactor {
        severity: "soft".to_string(),
        name: NOT_ENOUGH_INFORMATION.to_string(),
        description: format!(
            "Safely could check very little about this supplier. {} With this little information, a low score does not mean the supplier is safe - check them carefully yourself before paying.",
            reasoning
        ),
        contributing_signals: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(label: &str, value: &str, signal_type: &str, sub: &str) -> Signal {
        Signal {
            label: label.to_string(),
            sub: sub.to_string(),
            value: value.to_string(),
            signal_type: signal_type.to_string(),
            category: String::new(),
            check_type: String::new(),
        }
    }

    fn find<'a>(f: &'a [RiskFactor], name: &str) -> Option<&'a RiskFactor> {
        f.iter().find(|x| x.name == name)
    }

    #[test]
    fn implausible_price_with_full_prepayment_is_a_pattern_match() {
        let s = vec![
            sig(
                "Price analysis",
                "suspiciously low",
                "caution",
                "Far below market.",
            ),
            sig(
                "Advance payment request",
                FULL_PREPAYMENT,
                "caution",
                "100% before shipment.",
            ),
        ];
        let f = derive_risk_factors(&s);
        assert_eq!(f.len(), 1, "one combined factor");
        let c = find(&f, "implausible_price_with_full_prepayment").unwrap();
        assert_eq!(c.severity, "compound");
        assert!(
            c.description
                .starts_with("Far below market. 100% before shipment.")
        );
    }

    #[test]
    fn implausible_price_never_weakens_an_untraceable_payment() {
        let s = vec![
            sig("Price analysis", "suspiciously low", "caution", ""),
            sig(
                "Advance payment request",
                UNTRACEABLE_PAYMENT,
                "caution",
                "",
            ),
        ];
        let f = derive_risk_factors(&s);
        assert_eq!(find(&f, "unsafe_payment_terms").unwrap().severity, "hard");
        assert!(find(&f, "implausible_price_with_full_prepayment").is_none());
        assert_eq!(find(&f, "price_analysis_flagged").unwrap().severity, "soft");
    }

    #[test]
    fn normal_price_with_full_prepayment_stays_worth_noting() {
        let s = vec![
            sig("Price analysis", "normal", "good", ""),
            sig("Advance payment request", FULL_PREPAYMENT, "caution", ""),
        ];
        let f = derive_risk_factors(&s);
        assert!(find(&f, "implausible_price_with_full_prepayment").is_none());
        assert_eq!(
            find(&f, "advance_payment_request_flagged")
                .unwrap()
                .severity,
            "soft"
        );
    }

    #[test]
    fn little_information_gives_the_not_enough_information_note() {
        let mostly_empty = vec![
            sig("Account age", "Not provided", "caution", ""),
            sig("Seller website check", "No website found", "info", ""),
            sig("Platform verification", "Not offered", "info", ""),
            sig("Image authenticity", "Not checked", "info", ""),
            sig(
                "Listing completeness",
                "Not shown on this platform",
                "info",
                "",
            ),
            sig("Price analysis", "normal", "good", ""),
        ];
        let f = not_enough_information_factor(&mostly_empty).expect("note");
        assert_eq!(f.name, NOT_ENOUGH_INFORMATION);
        assert_eq!(f.severity, "soft");
        assert!(f.description.contains("Based on 1 of 6 signals"));

        let full: Vec<Signal> = (0..9)
            .map(|i| sig("x", &format!("v{i}"), "good", ""))
            .collect();
        assert!(not_enough_information_factor(&full).is_none());
    }

    #[test]
    fn fake_marketplace_domain_is_serious() {
        let f = derive_risk_factors(&[sig(
            "Domain check",
            "Suspicious",
            "bad",
            "This does not match Alibaba's real domain.",
        )]);
        assert_eq!(f.len(), 1, "one factor, not also a soft one");
        let d = find(&f, "fake_marketplace_domain").unwrap();
        assert_eq!(d.severity, "hard");
        assert_eq!(d.description, "This does not match Alibaba's real domain.");
    }

    #[test]
    fn real_marketplace_domain_is_never_a_risk_factor() {
        let f = derive_risk_factors(&[sig("Domain check", "Verified", "good", "")]);
        assert!(f.is_empty());
    }

    #[test]
    fn urgency_plus_untraceable_payment_is_serious_not_just_a_pattern() {
        let s = vec![
            sig("Urgency language", "Detected", "caution", ""),
            sig(
                "Advance payment request",
                UNTRACEABLE_PAYMENT,
                "caution",
                "Western Union only.",
            ),
        ];
        let f = derive_risk_factors(&s);
        assert_eq!(f.len(), 1, "one combined factor");
        let p = find(&f, "advance_fee_scam_pattern").unwrap();
        assert_eq!(
            p.severity, "hard",
            "pressure on top of Western Union must not look safer than Western Union alone"
        );
    }

    #[test]
    fn urgency_plus_full_prepayment_stays_a_pattern_match() {
        let s = vec![
            sig("Urgency language", "Detected", "caution", ""),
            sig("Advance payment request", FULL_PREPAYMENT, "caution", ""),
        ];
        let f = derive_risk_factors(&s);
        assert_eq!(f.len(), 1);
        assert_eq!(
            find(&f, "advance_fee_scam_pattern").unwrap().severity,
            "compound"
        );
    }

    #[test]
    fn regulated_product_from_non_maker_is_a_combined_flag() {
        let f = derive_risk_factors(&[sig(
            "Regulated product",
            REGULATED_NOT_MAKER,
            "caution",
            "Botox. Not the maker.",
        )]);
        let c = find(&f, "regulated_product_from_non_maker").unwrap();
        assert_eq!(c.severity, "compound");
        assert!(c.description.starts_with("Botox. Not the maker."));
        assert!(
            find(&f, "regulated_product_flagged").is_none(),
            "not counted twice"
        );

        let f = derive_risk_factors(&[sig(
            "Regulated product",
            "Licence needed",
            "caution",
            "Botox.",
        )]);
        assert!(find(&f, "regulated_product_from_non_maker").is_none());
        assert_eq!(
            find(&f, "regulated_product_flagged").unwrap().severity,
            "soft"
        );
    }

    #[test]
    fn missing_age_is_not_new() {
        assert!(!is_new_account("Unknown"));
        assert!(!is_new_account("Not provided"));
        assert!(!is_new_account("Invalid date"));
        assert!(!is_new_account("15 years 8 months"));
        assert!(is_new_account("This month"));
        assert!(is_new_account("3 months"));
        assert!(is_new_account("Founded this year"));
        assert!(!is_new_account("About 1 year"));
        assert!(!is_new_account("About 11 years"));
    }

    #[test]
    fn western_union_alone_is_serious() {
        let s = vec![
            sig("Urgency language", "None found", "good", ""),
            sig(
                "Advance payment request",
                "Detected",
                "caution",
                "Accepts Western Union.",
            ),
        ];
        let f = derive_risk_factors(&s);
        let p = find(&f, "unsafe_payment_terms").expect("payment factor");
        assert_eq!(p.severity, "hard");
        assert_eq!(p.description, "Accepts Western Union.");
        assert!(find(&f, "advance_payment_request_flagged").is_none());
    }

    #[test]
    fn full_prepayment_alone_is_worth_noting_not_serious() {
        let s = vec![sig(
            "Advance payment request",
            FULL_PREPAYMENT,
            "caution",
            "100% before shipment.",
        )];
        let f = derive_risk_factors(&s);
        assert!(find(&f, "unsafe_payment_terms").is_none());
        assert_eq!(
            find(&f, "advance_payment_request_flagged")
                .unwrap()
                .severity,
            "soft"
        );
    }

    #[test]
    fn full_prepayment_to_a_new_company_is_a_pattern_match() {
        let s = vec![
            sig(
                "Advance payment request",
                FULL_PREPAYMENT,
                "caution",
                "40% advance, 60% before shipment.",
            ),
            sig("Account age", "Founded this year", "caution", ""),
        ];
        let f = derive_risk_factors(&s);
        assert_eq!(f.len(), 1, "one combined factor, not three");
        let c = find(&f, "full_prepayment_to_new_company").unwrap();
        assert_eq!(c.severity, "compound");
        assert!(
            c.description
                .starts_with("40% advance, 60% before shipment.")
        );
        assert!(c.description.contains("very new (Founded this year)"));
        // An older company with full prepayment stays only worth noting.
        let older = vec![
            sig("Advance payment request", FULL_PREPAYMENT, "caution", ""),
            sig("Account age", "About 5 years", "good", ""),
        ];
        assert!(
            find(
                &derive_risk_factors(&older),
                "full_prepayment_to_new_company"
            )
            .is_none()
        );
    }

    #[test]
    fn untraceable_payment_is_serious() {
        let s = vec![sig(
            "Advance payment request",
            "Untraceable payment",
            "caution",
            "Accepts MoneyGram.",
        )];
        let f = derive_risk_factors(&s);
        assert_eq!(find(&f, "unsafe_payment_terms").unwrap().severity, "hard");
    }

    #[test]
    fn urgency_plus_older_detected_payment_stays_one_compound_factor() {
        // Consumer scans (OLX) write "Detected" for a payment request.
        let s = vec![
            sig("Urgency language", "Detected", "caution", ""),
            sig("Advance payment request", "Detected", "caution", ""),
        ];
        let f = derive_risk_factors(&s);
        assert_eq!(
            find(&f, "advance_fee_scam_pattern").unwrap().severity,
            "compound"
        );
        assert!(find(&f, "unsafe_payment_terms").is_none());
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn unchecked_image_never_triggers_counterfeit() {
        let s = vec![
            sig("Duplicate listing", "Detected", "caution", "d"),
            sig("Image authenticity", "Not checked", "info", "off"),
        ];
        let f = derive_risk_factors(&s);
        assert!(find(&f, "likely_counterfeit_or_nonexistent_product").is_none());
        // the duplicate listing is still reported on its own
        assert!(find(&f, "duplicate_listing_flagged").is_some());
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn legitimacy_uses_claude_reason_and_names_young_account() {
        let s = vec![
            sig(
                "Overall legitimacy check",
                "Not confirmed",
                "caution",
                "Name does not match products.",
            ),
            sig("Account age", "This month", "caution", ""),
        ];
        let f = derive_risk_factors(&s);
        assert_eq!(f.len(), 1);
        let l = &f[0];
        assert_eq!(l.severity, "hard");
        assert!(l.description.starts_with("Name does not match products."));
        assert!(l.description.contains("very new (This month)"));
        assert_eq!(
            l.contributing_signals,
            vec!["Overall legitimacy check", "Account age"]
        );
    }

    #[test]
    fn info_signals_are_never_risk_factors() {
        let s = vec![
            sig("Platform verification", "Not offered", "info", ""),
            sig("Account age", "Not provided", "info", ""),
            sig("Image authenticity", "Not checked", "info", ""),
        ];
        assert!(derive_risk_factors(&s).is_empty());
    }

    #[test]
    fn one_scam_report_is_worth_noting_not_serious() {
        let s = vec![sig(
            "Safely history",
            "Reported once",
            "bad",
            "1 Safely user has reported this seller as a scam.",
        )];
        let f = derive_risk_factors(&s);
        assert!(find(&f, "network_confirmed_high_risk_seller").is_none());
        let soft = find(&f, "safely_history_flagged").expect("still worth noting");
        assert_eq!(soft.severity, "soft");
    }

    #[test]
    fn two_or_more_scam_reports_are_serious() {
        for value in ["Reported 2 times", "Reported 5 times"] {
            let s = vec![sig(
                "Safely history",
                value,
                "bad",
                "Safely users have reported this seller as a scam.",
            )];
            let f = derive_risk_factors(&s);
            let h = find(&f, "network_confirmed_high_risk_seller").expect(value);
            assert_eq!(h.severity, "hard");
            assert_eq!(
                h.description,
                "Safely users have reported this seller as a scam."
            );
            assert_eq!(f.len(), 1, "not also counted as a soft factor");
        }
    }

    #[test]
    fn older_saved_scans_with_several_checks_are_still_serious() {
        let signals = vec![sig(
            "Safely history",
            "4 prior checks. Average risk score: 72",
            "bad",
            "",
        )];
        let f = derive_risk_factors(&signals);
        assert_eq!(
            find(&f, "network_confirmed_high_risk_seller")
                .unwrap()
                .severity,
            "hard"
        );
    }

    #[test]
    fn reads_the_number_in_every_wording() {
        let count = |v: &str| history_count(&sig("Safely history", v, "info", ""));
        assert_eq!(count("No scam reports"), 0);
        assert_eq!(count("Reported once"), 1);
        assert_eq!(count("Reported 3 times"), 3);
        assert_eq!(count("Reported 12 times"), 12);
        assert_eq!(count("New to Safely"), 0);
        assert_eq!(count("Checked once before"), 1);
        assert_eq!(count("Checked 2 times before"), 2);
        assert_eq!(count("3 prior checks"), 3, "old wording still works");
    }

    #[test]
    fn no_scam_reports_is_never_a_risk_factor() {
        let s = vec![sig("Safely history", "No scam reports", "info", "")];
        assert!(derive_risk_factors(&s).is_empty());
    }

    #[test]
    fn vague_b2b_listing_plus_flagged_images_is_one_compound_factor() {
        let s = vec![
            sig("Listing detail", "Vague", "caution", "v"),
            sig("Image authenticity", "not verified", "info", "i"),
        ];
        let f = derive_risk_factors(&s);
        assert_eq!(f.len(), 1);
        let c = find(&f, "likely_counterfeit_or_nonexistent_product").unwrap();
        assert_eq!(
            c.contributing_signals,
            vec!["Listing detail", "Image authenticity"]
        );
    }

    #[test]
    fn vague_listing_text_never_says_duplicate() {
        let b2b = derive_risk_factors(&[
            sig("Listing detail", "Vague", "caution", "v"),
            sig("Image authenticity", "not verified", "info", "i"),
        ]);
        let c = find(&b2b, "likely_counterfeit_or_nonexistent_product").unwrap();
        assert!(c.description.starts_with("The listing is vague"));
        assert!(!c.description.contains("duplicate"));
        let b2c = derive_risk_factors(&[
            sig("Duplicate listing", "Detected", "caution", "d"),
            sig("Image authenticity", "not verified", "info", "i"),
        ]);
        let c = find(&b2c, "likely_counterfeit_or_nonexistent_product").unwrap();
        assert!(c.description.contains("duplicate-style"));
    }

    #[test]
    fn commodity_scam_pattern_is_serious() {
        let f = derive_risk_factors(&[sig(
            "Product range",
            COMMODITY_SCAM_PATTERN,
            "caution",
            "Offers ICUMSA 45 sugar and Urea 46.",
        )]);
        assert_eq!(f.len(), 1, "not also a soft factor");
        let c = find(&f, "commodity_scam_pattern").unwrap();
        assert_eq!(c.severity, "hard");
        assert_eq!(c.description, "Offers ICUMSA 45 sugar and Urea 46.");
        // One bait product alone is info and never a risk factor.
        let f = derive_risk_factors(&[sig("Product range", "Often used in scams", "info", "")]);
        assert!(f.is_empty());
    }

    #[test]
    fn unconfirmed_photos_alone_are_not_a_risk_factor() {
        let s = vec![
            sig("Listing detail", "Specific", "good", ""),
            sig("Image authenticity", "not verified", "info", "i"),
        ];
        assert!(derive_risk_factors(&s).is_empty());
    }

    #[test]
    fn vague_b2b_listing_alone_is_worth_noting() {
        let s = vec![sig("Listing detail", "Vague", "caution", "v")];
        let f = derive_risk_factors(&s);
        assert_eq!(f[0].severity, "soft");
        assert_eq!(f[0].name, "listing_detail_flagged");
    }

    #[test]
    fn founded_this_year_with_a_legitimacy_concern_is_named_together() {
        let s = vec![
            sig("Overall legitimacy check", "Not confirmed", "caution", "x"),
            sig("Account age", "Founded this year", "caution", ""),
        ];
        let f = derive_risk_factors(&s);
        assert_eq!(f.len(), 1);
        assert!(f[0].description.contains("very new (Founded this year)"));
    }
}
