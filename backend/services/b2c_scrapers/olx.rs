use crate::services::b2c_scrapers::{B2cProfileResult, B2cScraper};

pub struct OlxScraper;

impl B2cScraper for OlxScraper {
    fn matches_platform(&self, platform: &str) -> bool {
        platform == "olx"
    }

    fn parse(&self, html: &str, expected_seller_name: &str) -> B2cProfileResult {
        let document = scraper::Html::parse_document(html);
        let body_selector = scraper::Selector::parse("body").unwrap();
        let full_text: String = document
            .select(&body_selector)
            .next()
            .map(|body| body.text().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        let lower_text = full_text.to_lowercase();
        let lower_name = expected_seller_name.to_lowercase();
        let seller_name_confirmed =
            !lower_name.trim().is_empty() && lower_text.contains(&lower_name);
        let website = extract_self_referenced_website(&full_text);
        B2cProfileResult {
            website,
            seller_name_confirmed,
        }
    }
}

fn extract_self_referenced_website(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let self_reference_phrases = [
        "our website",
        "our shop",
        "our store",
        "visit us at",
        "contact us at",
    ];
    for phrase in self_reference_phrases {
        if let Some(pos) = lower.find(phrase) {
            let window_start = pos;
            let window_end = (pos + phrase.len() + 60).min(text.len());
            let window = &text[window_start..window_end];
            if let Some(url) = find_url_in_text(window) {
                if !url.contains("olx.com") {
                    return Some(url);
                }
            }
        }
    }
    None
}

fn find_url_in_text(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|word| {
            let cleaned = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.');
            cleaned.contains('.') && cleaned.len() > 4 && !cleaned.starts_with('.')
        })
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.')
                .to_string()
        })
}
