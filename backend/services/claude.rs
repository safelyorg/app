use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ClaudeAnalysis {
    pub urgency_language: Finding,
    pub advance_payment_request: Finding,
    pub account_age: Finding,
    pub duplicate_listing: Finding,
    pub image_authenticity: ImageAssessment,
    pub fraud_pattern_match: Finding,
    pub contact_info_in_listing: Finding,
    pub price_assessment: PriceAssessment,
    pub overall_risk_notes: String,
}

#[derive(Debug, Deserialize)]
pub struct Finding {
    pub found: bool,
    pub evidence: String,
}

#[derive(Debug, Deserialize)]
pub struct PriceAssessment {
    pub verdict: String,
    pub reasoning: String,
}

#[derive(Debug, Deserialize)]
pub struct ImageAssessment {
    pub verdict: String,
    pub reasoning: String,
}

// --------------

#[derive(Serialize)]
pub struct ClaudeRequest {
    pub model: String,
    pub max_tokens: u32,
    pub messages: Vec<Message>,
}

#[derive(Serialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

pub fn content(
    platform: &str,
    seller_name: &str,
    seller_account_age: &str,
    seller_total_deals: i32,
    seller_disputes: i32,
    title: &str,
    price: i64,
    description: &str,
) -> String {
    let claude_content = format!(
        r#"
        You are a fraud detection assistant for an online marketplace.

        Analyze this listing and seller, then return ONLY a valid JSON object. Do not include markdown, explanations, or code fences.

        Platform: {platform}
        Seller name: {seller_name}
        Seller account age: {seller_account_age}
        Seller total deals: {seller_total_deals}
        Seller disputes: {seller_disputes}
        Listing title: {title}
        Listing price: PKR {price}
        Listing description: {description}

        Return JSON in exactly this shape:

        {{
        "urgency_language": {{
            "found": false,
            "evidence": ""
        }},
        "advance_payment_request": {{
            "found": false,
            "evidence": ""
        }},
        "contact_info_in_listing": {{
            "found": false,
            "evidence": ""
        }},
        "price_assessment": {{
            "verdict": "normal",
            "reasoning": ""
        }},
        "fraud_pattern_match": {{
            "found": false,
            "evidence": ""
        }},
        "duplicate_listing": {{
            "found": false,
            "evidence": ""
        }},
        "image_authenticity": {{
            "verdict": "original",
            "reasoning": ""
        }},
        "overall_risk_notes": ""
        }}
        "#,
    );

    claude_content
}

pub async fn call_claude(
    platform: &str,
    seller_name: &str,
    seller_account_age: &str,
    seller_total_deals: i32,
    seller_disputes: i32,
    title: &str,
    price: i64,
    description: &str,
) -> Result<(), reqwest::Error> {
    let client = Client::new();
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_URL needs to be present in the .env file");
    dotenvy::dotenv().expect("The .env file should be accessed");

    let prompt = content(
        platform,
        seller_name,
        seller_account_age,
        seller_total_deals,
        seller_disputes,
        title,
        price,
        description,
    );

    let payload = ClaudeRequest {
        model: String::from("claude-sonnet-4-6"),
        max_tokens: 1024,
        messages: vec![Message {
            role: "user".to_string(),
            content: prompt,
        }],
    };

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&payload)
        .send()
        .await?;

    println!("response: {:?}", response.status());
    Ok(())
}
