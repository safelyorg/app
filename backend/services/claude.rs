use reqwest::Client;
use serde::{Deserialize, Serialize};

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

// ----------

// 2. Gives you the text(that contains the actual fraud analysis) from the content block
#[derive(Debug, Deserialize)]
struct ClaudeEnvelope {
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    text: String,
}
// ----------

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
// ----------

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
        Analyze this listing and seller, then return ONLY a raw JSON object with no markdown, no code fences, no backticks, no explanation. Start your response with {{ and end with }}.

        Platform: {platform}
        Seller name: {seller_name}
        Seller account age: {seller_account_age}
        Seller total deals: {seller_total_deals}
        Seller disputes: {seller_disputes}
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
    _image_urls: &[String],
) -> Result<ClaudeAnalysis, reqwest::Error> {
    let client = Client::new();
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY needs to be present in the .env file");
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
        .await?;

    let body_text = response.text().await?;

    let envelope: ClaudeEnvelope =
        serde_json::from_str(&body_text).expect("failed to parse Claude's response envelope");

    let inner_json = &envelope.content[0].text;
    let cleaned = inner_json
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let analysis: ClaudeAnalysis =
        serde_json::from_str(cleaned).expect("failed to parse Claude's analysis JSON");

    Ok(analysis)
}
