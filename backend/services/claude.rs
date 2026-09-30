use crate::{errors::claude::ClaudeError, services::translation::language_instruction};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::from_str;
use std::env::var;

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
    pub extracted_phone_number: Option<String>,
    pub overall_risk_notes: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Finding {
    pub found: bool,
    pub evidence: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ImageAssessment {
    pub verdict: String,
    pub reasoning: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PriceAssessment {
    pub verdict: String,
    pub reasoning: String,
}

#[derive(Debug)]
pub struct CallClaudeArguments<'a> {
    pub platform: &'a str,
    pub seller_name: &'a str,
    pub seller_account_age: &'a str,
    pub title: &'a str,
    pub price: i64,
    pub description: &'a str,
    pub image_urls: &'a [String],
    pub language: &'a str,
}

// B2B analysis structs
#[derive(Debug, Deserialize)]
pub struct B2bClaudeAnalysis {
    pub business_legitimacy: Finding,
    pub registration_consistency: Finding,
    pub listing_specificity: Finding,
    pub pricing_transparency: PriceAssessment,
    pub contact_verifiability: Finding,
    pub urgency_language: Finding,
    pub advance_payment_request: Finding,
    pub image_authenticity: ImageAssessment,
    pub overall_risk_notes: String,
}

pub struct CallB2bClaudeArguments<'a> {
    pub platform: &'a str,
    pub company_name: &'a str,
    pub year_established: &'a str,
    pub platform_verified: bool,
    pub employee_count: &'a str,
    pub company_description: &'a str,
    pub product_title: &'a str,
    pub product_description: &'a str,
    pub image_urls: &'a [String],
    pub language: &'a str,
    pub contact_name: &'a str,
    pub contact_phone: &'a str,
    pub website_url: &'a str,
    /// Scraped price text (e.g. "US$250 (5-99 cartons) | ..."), "" if none.
    pub unit_price: &'a str,
    /// Scraped MOQ text (e.g. "5 cartons"), "" if none.
    pub minimum_order_quantity: &'a str,
    /// Accepted payment methods (e.g. "Bank wire (T/T), Western Union (WU)"), "" if none.
    pub payment_type: &'a str,
}

/// Platforms that hide a supplier's direct contact details from
/// Safely's (anonymous) visitor - buyers are meant to contact suppliers
/// through the platform itself. For these, a missing phone/website says
/// nothing about the supplier. Returns the sentence Claude is given,
/// describing exactly what that platform hides.
fn platform_contact_policy(platform: &str) -> Option<&'static str> {
    match platform {
        "alibaba" => Some(
            "never publishes supplier phone numbers or websites; buyers contact suppliers through the platform's messaging",
        ),
        "b2brazil" => Some(
            "masks supplier contact names and phone numbers (e.g. \"Smith ********\") and does not show supplier websites to non-paying visitors; buyers contact suppliers through the platform. A contact name shown here may be only the part the platform leaves visible",
        ),
        "exporthub" => Some(
            "does not show supplier websites, and hides phone numbers from non-paying visitors (a phone listed above was still published by the supplier and is usable); buyers contact suppliers through the platform's inquiry form. ExportHub also does not verify companies, so a missing verified badge is normal there",
        ),
        _ => None,
    }
}

fn or_not_provided<'a>(value: &'a str) -> &'a str {
    if value.trim().is_empty() {
        "Not provided"
    } else {
        value
    }
}

/// Master switch for sending listing photos to Claude (off to save
/// cost). The Image authenticity card reads this too: while it is off,
/// the card shows "Not checked" instead of a caution. Set to true to
/// turn image checking back on - nothing else needs changing.
pub const IMAGE_ANALYSIS_ENABLED: bool = false;

