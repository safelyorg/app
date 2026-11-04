//! The AI part of the website check: what needs reading and judging
//! (address, payment methods, prices, pressure tactics, does the shop
//! look real). Safely gives it the shop's pages plus what it already
//! found itself (emails, phones, company numbers, policies), so the AI
//! never has to guess those.

use crate::{
    models::analysis::Signal,
    services::shared::claude::{Finding, IMAGE_ANALYSIS_ENABLED, ImageAssessment, PriceAssessment},
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
    /// Does what the shop states about itself (company name, founding
    /// date, claimed size/scale, country) hang together, the same
    /// inconsistency check a marketplace scan runs on a supplier's
    /// profile.
    pub registration_consistency: Finding,
    /// Whether product photos found on the shop's own pages look
    /// genuine, the same check a marketplace scan runs on listing
    /// photos. "not verified" when no photos were found, or when
    /// IMAGE_ANALYSIS_ENABLED (services/shared/claude.rs) is off.
    pub image_authenticity: ImageAssessment,
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

/// The "For image_authenticity: ..." instruction, matching the wording
/// of the same check on a marketplace scan (services/shared/claude.rs)
/// - "original" is good, "not verified" is a note, never a red flag on
/// its own.
fn image_context(image_urls: &[String], photos_blocked: bool) -> String {
    if photos_blocked {
        format!(
            "{} product photo(s) were found on the shop's pages, but they could not be loaded, so you cannot view them - use \"not verified\" as the verdict and say in the reasoning that the photos could not be checked because they could not be loaded.",
            image_urls.len().min(3)
        )
    } else if IMAGE_ANALYSIS_ENABLED && !image_urls.is_empty() {
        format!(
            "{} product photo(s) found on the shop's own pages are attached. Use \"original\" only if they look like genuine photos of this shop's own products; use \"not verified\" if they look like stock, catalogue or reused images, or if you cannot tell.",
            image_urls.len().min(3)
        )
    } else if image_urls.is_empty() {
        "no product photos were found on the pages read, so use \"not verified\" as the verdict, since authenticity cannot genuinely be assessed without any real images.".to_string()
    } else {
        format!(
            "{} real product photo(s) were found on the shop's pages, but image sending is disabled for cost reasons, so you cannot directly view them - use \"not verified\" as the honest verdict, since authenticity cannot genuinely be assessed without actually viewing the real images.",
            image_urls.len()
        )
    }
}

pub fn website_prompt(
    pages: &[super::WebsitePage],
    facts: &KnownFacts,
    image_urls: &[String],
) -> String {
    website_prompt_with(pages, facts, image_urls, false)
}

/// The same prompt for the second try, when the shop's pages have
/// photos but Claude was not allowed to load them (mirrors
/// services/shared/claude.rs's b2b_content_photos_blocked).
pub fn website_prompt_photos_blocked(
    pages: &[super::WebsitePage],
    facts: &KnownFacts,
    image_urls: &[String],
) -> String {
    website_prompt_with(pages, facts, image_urls, true)
}

fn website_prompt_with(
    pages: &[super::WebsitePage],
    facts: &KnownFacts,
    image_urls: &[String],
    photos_blocked: bool,
) -> String {
    let policies = if facts.policies.is_empty() {
        "none found".to_string()
    } else {
        facts.policies.join(", ")
    };
    let image_context = image_context(image_urls, photos_blocked);
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

For registration_consistency: found = true if what the site states about
itself (company name, founding date, claimed size or scale, country)
hangs together coherently; false ONLY if you can name a real, concrete
inconsistency - e.g. a founding date contradicted elsewhere on the site, a
claimed scale ("the world's largest supplier", "serving millions of
customers", "since 1990") for a shop that elsewhere says it was only
founded this year or last year, or a registered address in a different
country than everything else on the site. Missing details alone are not
an inconsistency - do not set this to false just because a founding date
or company size is not given.

For image_authenticity: {image_context}
Verdict must be exactly "original" or "not verified" - no other words.

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
"registration_consistency": {{ "found": true, "evidence": "" }},
"image_authenticity": {{ "verdict": "not verified", "reasoning": "" }},
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
        image_context = image_context,
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

/// "Image authenticity" card. While IMAGE_ANALYSIS_ENABLED
/// (services/shared/claude.rs) is off, Claude never sees the photos, so
/// "not verified" says nothing about the shop - the card then reads
/// "Not checked" and does not count against it. With image checking
/// on, "original" is good; "not verified" (no photos found, or Claude
/// could not confirm they are the shop's own) is only informational,
/// the same as the marketplace card.
fn image_authenticity_card(a: &ImageAssessment) -> Signal {
    if !IMAGE_ANALYSIS_ENABLED {
        return card(
            "Image authenticity",
            "Not checked",
            "info",
            "Image checking is switched off in Safely for now, so this shop's photos were not reviewed. This does not count against the shop.",
            "website",
        );
    }
    card(
        "Image authenticity",
        &a.verdict,
        if a.verdict == "original" {
            "good"
        } else {
            "info"
        },
        &a.reasoning,
        "website",
    )
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
        card(
            "Registration consistency",
            if a.registration_consistency.found {
                "Confirmed"
            } else {
                "Not confirmed"
            },
            good_bad(a.registration_consistency.found),
            &a.registration_consistency.evidence,
            "company",
        ),
        image_authenticity_card(&a.image_authenticity),
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
            images: vec![],
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
            &[],
        );
        assert!(p.contains("Shop domain: shop.com"));
        assert!(p.contains("- emails: sales@shop.com"));
        assert!(p.contains("- phone links: none found"));
        assert!(p.contains("- shop policy pages with real text: privacy policy"));
        assert!(p.contains("--- PAGE: https://shop.com/ (Shop)"));
        assert!(p.contains("A contact form alone is NOT a contact channel"));
        assert!(p.contains("no product photos were found on the pages read"));
    }

    #[test]
    fn the_image_context_reflects_what_was_found() {
        let pages = vec![WebsitePage {
            url: "https://shop.com/".into(),
            title: "Shop".into(),
            text: "Welcome".into(),
            links: vec![],
            images: vec![],
        }];
        let facts = KnownFacts {
            domain: "shop.com",
            emails: &[],
            phones: &[],
            company_numbers: &[],
            policies: &[],
        };
        let urls = vec!["https://shop.com/a.jpg".to_string()];
        let with_photos = website_prompt(&pages, &facts, &urls);
        assert!(with_photos.contains("1 product photo(s) found on the shop's own pages"));
        let blocked = website_prompt_photos_blocked(&pages, &facts, &urls);
        assert!(blocked.contains("could not be loaded, so you cannot view them"));
    }

    #[test]
    fn long_pages_are_cut() {
        let page = WebsitePage {
            url: "u".into(),
            title: "t".into(),
            text: "a".repeat(MAX_PAGE_CHARS * 2),
            links: vec![],
            images: vec![],
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
            "registration_consistency": {"found": false, "evidence": "Says founded this year but claims to serve millions of customers since 1990."},
            "image_authenticity": {"verdict": "not verified", "reasoning": "No product photos were found."},
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
        assert_eq!(get("Registration consistency").value, "Not confirmed");
        assert_eq!(get("Registration consistency").signal_type, "caution");
        assert_eq!(get("Image authenticity").value, "not verified");
        assert_eq!(get("Image authenticity").signal_type, "info");
        assert_eq!(a.phone, "");
    }

    #[test]
    fn image_authenticity_card_reads_originals_as_good() {
        // IMAGE_ANALYSIS_ENABLED is currently true; while it is, an
        // "original" verdict is a good card, not a caution or a note.
        if IMAGE_ANALYSIS_ENABLED {
            let a = ImageAssessment {
                verdict: "original".to_string(),
                reasoning: "Looks genuine.".to_string(),
            };
            let c = image_authenticity_card(&a);
            assert_eq!(c.value, "original");
            assert_eq!(c.signal_type, "good");
        }
    }
}
