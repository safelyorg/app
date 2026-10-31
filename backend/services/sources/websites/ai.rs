//! The AI part of the website check: what needs reading and judging
//! (address, payment methods, prices, pressure tactics, does the shop
//! look real). Safely gives it the shop's pages plus what it already
//! found itself (emails, phones, company numbers, policies), so the AI
//! never has to guess those.

use crate::{
    models::analysis::Signal,
    services::shared::claude::{Finding, PriceAssessment},
};
use serde::Deserialize;

/// Most characters of page text sent to the AI in total, and per page.
const MAX_TOTAL_CHARS: usize = 45_000;
const MAX_PAGE_CHARS: usize = 8_000;

#[derive(Debug, Deserialize)]
pub struct WebsiteClaudeAnalysis {
    pub business_details: Finding,
    pub contact_verifiability: Finding,
    pub payment_methods: PriceAssessment,
    pub pricing: PriceAssessment,
    pub urgency_language: Finding,
    pub business_legitimacy: Finding,
    /// The company behind the shop, as the site writes it ("" if none).
    #[serde(default)]
    pub company_name: String,
    /// The country of the company's address ("" if none).
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub overall_risk_notes: String,
}

/// What Safely found by itself, given to the AI as facts.
pub struct KnownFacts<'a> {
    pub domain: &'a str,
    pub emails: &'a [String],
    pub phones: &'a [String],
    pub company_numbers: &'a [String],
    pub policies: &'a [&'static str],
}

/// The page texts, each cut to MAX_PAGE_CHARS, all to MAX_TOTAL_CHARS.
fn pages_text(pages: &[super::WebsitePage]) -> String {
    let mut out = String::new();
    for page in pages {
        let text: String = page.text.chars().take(MAX_PAGE_CHARS).collect();
        let block = format!("\n--- PAGE: {} ({})\n{}\n", page.url, page.title, text);
        if out.chars().count() + block.chars().count() > MAX_TOTAL_CHARS {
            break;
        }
        out.push_str(&block);
    }
    out
}

fn list(items: &[String]) -> String {
    if items.is_empty() {
        "none found".to_string()
    } else {
        items.join(", ")
    }
}

pub fn website_prompt(pages: &[super::WebsitePage], facts: &KnownFacts) -> String {
    let policies = if facts.policies.is_empty() {
        "none found".to_string()
    } else {
        facts.policies.join(", ")
    };
    format!(
        r#"
You are checking an online shop's own website for a buyer who wants to know
whether it is safe to buy from it. Judge the SHOP (the domain), not one
product. Return ONLY a raw JSON object, no markdown, no code fences.

Today's date: {today}
Shop domain: {domain}
Found by Safely on the pages (facts, do not doubt them):
- emails: {emails}
- phone links: {phones}
- company numbers: {numbers}
- shop policy pages with real text: {policies}

RULES:
- Only contact details the site itself gives count: a phone, an email or a
  street address. A contact form alone is NOT a contact channel.
- A Gmail or Hotmail address is a note, not proof of a scam.
- NEVER say the company is not registered or does not exist: you cannot
  look up registers. Say only what is or is not shown.
- Write every "evidence" and "reasoning" in English, in plain short
  sentences the buyer can understand.

For business_details: found = true when the site shows a real street
address AND at least one of: a phone, or a company / tax number. Name what
is shown and what is missing in the evidence.

For contact_verifiability: found = true when at least one direct channel
(phone, email or street address) is given by the site.

For payment_methods: verdict exactly "normal", "risky only" or "not shown".
"risky only" when the ONLY ways to pay shown are bank transfer, Pix to a
person, crypto, gift cards, Western Union or MoneyGram (no card, PayPal or
other protected way). Name the methods in the reasoning.

For pricing: verdict exactly "normal", "suspiciously low" or "not shown".
"suspiciously low" when prices are far below what the products normally
cost (e.g. 70-90% off everything, new iPhones at a third of their price).
The verdict must match the reasoning.

For urgency_language: found = true for pressure tactics: countdown
timers, "only 2 left" on every product, "offer ends today", fake
"X people are buying now". Normal sale banners are not pressure.

For business_legitimacy: found = true if this looks like a real,
operating shop; false only for a concrete red flag: made-up, impossible or
self-contradicting details (e.g. "founded in 1990" on a site full of
template text, an address that is a different country than everything
else, copied policies naming another shop). Missing details alone are not
a red flag - list them in the evidence instead.

company_name: the company behind the shop as the site writes it (legal
name if shown), or "". country: the country of the company's address,
or "". phone: one phone number shown on the site, or "".

Return JSON in exactly this shape:
{{
"business_details": {{ "found": true, "evidence": "" }},
"contact_verifiability": {{ "found": true, "evidence": "" }},
"payment_methods": {{ "verdict": "normal", "reasoning": "" }},
"pricing": {{ "verdict": "normal", "reasoning": "" }},
"urgency_language": {{ "found": false, "evidence": "" }},
"business_legitimacy": {{ "found": true, "evidence": "" }},
"company_name": "",
"country": "",
"phone": "",
"overall_risk_notes": ""
}}

THE SHOP'S PAGES:
{pages}
"#,
        today = chrono::Utc::now().format("%Y-%m-%d"),
        domain = facts.domain,
        emails = list(facts.emails),
        phones = list(facts.phones),
        numbers = list(facts.company_numbers),
        policies = policies,
        pages = pages_text(pages),
    )
}