/// The ONE, shared place that builds the real content blocks sent to
/// Claude - genuinely unified for both B2C and B2B, so a future
/// decision to re-enable image analysis only ever needs to happen in
/// one spot, not two separate, duplicated copies.
fn build_content_blocks(prompt: String, image_urls: &[String]) -> Vec<ContentItem> {
    let mut content_blocks: Vec<ContentItem> = vec![ContentItem::Text { text: prompt }];

    // Images are only sent when IMAGE_ANALYSIS_ENABLED is true (off
    // for cost reasons), for both B2C and B2B.
    if IMAGE_ANALYSIS_ENABLED {
        for url in image_urls.iter().take(3) {
            content_blocks.push(ContentItem::Image {
                source: ImageSource {
                    source_type: "url".to_string(),
                    url: url.clone(),
                },
            });
        }
    }

    content_blocks
}

pub async fn call_b2c_claude(args: CallClaudeArguments<'_>) -> Result<ClaudeAnalysis, ClaudeError> {
    let client = Client::new();
    let api_key = var("ANTHROPIC_API_KEY").map_err(|_| ClaudeError::MissingApiKey)?;
    let prompt = b2c_content(&args);
    let content_blocks = build_content_blocks(prompt, args.image_urls);

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

    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|e| ClaudeError::RequestFailed(e.to_string()))?;

    if !status.is_success() {
        eprintln!(
            "Safely: Claude API real, non-success status {} - body: {}",
            status,
            &body_text[..body_text.len().min(300)]
        );
        return Err(match status.as_u16() {
            401 | 403 => ClaudeError::Unauthorized,
            429 | 402 => ClaudeError::QuotaExceeded,
            code => ClaudeError::ServiceUnavailable(code),
        });
    }

    let envelope: ClaudeEnvelope =
        from_str(&body_text).map_err(|e| ClaudeError::ParseFailed(e.to_string()))?;

    let inner_json = &envelope.content[0].text;
    let cleaned = inner_json
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    from_str(cleaned).map_err(|e| ClaudeError::ParseFailed(e.to_string()))
}

pub async fn call_b2b_claude(
    args: CallB2bClaudeArguments<'_>,
) -> Result<B2bClaudeAnalysis, ClaudeError> {
    let client = Client::new();
    let api_key = var("ANTHROPIC_API_KEY").map_err(|_| ClaudeError::MissingApiKey)?;
    let prompt = b2b_content(&args);
    let content_blocks = build_content_blocks(prompt, args.image_urls);

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

    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|e| ClaudeError::RequestFailed(e.to_string()))?;

    if !status.is_success() {
        eprintln!(
            "Safely: Claude API real, non-success status {} - body: {}",
            status,
            &body_text[..body_text.len().min(300)]
        );
        return Err(match status.as_u16() {
            401 | 403 => ClaudeError::Unauthorized,
            429 | 402 => ClaudeError::QuotaExceeded,
            code => ClaudeError::ServiceUnavailable(code),
        });
    }

    let envelope: ClaudeEnvelope =
        from_str(&body_text).map_err(|e| ClaudeError::ParseFailed(e.to_string()))?;

    let inner_json = &envelope.content[0].text;
    let cleaned = inner_json
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    from_str(cleaned).map_err(|e| ClaudeError::ParseFailed(e.to_string()))
}

