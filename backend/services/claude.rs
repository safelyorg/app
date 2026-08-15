use crate::errors::claude::ClaudeError;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::from_str;

// 1. Packaging the question for Claude's API
#[derive(Serialize)]
pub struct ClaudeRequest {
    pub model: String,
    pub max_tokens: u32,
    pub messages: Vec<Message>,
}

#[derive(Serialize)]
pub struct Message {
    pub role: String,
    pub content: Vec<ContentItem>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentItem {
    Text { text: String },
    Image { source: ImageSource },
}

#[derive(Serialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub url: String,
}

// 2. Gives you the text(that contains the actual fraud analysis) from the content block
#[derive(Debug, Deserialize)]
struct ClaudeEnvelope {
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    text: String,
}

// 3. The raw data of analysis is inserted in this struct for better structure.
#[derive(Debug, Deserialize)]
pub struct ClaudeAnalysis {
    pub urgency_language: Finding,
    pub advance_payment_request: Finding,
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

#[derive(Debug)]
pub struct ContentArguments<'a> {
    pub platform: &'a str,
    pub seller_name: &'a str,
    pub seller_account_age: &'a str,
    pub title: &'a str,
    pub price: i64,
    pub description: &'a str,
}

pub fn content(arg: ContentArguments) -> String {
    format!(
        r#"
        You are a fraud detection assistant for an online marketplace.
        Analyze this listing and seller, then return ONLY a raw JSON object with no markdown, no code fences, no backticks, no explanation. Start your response with {{ and end with }}.

        Platform: {platform}
        Seller name: {seller_name}
        Seller account age: {seller_account_age}
        Listing title: {title}
        Listing price: PKR {price}
        Listing description: {description}

        For the duplicate_listing field: check if the description appears generic, templated, or copy-pasted. Look for mismatched details between title and description, no item-specific information like serial numbers or condition details, language that could apply to any listing of this type rather than this specific item. Set found to true if the listing appears to be a template or copy rather than an original genuine listing.

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
        platform = arg.platform,
        seller_name = arg.seller_name,
        seller_account_age = arg.seller_account_age,
        title = arg.title,
        price = arg.price,
        description = arg.description,
    )
}
pub async fn call_claude(
    platform: &str,
    seller_name: &str,
    seller_account_age: &str,
    title: &str,
    price: i64,
    description: &str,
    _image_urls: &[String],
) -> Result<ClaudeAnalysis, ClaudeError> {
    let client = Client::new();
    let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| ClaudeError::MissingApiKey)?;
    let args = ContentArguments {
        platform,
        seller_name,
        seller_account_age,
        title,
        price,
        description,
    };
    let prompt = content(args);
    // Commenting to prevent image detection because it takes a lot of tokens
    let mut content_blocks: Vec<ContentItem> = vec![ContentItem::Text { text: prompt }];
    // for url in image_urls.iter().take(3) {
    //     content_blocks.push(ContentItem::Image {
    //         source: ImageSource {
    //             source_type: "url".to_string(),
    //             url: url.clone(),
    //         },
    //     });
    // }

    let payload = ClaudeRequest {
        model: String::from("claude-sonnet-4-6"),
        max_tokens: 2048,
        messages: vec![Message {
            role: "user".to_string(),
            content: content_blocks,
        }],
    };

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&payload)
        .send()
        .await
        .map_err(|e| ClaudeError::RequestFailed(e.to_string()))?;

    let body_text = response
        .text()
        .await
        .map_err(|e| ClaudeError::RequestFailed(e.to_string()))?;

    let envelope: ClaudeEnvelope =
        serde_json::from_str(&body_text).map_err(|e| ClaudeError::ParseFailed(e.to_string()))?;

    let inner_json = &envelope.content[0].text;
    let cleaned = inner_json
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    from_str(cleaned).map_err(|e| ClaudeError::ParseFailed(e.to_string()))
}