fn card(label: &str, value: &str, signal_type: &str, sub: &str, category: &str) -> Signal {
    Signal {
        label: label.to_string(),
        sub: sub.to_string(),
        value: value.to_string(),
        signal_type: signal_type.to_string(),
        category: category.to_string(),
        check_type: "existence".to_string(),
    }
}

/// The cards the AI answers (names as in standards/websites.json).
pub fn ai_cards(a: &WebsiteClaudeAnalysis) -> Vec<Signal> {
    let good_bad = |found: bool| if found { "good" } else { "caution" };
    let mut cards = vec![
        card(
            "Business details",
            if a.business_details.found {
                "Shown"
            } else {
                "Missing"
            },
            good_bad(a.business_details.found),
            &a.business_details.evidence,
            "company",
        ),
        card(
            "Contact info",
            if a.contact_verifiability.found {
                "Given"
            } else {
                "Not confirmed"
            },
            good_bad(a.contact_verifiability.found),
            &a.contact_verifiability.evidence,
            "identity",
        ),
        card(
            "Urgency language",
            if a.urgency_language.found {
                "Pressure tactics"
            } else {
                "None found"
            },
            good_bad(!a.urgency_language.found),
            &a.urgency_language.evidence,
            "communication",
        ),
        card(
            "Overall legitimacy check",
            if a.business_legitimacy.found {
                "Confirmed"
            } else {
                "Not confirmed"
            },
            good_bad(a.business_legitimacy.found),
            &a.business_legitimacy.evidence,
            "company",
        ),
    ];
    let payment = match a.payment_methods.verdict.as_str() {
        "risky only" => ("Risky methods only", "caution"),
        "not shown" => ("Not shown", "info"),
        _ => ("Normal", "good"),
    };
    cards.push(card(
        "Payment at checkout",
        payment.0,
        payment.1,
        &a.payment_methods.reasoning,
        "listing",
    ));
    let price = match a.pricing.verdict.as_str() {
        "suspiciously low" => ("suspiciously low", "caution"),
        "not shown" => ("Not shown", "info"),
        _ => ("normal", "good"),
    };
    cards.push(card(
        "Prices",
        price.0,
        price.1,
        &a.pricing.reasoning,
        "listing",
    ));
    cards
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::sources::websites::WebsitePage;

    #[test]
    fn the_prompt_carries_the_facts_and_the_pages() {
        let pages = vec![WebsitePage {
            url: "https://shop.com/".into(),
            title: "Shop".into(),
            text: "Welcome to Shop. 80% off everything!".into(),
            links: vec![],
        }];
        let p = website_prompt(
            &pages,
            &KnownFacts {
                domain: "shop.com",
                emails: &["sales@shop.com".to_string()],
                phones: &[],
                company_numbers: &[],
                policies: &["privacy policy"],
            },
        );
        assert!(p.contains("Shop domain: shop.com"));
        assert!(p.contains("- emails: sales@shop.com"));
        assert!(p.contains("- phone links: none found"));
        assert!(p.contains("- shop policy pages with real text: privacy policy"));
        assert!(p.contains("--- PAGE: https://shop.com/ (Shop)"));
        assert!(p.contains("A contact form alone is NOT a contact channel"));
    }

    #[test]
    fn long_pages_are_cut() {
        let page = WebsitePage {
            url: "u".into(),
            title: "t".into(),
            text: "a".repeat(MAX_PAGE_CHARS * 2),
            links: vec![],
        };
        let text = pages_text(&[
            page.clone(),
            page.clone(),
            page.clone(),
            page.clone(),
            page.clone(),
            page.clone(),
            page,
        ]);
        assert!(text.chars().count() <= MAX_TOTAL_CHARS);
        assert!(text.contains(&"a".repeat(MAX_PAGE_CHARS)));
        assert!(!text.contains(&"a".repeat(MAX_PAGE_CHARS + 1)));
    }

    #[test]
    fn the_answer_becomes_cards() {
        let a: WebsiteClaudeAnalysis = serde_json::from_str(
            r#"{
            "business_details": {"found": false, "evidence": "No address."},
            "contact_verifiability": {"found": true, "evidence": "Email given."},
            "payment_methods": {"verdict": "risky only", "reasoning": "Only Pix and crypto."},
            "pricing": {"verdict": "suspiciously low", "reasoning": "iPhone 16 at US$199."},
            "urgency_language": {"found": true, "evidence": "Countdown timer."},
            "business_legitimacy": {"found": true, "evidence": ""},
            "company_name": "Loja X LTDA", "country": "Brazil"
        }"#,
        )
        .unwrap();
        let cards = ai_cards(&a);
        let get = |l: &str| cards.iter().find(|c| c.label == l).unwrap();
        assert_eq!(get("Business details").signal_type, "caution");
        assert_eq!(get("Contact info").signal_type, "good");
        assert_eq!(get("Payment at checkout").value, "Risky methods only");
        assert_eq!(get("Prices").value, "suspiciously low");
        assert_eq!(get("Urgency language").signal_type, "caution");
        assert_eq!(get("Overall legitimacy check").value, "Confirmed");
        assert_eq!(a.phone, "");
    }
}