pub fn b2c_content(arg: &CallClaudeArguments) -> String {
    format!(
        r#"
        You are a fraud detection assistant for an online marketplace.
        Analyze this listing and seller, then return ONLY a raw JSON object with no markdown, no code fences, no backticks, no explanation. Start your response with {{ and end with }}.

        IMPORTANT: Write every text value in the JSON below (all
        "evidence" and "reasoning" fields, and "overall_risk_notes") in
        {language}. Keep every JSON key name and every "verdict" value
        exactly as specified in English - only the free-text explanations
        should be in {language}.

        Platform: {platform}
        Seller name: {seller_name}
        Seller account age: {seller_account_age}
        Listing title: {title}
        Listing price: PKR {price}
        Listing description: {description}

        For the duplicate_listing field: check if the description appears generic, templated, or copy-pasted. Look for mismatched details between title and description, no item-specific information like serial numbers or condition details, language that could apply to any listing of this type rather than this specific item. Set found to true if the listing appears to be a template or copy rather than an original genuine listing.
        For image_authenticity: verdict must be exactly "original" or
        "not verified" - no other words. Use "not verified" whenever no
        images were provided or authenticity cannot genuinely be assessed.
        For extracted_phone_number: sellers often write their real phone
        number in the description using odd separators to dodge
        automated scraping - things like "0+3+4+2+..." or "0:3:4:2..."
        or "0/3/4/2..." Look for any sequence of digits, however
        separated, that forms a plausible Pakistani phone number
        (typically starting with 0, 10-11 digits total). If found,
        return it as a single, clean digit string with all
        placeholder characters removed (e.g. "03001234567"). If no
        phone number is genuinely present in the text, return null.
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
        "extracted_phone_number": null,
        "overall_risk_notes": ""
        }}
        "#,
        platform = arg.platform,
        seller_name = arg.seller_name,
        seller_account_age = arg.seller_account_age,
        title = arg.title,
        price = arg.price,
        description = arg.description,
        language = language_instruction(arg.language),
    )
}

pub fn b2b_content(arg: &CallB2bClaudeArguments) -> String {
    let image_context = if IMAGE_ANALYSIS_ENABLED && !arg.image_urls.is_empty() {
        format!(
            "{} product image(s) from this listing are attached. Use \"original\" only if they look like genuine photos of this supplier's own product; use \"not verified\" if they look like stock, catalogue or reused images, or if you cannot tell.",
            arg.image_urls.len().min(3)
        )
    } else if arg.image_urls.is_empty() {
        "no actual product images were provided or found for this listing - use \"not verified\" as the verdict, since authenticity cannot genuinely be assessed without any real images.".to_string()
    } else {
        format!(
            "{} real product image(s) were found on this listing, but image sending is disabled for cost reasons, so you cannot directly view them - use \"not verified\" as the honest verdict, since authenticity cannot genuinely be assessed without actually viewing the real images.",
            arg.image_urls.len()
        )
    };

    let contact_policy = platform_contact_policy(arg.platform);
    let hides_contact = contact_policy.is_some();
    let hidden_note = "Not published - this platform never shows it to buyers";
    let contact_phone = if arg.contact_phone.trim().is_empty() && hides_contact {
        hidden_note
    } else {
        or_not_provided(arg.contact_phone)
    };
    let website_url = if arg.website_url.trim().is_empty() && hides_contact {
        hidden_note
    } else {
        or_not_provided(arg.website_url)
    };
    let platform_contact_rule = match contact_policy {
        Some(policy) => format!(
            "On {} the platform itself {}. A missing phone or website here is normal and must NOT count against the supplier. Judge contact_verifiability on what the platform does show (e.g. a named contact person).",
            arg.platform, policy
        ),
        None => String::new(),
    };

    format!(
        r#"
        You are a B2B supplier due-diligence assistant helping a procurement
        team evaluate a potential vendor. This is NOT a consumer marketplace -
        do not apply consumer fraud patterns like "urgency language" or
        "advance payment scams." B2B listings routinely omit pricing, MOQ,
        and shipping terms (these are typically negotiated privately after
        an inquiry) - this is completely normal and must NOT be treated as
        suspicious on its own.

        IMPORTANT: Write every text value in the JSON below (all
        "evidence" and "reasoning" fields, and "overall_risk_notes") in
        {language}. Keep every JSON key name and every "verdict" value
        exactly as specified in English - only the free-text explanations
        should be in {language}.

        Analyze this supplier and product listing, then return ONLY a raw
        JSON object with no markdown, no code fences, no backticks, no
        explanation. Start your response with {{ and end with }}.

        Platform: {platform}
        Company name: {company_name}
        Year established: {year_established}
        Platform-verified badge: {platform_verified}
        Employee count: {employee_count}
        Company description: {company_description}
        Contact name: {contact_name}
        Contact phone: {contact_phone}
        Website: {website_url}
        Product title: {product_title}
        Product description: {product_description}
        Unit price: {unit_price}
        Minimum order quantity: {moq}
        Accepted payment methods: {payment_type}

        {platform_contact_rule}

        MEANING OF "found" - read carefully, it differs by field:
        - For business_legitimacy, registration_consistency,
          listing_specificity and contact_verifiability, "found": true
          means the GOOD thing was found (the business looks genuine / the
          details are consistent / the listing is specific / contact is
          verifiable). "found": false means a real concern exists.
        - For urgency_language and advance_payment_request, "found": true
          means the BAD thing was found (pressure tactics / an unusual
          upfront payment demand). "found": false means none was found.
        The example values in the JSON shape at the end show the format
        only - they are not the answer.

        For business_legitimacy: set found to true if this looks like a
        genuine, established business with real operational details; false
        if it shows signs of being a shell, front, or fabricated entity
        (e.g. no real company details, generic or nonsensical company name,
        inconsistent information).

        For registration_consistency: set found to true if the company's
        stated information (name, founding year, scale) hangs together
        coherently; false ONLY if you can name a real, concrete
        inconsistency. Missing fields alone are not an inconsistency.

        For listing_specificity: set found to true if the listing describes
        a real, specific product with genuine, plausible details (concrete
        attributes, specs, materials, origin); false if it is template-like,
        vague, or nonsensical for the stated industry. Keyword-heavy titles
        are standard practice on B2B platforms for search visibility and
        are NOT on their own a sign of a template listing - judge the
        attributes and description instead.

        For pricing_transparency: assess ONLY whether the unit price and
        MOQ above (if provided) seem plausible for this product type.
        Missing pricing/MOQ/Incoterms is NORMAL in B2B and should verdict
        as "normal" unless something provided is actually implausible. If
        a price is provided, say in the reasoning what it is and whether it
        is plausible - do not claim pricing is missing when it is provided.

        For contact_verifiability: judge ONLY the contact details
        themselves - can a buyer actually reach and check this company?
        Set found to true if a named contact person and at least one
        direct channel (phone, email or website) are given, or if the
        details shown are all this platform publishes. Set found to false
        only if contact details are missing in a way that is unusual for
        this platform, look fake, or contradict the company (e.g. a phone
        country code that does not match the company's country). Do NOT
        use the verified badge, founding year, employee count or company
        size here - those are judged by other checks, and counting them
        again here would double-count them.

        For urgency_language: set found to true only if the listing or
        company description uses artificial pressure tactics inconsistent
        with normal B2B relationship-building - e.g. "deal expires today,"
        "must decide now," discouraging normal due diligence or sample
        requests. Reasonable business urgency (limited stock, seasonal
        demand) is normal and should NOT be flagged.

        For advance_payment_request: set found to true only if the listing
        demands full, 100% upfront payment before any samples,
        verification, or standard partial-deposit terms - genuinely unusual
        for legitimate B2B trade, where partial deposits and
        post-inspection payment terms are standard. ALSO set found to true
        if the accepted payment methods above include Western Union,
        MoneyGram, cryptocurrency or gift cards: these are cash-style
        transfers that cannot be reversed or traced to a company, and a
        genuine B2B supplier does not ask for them. Name the method in the
        evidence. Bank wire (T/T), L/C, D/A, D/P and platform escrow are
        normal and must not be flagged on their own.

        For image_authenticity: {image_context}
        Verdict must be exactly "original" or "not verified" - no other
        words.

        Return JSON in exactly this shape:
        {{
        "business_legitimacy": {{ "found": true, "evidence": "" }},
        "registration_consistency": {{ "found": true, "evidence": "" }},
        "listing_specificity": {{ "found": true, "evidence": "" }},
        "pricing_transparency": {{ "verdict": "normal", "reasoning": "" }},
        "contact_verifiability": {{ "found": true, "evidence": "" }},
        "urgency_language": {{ "found": false, "evidence": "" }},
        "advance_payment_request": {{ "found": false, "evidence": "" }},
        "image_authenticity": {{ "verdict": "not verified", "reasoning": "" }},
        "overall_risk_notes": ""
        }}
        "#,
        platform = arg.platform,
        company_name = arg.company_name,
        year_established = or_not_provided(arg.year_established),
        platform_verified = arg.platform_verified,
        employee_count = or_not_provided(arg.employee_count),
        contact_name = or_not_provided(arg.contact_name),
        contact_phone = contact_phone,
        website_url = website_url,
        company_description = or_not_provided(arg.company_description),
        product_title = arg.product_title,
        product_description = or_not_provided(arg.product_description),
        unit_price = or_not_provided(arg.unit_price),
        moq = or_not_provided(arg.minimum_order_quantity),
        payment_type = or_not_provided(arg.payment_type),
        platform_contact_rule = platform_contact_rule,
        image_context = image_context,
        language = language_instruction(arg.language),
    )
}

#[cfg(test)]
mod b2b_prompt_tests {
    use super::*;

    fn args<'a>(platform: &'a str, phone: &'a str, price: &'a str) -> CallB2bClaudeArguments<'a> {
        CallB2bClaudeArguments {
            platform,
            company_name: "Ningbo Youhuan Automation Technology Co., Ltd.",
            year_established: "2019",
            platform_verified: true,
            employee_count: "11-50",
            company_description: "",
            product_title: "Electric Wheelchair",
            product_description: "Material: steel",
            image_urls: &[],
            language: "English",
            contact_name: "Mr. Xu",
            contact_phone: phone,
            website_url: "",
            unit_price: price,
            minimum_order_quantity: "5 cartons",
            payment_type: "Bank wire (T/T), Western Union (WU)",
        }
    }

    #[test]
    fn price_and_moq_reach_the_prompt() {
        let p = b2b_content(&args("alibaba", "", "US$250 (5-99 cartons)"));
        assert!(p.contains("Unit price: US$250 (5-99 cartons)"));
        assert!(p.contains("Minimum order quantity: 5 cartons"));
    }

    #[test]
    fn missing_price_says_not_provided() {
        let p = b2b_content(&args("alibaba", "", ""));
        assert!(p.contains("Unit price: Not provided"));
    }

    #[test]
    fn alibaba_missing_contact_is_marked_as_platform_policy() {
        let p = b2b_content(&args("alibaba", "", ""));
        assert!(p.contains("Contact phone: Not published - this platform never shows it"));
        assert!(p.contains("Website: Not published - this platform never shows it"));
        assert!(p.contains("must NOT count against the supplier"));
    }

    #[test]
    fn other_platforms_keep_plain_not_provided() {
        let p = b2b_content(&args("kompass", "", ""));
        assert!(p.contains("Contact phone: Not provided"));
        assert!(!p.contains("must NOT count against the supplier"));
    }

    #[test]
    fn b2brazil_masked_contact_is_marked_as_platform_policy() {
        let p = b2b_content(&args("b2brazil", "", ""));
        assert!(p.contains("Contact phone: Not published - this platform never shows it"));
        assert!(p.contains("masks supplier contact names and phone numbers"));
        assert!(p.contains("must NOT count against the supplier"));
    }

    #[test]
    fn payment_methods_reach_the_prompt() {
        let p = b2b_content(&args("exporthub", "", ""));
        assert!(p.contains("Accepted payment methods: Bank wire (T/T), Western Union (WU)"));
        assert!(p.contains("include Western Union,\n        MoneyGram"));
    }

    #[test]
    fn contact_check_does_not_reuse_badge_or_company_size() {
        let p = b2b_content(&args("exporthub", "+8617728195735", ""));
        assert!(p.contains("judge ONLY the contact details"));
        assert!(p.contains("Do NOT\n        use the verified badge"));
        assert!(!p.contains("structured data"));
    }

    #[test]
    fn found_meaning_is_spelled_out() {
        let p = b2b_content(&args("alibaba", "", ""));
        assert!(p.contains("\"found\": true\n          means the GOOD thing was found"));
        assert!(p.contains("\"found\": true\n          means the BAD thing was found"));
    }
}
