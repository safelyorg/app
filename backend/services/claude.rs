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
    /// How much Claude's answers vary between runs. None = Claude's
    /// default (1.0, the most varied). The scans use SCAN_TEMPERATURE.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    pub messages: Vec<Message>,
}

/// Switch: 0.0 makes Claude give (nearly) the same verdicts every time
/// the same listing is scanned, instead of flipping between e.g.
/// "Normal" and "Abnormal" on a borderline price. Set to None to go
/// back to Claude's default.
pub const SCAN_TEMPERATURE: Option<f32> = Some(0.0);

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

/// A photo sent to Claude: either a web link ("url") that Claude
/// downloads itself, or the photo's own bytes ("base64") when the
/// website does not let Claude download it.
#[derive(Serialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

// 2. Gives you the text(that contains the actual fraud analysis) from the content block
#[derive(Debug, Deserialize)]
struct ClaudeEnvelope {
    /// Claude can (rarely) answer with no text at all - e.g. when it
    /// declines. Defaults to empty instead of failing to read.
    #[serde(default)]
    content: Vec<ContentBlock>,
    /// Why Claude stopped ("end_turn", "max_tokens", "refusal"...).
    /// Logged when the answer is empty, to see why.
    #[serde(default)]
    stop_reason: Option<String>,
}

/// The text of Claude's answer. An empty answer used to crash the
/// server ("index out of bounds: the len is 0 but the index is 0");
/// now it becomes a normal error and the reason is logged.
fn answer_text(envelope: &ClaudeEnvelope) -> Result<&str, ClaudeError> {
    match envelope.content.first() {
        Some(block) => Ok(&block.text),
        None => {
            eprintln!(
                "Safely: Claude returned an empty answer (stop_reason: {:?})",
                envelope.stop_reason
            );
            Err(ClaudeError::ParseFailed(
                "Claude returned an empty answer".to_string(),
            ))
        }
    }
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

#[derive(Clone, Debug, Default, Deserialize)]
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
    /// 100% payment before shipment / before any check (by any method).
    pub advance_payment_request: Finding,
    /// Western Union, MoneyGram, crypto, gift cards or a personal
    /// account - money that cannot be got back or traced to a company.
    /// Kept apart from advance_payment_request so full prepayment by
    /// bank transfer is not treated as harshly as these. Defaults to
    /// "not found" if Claude leaves it out.
    #[serde(default)]
    pub untraceable_payment_method: Finding,
    /// The product is a prescription drug, injectable, or another
    /// product that legally needs a licence to sell or buy (e.g.
    /// botulinum toxin, dermal fillers, prescription medicines,
    /// controlled medical devices). Defaults to "not found".
    #[serde(default)]
    pub regulated_product: Finding,
    /// The seller calls itself the manufacturer of a branded product
    /// that is made by a different, named company (e.g. a reseller of
    /// Medytox's Meditoxin calling itself a "Manufacturer"). Defaults to
    /// "not found".
    #[serde(default)]
    pub maker_claim_mismatch: Finding,
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
        "b2bmap" => Some(
            "shows the supplier's own phone number on product pages, but masks contact numbers on company pages for non-paying visitors (e.g. \"+848783xxxxx\"); a website is shown only if the supplier adds one; buyers can also contact suppliers through the platform's inquiry form. b2bmap does not verify companies: \"Free Member\", its paid plans and its paid \"B2BMAP Verified Seal\" are memberships, not a check on the company. \"Member of b2bmap since\" is when the company joined b2bmap, not when the company was founded",
        ),
        "tradewheel" => Some(
            "never shows supplier phone numbers, and shows supplier websites only to logged-in members (a website listed above came from the buyer's own logged-in view); buyers contact suppliers through the platform's inquiry form. A contact name with no phone is the normal level of detail there. TradeWheel's Gold and Platinum badges are paid membership levels, not company verification",
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

/// Master switch for sending listing photos to Claude. ON: up to 3
/// photos per listing are sent, and the Image authenticity card judges
/// them ("original" = good, "not verified" = a warning). Each photo
/// adds to the cost of a scan. Set to false to stop sending photos -
/// the card then shows "Not checked" and never counts against the
/// seller. Nothing else needs changing.
pub const IMAGE_ANALYSIS_ENABLED: bool = true;

/// The ONE, shared place that builds the real content blocks sent to
/// Claude - genuinely unified for both B2C and B2B, so a future
/// decision to re-enable image analysis only ever needs to happen in
/// one spot, not two separate, duplicated copies.
fn build_content_blocks(prompt: String, image_urls: &[String]) -> Vec<ContentItem> {
    let mut content_blocks: Vec<ContentItem> = vec![ContentItem::Text { text: prompt }];

    // Images are only sent when IMAGE_ANALYSIS_ENABLED is true, for
    // both B2C and B2B.
    if IMAGE_ANALYSIS_ENABLED {
        for url in image_urls.iter().take(3) {
            content_blocks.push(ContentItem::Image {
                source: ImageSource {
                    source_type: "url".to_string(),
                    url: Some(url.clone()),
                    media_type: None,
                    data: None,
                },
            });
        }
    }

    content_blocks
}

/// Why a request to Claude failed.
enum SendError {
    /// Claude could not load the listing photos from their web address
    /// (for example, ExportHub's robots.txt does not allow it). The
    /// photos are then downloaded by Safely and sent as bytes.
    PhotosBlocked,
    Failed(ClaudeError),
}

/// True when Claude's error says it could not load a photo from its
/// link: "This URL is disallowed by the website's robots.txt file", a
/// photo that could not be downloaded, or one in a format it can't read.
fn is_photo_load_error(status: u16, body: &str) -> bool {
    let lower = body.to_lowercase();
    status == 400
        && (lower.contains("robots.txt")
            || (lower.contains("url")
                && (lower.contains("image")
                    || lower.contains("download")
                    || lower.contains("fetch")))
            || lower.contains("could not process image"))
}

/// Sends one request to Claude and returns its answer text, without
/// code fences. Shared by the consumer (B2C) and supplier (B2B) scans.
async fn send_to_claude(content: Vec<ContentItem>) -> Result<String, SendError> {
    let has_photos = content
        .iter()
        .any(|c| matches!(c, ContentItem::Image { .. }));
    let client = Client::new();
    let api_key =
        var("ANTHROPIC_API_KEY").map_err(|_| SendError::Failed(ClaudeError::MissingApiKey))?;

    let payload = ClaudeRequest {
        model: String::from("claude-sonnet-4-6"),
        max_tokens: 2048,
        temperature: SCAN_TEMPERATURE,
        messages: vec![Message {
            role: "user".to_string(),
            content,
        }],
    };

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&payload)
        .send()
        .await
        .map_err(|e| SendError::Failed(ClaudeError::RequestFailed(e.to_string())))?;

    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|e| SendError::Failed(ClaudeError::RequestFailed(e.to_string())))?;

    if !status.is_success() {
        if has_photos && is_photo_load_error(status.as_u16(), &body_text) {
            eprintln!(
                "Safely: Claude could not load the listing photos itself ({})",
                &body_text[..body_text.len().min(200)]
            );
            return Err(SendError::PhotosBlocked);
        }
        eprintln!(
            "Safely: Claude API real, non-success status {} - body: {}",
            status,
            &body_text[..body_text.len().min(300)]
        );
        return Err(SendError::Failed(match status.as_u16() {
            401 | 403 => ClaudeError::Unauthorized,
            429 | 402 => ClaudeError::QuotaExceeded,
            code => ClaudeError::ServiceUnavailable(code),
        }));
    }

    let envelope: ClaudeEnvelope = from_str(&body_text)
        .map_err(|e| SendError::Failed(ClaudeError::ParseFailed(e.to_string())))?;
    let inner_json = answer_text(&envelope).map_err(SendError::Failed)?;
    Ok(inner_json
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string())
}

/// Largest photo sent as its own bytes. Claude accepts up to 5 MB per
/// photo; base64 makes the data about a third bigger.
const MAX_PHOTO_BYTES: usize = 3_500_000;

/// The photo's real format from its first bytes (the link's ending is
/// not reliable - ExportHub serves ".jpeg_.webp" files). Only formats
/// Claude reads are accepted.
fn photo_media_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Standard base64, as Claude expects for a photo's bytes.
fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Downloads the listing photos on Safely's own server, for websites
/// that do not let Claude download them. Photos that fail, are too big
/// or are not a real photo are skipped. Returns (format, base64) pairs.
async fn download_photos(image_urls: &[String]) -> Vec<(String, String)> {
    let Ok(client) = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36")
        .build()
    else {
        return Vec::new();
    };
    let mut photos = Vec::new();
    for url in image_urls.iter().take(3) {
        let Ok(response) = client.get(url).send().await else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        let Ok(bytes) = response.bytes().await else {
            continue;
        };
        if bytes.len() > MAX_PHOTO_BYTES {
            continue;
        }
        if let Some(media_type) = photo_media_type(&bytes) {
            photos.push((media_type.to_string(), base64_encode(&bytes)));
        }
    }
    photos
}

/// The prompt followed by the downloaded photos.
fn content_with_photo_bytes(prompt: String, photos: Vec<(String, String)>) -> Vec<ContentItem> {
    let mut content = vec![ContentItem::Text { text: prompt }];
    for (media_type, data) in photos {
        content.push(ContentItem::Image {
            source: ImageSource {
                source_type: "base64".to_string(),
                url: None,
                media_type: Some(media_type),
                data: Some(data),
            },
        });
    }
    content
}

/// Photo servers that never let Claude download photos (their
/// robots.txt blocks it), so Safely downloads the photos itself from
/// the start instead of trying the link first. Any other site that
/// blocks Claude is still caught by the automatic fallback below.
const DOWNLOAD_PHOTOS_FIRST_HOSTS: &[&str] = &["exporthub.com"];

/// True when the photos are on a server listed in
/// DOWNLOAD_PHOTOS_FIRST_HOSTS ("img.exporthub.com" counts as
/// "exporthub.com").
fn photos_need_download(image_urls: &[String]) -> bool {
    image_urls.iter().any(|url| {
        let after_scheme = url.split("://").nth(1).unwrap_or(url);
        let host = after_scheme
            .split(['/', '?', '#', ':'])
            .next()
            .unwrap_or("")
            .to_lowercase();
        DOWNLOAD_PHOTOS_FIRST_HOSTS
            .iter()
            .any(|h| host == *h || host.ends_with(&format!(".{h}")))
    })
}

/// Sends the scan with its photos as web links. If Claude is not
/// allowed to download them (e.g. ExportHub's robots.txt), Safely
/// downloads the photos itself and sends their bytes, so the photos
/// are still checked. Only if that fails too is the scan sent without
/// photos; `prompt_without_photos` builds the text for that last try.
async fn send_with_photo_fallback(
    prompt: String,
    image_urls: &[String],
    prompt_without_photos: impl FnOnce() -> String,
) -> Result<String, ClaudeError> {
    // Sites known to block Claude skip the first try (it always fails
    // there): Safely downloads their photos straight away.
    if !photos_need_download(image_urls) {
        match send_to_claude(build_content_blocks(prompt.clone(), image_urls)).await {
            Ok(answer) => return Ok(answer),
            Err(SendError::Failed(e)) => return Err(e),
            Err(SendError::PhotosBlocked) => {}
        }
    }

    let photos = download_photos(image_urls).await;
    if !photos.is_empty() {
        match send_to_claude(content_with_photo_bytes(prompt, photos)).await {
            Ok(answer) => return Ok(answer),
            Err(SendError::Failed(e)) => return Err(e),
            Err(SendError::PhotosBlocked) => {}
        }
    }

    eprintln!("Safely: the listing photos could not be loaded - scanning without photos");
    match send_to_claude(build_content_blocks(prompt_without_photos(), &[])).await {
        Ok(answer) => Ok(answer),
        Err(SendError::Failed(e)) => Err(e),
        // Cannot happen without photos; kept as a plain failure.
        Err(SendError::PhotosBlocked) => Err(ClaudeError::ServiceUnavailable(400)),
    }
}

pub async fn call_b2c_claude(args: CallClaudeArguments<'_>) -> Result<ClaudeAnalysis, ClaudeError> {
    let prompt = b2c_content(&args);
    let answer = send_with_photo_fallback(prompt, args.image_urls, || {
        b2c_content(&CallClaudeArguments {
            image_urls: &[],
            ..args
        })
    })
    .await?;
    from_str(&answer).map_err(|e| ClaudeError::ParseFailed(e.to_string()))
}

pub async fn call_b2b_claude(
    args: CallB2bClaudeArguments<'_>,
) -> Result<B2bClaudeAnalysis, ClaudeError> {
    let prompt = b2b_content(&args);
    let answer = send_with_photo_fallback(prompt, args.image_urls, || {
        b2b_content_photos_blocked(&args)
    })
    .await?;
    from_str(&answer).map_err(|e| ClaudeError::ParseFailed(e.to_string()))
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
    b2b_content_with(arg, false)
}

/// The same prompt for the second try, when the listing has photos but
/// Claude was not allowed to load them from the website.
fn b2b_content_photos_blocked(arg: &CallB2bClaudeArguments) -> String {
    b2b_content_with(arg, true)
}

fn b2b_content_with(arg: &CallB2bClaudeArguments, photos_blocked: bool) -> String {
    let image_context = if photos_blocked {
        format!(
            "{} product image(s) were found on this listing, but the website does not allow them to be loaded, so you cannot view them - use \"not verified\" as the verdict and say in the reasoning that the photos could not be checked because the website blocks them.",
            arg.image_urls.len().min(3)
        )
    } else if IMAGE_ANALYSIS_ENABLED && !arg.image_urls.is_empty() {
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

        DATES: Judge every year against today's date given below, not
        against what you assume the current year is. A founding year equal
        to the current year means the company is brand new - you may note
        that it is new, but it is NOT a future, impossible or fabricated
        date. Only a year later than today's date is impossible.

        IMPORTANT: Write every text value in the JSON below (all
        "evidence" and "reasoning" fields, and "overall_risk_notes") in
        {language}. Keep every JSON key name and every "verdict" value
        exactly as specified in English - only the free-text explanations
        should be in {language}.

        Analyze this supplier and product listing, then return ONLY a raw
        JSON object with no markdown, no code fences, no backticks, no
        explanation. Start your response with {{ and end with }}.

        Today's date: {today}
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
        - For urgency_language, advance_payment_request,
          untraceable_payment_method, regulated_product and
          maker_claim_mismatch, "found": true means the BAD thing was
          found (pressure tactics / full payment before shipment / an
          untraceable payment method / a licence-only product / a false
          manufacturer claim). "found": false means none was found.
        The example values in the JSON shape at the end show the format
        only - they are not the answer.

        For business_legitimacy: set found to true if this looks like a
        genuine, operating business; false only for strong signs of a
        shell, front, or fabricated entity - e.g. made-up, impossible or
        self-contradicting details, or a product range that is a random
        mix of unrelated industries (e.g. one company selling food,
        excavators, scrap metal and refrigerant gas at once).
        COMPANY NAMES: many real suppliers (especially in China) use broad
        legal names such as "... Information Technology Co., Ltd.",
        "... Technology Co., Ltd." or "... Trading Co., Ltd." and sell
        physical goods. A name that does not literally describe the
        product is NOT a reason to set business_legitimacy to false. Judge
        a name/product mismatch ONLY under registration_consistency, so
        it is never counted twice.
        BRANDS: many real companies sell under a brand name that differs
        from the company name, and describe the brand's own history (e.g.
        "HYM Textile" describing its "Gabbiacci" brand, "founded in Italy
        in 1971"). A brand name or brand history that differs from the
        company is NOT a fabricated or self-contradicting detail and is
        NOT a reason to set business_legitimacy to false. If the page does
        not explain how the company and the brand are related, say so
        under registration_consistency only.
        These are also NOT reasons to set business_legitimacy to false,
        on their own or added together - but DO mention each one that
        applies in the evidence, as a plain fact, so the buyer sees it
        (e.g. "No registration or tax number is shown. No street address
        is shown. The company description is very short."):
        - missing details: registration or tax number, street address,
          founding year, employee count, revenue, certifications;
        - a short, plain or generic company description (say that more
          detail would help);
        - a company name that looks unusual or made up (say so plainly;
          a broad legal name like "... Trading Co., Ltd." is normal and
          needs no mention).
        These are NOT reasons and need no mention at all:
        - how long the company has been on this or any platform (years
          on Alibaba, "Member of ExportHub: 1st year", "Member of b2bmap
          since", a TradeWheel or B2Brazil membership date). Joining a
          platform recently says nothing about how old the company is,
          so never compare the membership date with the founding year;
        - anything already judged under another field (listing detail,
          payment, contact).
        NEVER judge or comment on a person's name, nationality or
        ethnicity - that says nothing about the company. NEVER say a
        company is not registered, not traceable or does not exist: you
        cannot look up company registries. Say only what is or is not
        shown (e.g. "No registration number is shown").
        Set business_legitimacy to false only if you can name a concrete
        red flag of the kind listed above (made-up, impossible or
        self-contradicting details, or a random mix of unrelated
        industries). If you cannot, set it to true and list the missing
        or unusual details in the evidence.

        For registration_consistency: set found to true if the company's
        stated information (name, founding year, scale) hangs together
        coherently; false ONLY if you can name a real, concrete
        inconsistency. Missing fields alone are not an inconsistency. A
        broad company name (see COMPANY NAMES above) is consistent when
        the company's own description, business type or main products
        cover what it sells (e.g. "hardware product customization" covers
        a keyboard); set false for the name only if nothing in the
        company's own information explains the products.

        For listing_specificity: set found to true if the listing describes
        a real, specific product with genuine, plausible details (concrete
        attributes, specs, materials, origin); false if it is template-like,
        vague, or nonsensical for the stated industry. Keyword-heavy titles
        are standard practice on B2B platforms for search visibility and
        are NOT on their own a sign of a template listing - judge the
        attributes and description instead.
        GENERIC TEXT: also set listing_specificity to false when BOTH the
        product description and the company description are only generic,
        copy-paste sales text that could belong to any supplier - e.g. "we
        are a leading professional manufacturer with high quality and
        competitive price, OEM/ODM welcome, best service" - with no
        concrete detail about this product or this company (no specs,
        materials, factory location, certifications, product range or real
        numbers). Quote one generic phrase in the evidence. Do NOT judge
        whether the text was written by AI or machine-translated: fluent,
        AI-assisted, translated or imperfect English is normal for honest
        suppliers. Judge only whether real, specific details are present.
        PLATFORM TEXT: ExportHub writes a paragraph for every product
        automatically from the supplier's form ("If you want to get the
        best then ... is a great option to trust", "we accepts all
        payments methods like ...", "produce up to High Piece every
        month"). Its odd wording comes from the platform, not from the
        supplier - never call it a template placeholder or quote it as the
        supplier's own text. Judge only the details the supplier added.
        If either description gives real, specific details, the text is
        not generic.

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

        For advance_payment_request: set found to true only if the
        terms require the FULL price (100%) to be paid before the goods
        are shipped or before any sample or inspection - including split
        terms that still add up to 100% before shipment (e.g. "40%
        advance, 60% before shipment"). Partial deposits with the balance
        paid against shipping documents (e.g. "30% advance, 70% against
        copy of B/L"), L/C, D/A, D/P and platform escrow (including
        Alibaba Trade Assurance, Alibaba's own order protection) are
        standard B2B terms and must NOT be flagged. Judge only how much is
        paid before shipment here, not the payment method, and never
        call Western Union, MoneyGram, crypto or gift cards standard or
        safe in the evidence.

        For untraceable_payment_method: Western Union, MoneyGram,
        cryptocurrency, gift cards and paying a personal (individual's)
        account cannot be reversed or traced to a company. Set found to
        true only if ONE of these is true:
        (a) such a method is the ONLY way to pay that is offered;
        (b) the supplier pushes buyers toward it (e.g. "Western Union
            only", "discount for Western Union", "pay by crypto");
        (c) payment goes to a personal account instead of the company's;
        (d) most of the listed ways to pay are such methods or cash (e.g.
            "Cash, Western Union, MoneyGram, Credit Card").
        If such a method is only one option in a list that also offers a
        protected or traceable way to pay - platform order protection such
        as Alibaba Trade Assurance, bank wire (T/T) to the company, L/C,
        D/A, D/P, credit card or PayPal - and nothing pushes the risky
        option, set found to false. In that case still name the risky
        method in the evidence and say the buyer should use the protected
        option instead. Bank wire (T/T), L/C, D/A, D/P and platform escrow
        are normal and are never flagged here.

        For regulated_product: set found to true only if the product is
        one that legally needs a licence or prescription to sell or buy:
        prescription medicines, injectables such as botulinum toxin
        ("botox", Meditoxin, Botulax, Nabota) or dermal fillers, local
        anaesthetics for clinical use, controlled substances, or medical
        devices that only licensed professionals may buy. Name the
        product type in the evidence. Ordinary cosmetics, supplements,
        food, and general medical supplies (gloves, masks, bandages) are
        NOT regulated for this question and must not be flagged.

        For maker_claim_mismatch: set found to true only if the seller
        says it is a manufacturer (business type "Manufacturer" or "we
        manufacture") AND the listed product is a branded product that is
        made by a different, named company. Name both companies in the
        evidence (e.g. "Meditoxin is made by Medytox, but PHARMOCEAN
        calls itself a manufacturer"). Resellers that call themselves a
        supplier, distributor or trader are fine and must not be flagged.
        Generic or unbranded products are never flagged here.

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
        "untraceable_payment_method": {{ "found": false, "evidence": "" }},
        "regulated_product": {{ "found": false, "evidence": "" }},
        "maker_claim_mismatch": {{ "found": false, "evidence": "" }},
        "image_authenticity": {{ "verdict": "not verified", "reasoning": "" }},
        "overall_risk_notes": ""
        }}
        "#,
        today = chrono::Utc::now().format("%Y-%m-%d"),
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

    #[test]
    fn scan_requests_use_a_fixed_temperature() {
        let request = ClaudeRequest {
            model: "m".to_string(),
            max_tokens: 1,
            temperature: SCAN_TEMPERATURE,
            messages: vec![],
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["temperature"], 0.0);

        let default = ClaudeRequest {
            temperature: None,
            ..request
        };
        let json = serde_json::to_value(&default).unwrap();
        assert!(
            json.get("temperature").is_none(),
            "None leaves Claude's default"
        );
    }

    #[test]
    fn prompt_carries_todays_date() {
        let args = CallB2bClaudeArguments {
            platform: "tradewheel",
            company_name: "Dadal General Trading",
            year_established: "2026",
            platform_verified: false,
            employee_count: "",
            company_description: "",
            contact_name: "",
            contact_phone: "",
            website_url: "",
            product_title: "Opal",
            product_description: "",
            image_urls: &[],
            language: "en",
            unit_price: "",
            minimum_order_quantity: "",
            payment_type: "",
        };
        let prompt = b2b_content(&args);
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        assert!(prompt.contains(&format!("Today's date: {}", today)));
        assert!(prompt.contains("NOT a future, impossible or fabricated"));
    }

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
        let flat = p.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat.contains("Western Union, MoneyGram, cryptocurrency, gift cards"));
    }

    #[test]
    fn a_risky_method_is_flagged_only_when_forced_or_pushed() {
        let p = b2b_content(&args("alibaba", "", ""));
        let flat = p.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat.contains("(a) such a method is the ONLY way to pay that is offered"));
        assert!(flat.contains("(b) the supplier pushes buyers toward it"));
        assert!(flat.contains("(c) payment goes to a personal account"));
        assert!(flat.contains(
            "only one option in a list that also offers a protected or traceable way to pay"
        ));
        assert!(flat.contains("Alibaba Trade Assurance"));
        assert!(flat.contains("say the buyer should use the protected option instead"));
    }

    #[test]
    fn full_prepayment_and_untraceable_methods_are_separate_questions() {
        let p = b2b_content(&args("b2bmap", "", ""));
        let flat = p.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat.contains("\"40% advance, 60% before shipment\""));
        assert!(flat.contains("\"30% advance, 70% against copy of B/L\""));
        assert!(flat.contains("For untraceable_payment_method: Western Union, MoneyGram"));
        assert!(flat.contains("Set found to true only if ONE of these is true"));
        assert!(flat.contains("\"untraceable_payment_method\": { \"found\": false"));
    }

    #[test]
    fn missing_untraceable_field_defaults_to_not_found() {
        let json = r#"{
            "business_legitimacy": {"found": true, "evidence": ""},
            "registration_consistency": {"found": true, "evidence": ""},
            "listing_specificity": {"found": true, "evidence": ""},
            "pricing_transparency": {"verdict": "normal", "reasoning": ""},
            "contact_verifiability": {"found": true, "evidence": ""},
            "urgency_language": {"found": false, "evidence": ""},
            "advance_payment_request": {"found": true, "evidence": "100% before shipment"},
            "image_authenticity": {"verdict": "not verified", "reasoning": ""},
            "overall_risk_notes": ""
        }"#;
        let a: B2bClaudeAnalysis = serde_json::from_str(json).unwrap();
        assert!(a.advance_payment_request.found);
        assert!(!a.untraceable_payment_method.found);
    }

    #[test]
    fn contact_check_does_not_reuse_badge_or_company_size() {
        let p = b2b_content(&args("exporthub", "+8617728195735", ""));
        assert!(p.contains("judge ONLY the contact details"));
        assert!(p.contains("Do NOT\n        use the verified badge"));
        assert!(!p.contains("structured data"));
    }

    #[test]
    fn b2bmap_policy_reaches_the_prompt() {
        let p = b2b_content(&args("b2bmap", "+84878369911", ""));
        let flat = p.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat.contains("b2bmap does not verify companies"));
        assert!(flat.contains("not when the company was founded"));
        assert!(flat.contains("must NOT count against the supplier"));
        assert!(p.contains("Contact phone: +84878369911"));
    }

    #[test]
    fn business_check_needs_a_concrete_red_flag() {
        let p = b2b_content(&args("exporthub", "", ""));
        let flat = p.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat.contains("is NOT a reason to set business_legitimacy to false"));
        assert!(flat.contains("Judge a name/product mismatch ONLY under registration_consistency"));
        assert!(flat.contains("random mix of unrelated industries"));
        assert!(flat.contains("DO mention each one that applies in the evidence, as a plain fact"));
        assert!(flat.contains(
            "missing details: registration or tax number, street address, founding year"
        ));
        assert!(flat.contains("a company name that looks unusual or made up (say so plainly"));
        assert!(
            flat.contains(
                "how long the company has been on this or any platform (years on Alibaba"
            )
        );
        assert!(flat.contains("never compare the membership date with the founding year"));
        assert!(
            flat.contains("NEVER judge or comment on a person's name, nationality or ethnicity")
        );
        assert!(flat.contains("NEVER say a company is not registered"));
        assert!(
            flat.contains("set it to true and list the missing or unusual details in the evidence")
        );
        assert!(flat.contains("only if you can name a concrete red flag"));
        assert!(flat.contains("\"hardware product customization\" covers a keyboard"));
        assert!(!flat.contains("generic or nonsensical company name"));
    }

    #[test]
    fn generic_copy_paste_text_is_judged_but_ai_writing_is_not() {
        let p = b2b_content(&args("alibaba", "", ""));
        let flat = p.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat.contains("GENERIC TEXT: also set listing_specificity to false when BOTH"));
        assert!(flat.contains("could belong to any supplier"));
        assert!(flat.contains("Quote one generic phrase in the evidence"));
        assert!(
            flat.contains("Do NOT judge whether the text was written by AI or machine-translated")
        );
        assert!(flat.contains(
            "If either description gives real, specific details, the text is not generic"
        ));
    }

    #[test]
    fn regulated_product_questions_are_asked_and_default_to_not_found() {
        let p = b2b_content(&args("tradewheel", "", ""));
        let flat = p.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat.contains("For regulated_product: set found to true only if"));
        assert!(flat.contains("botulinum toxin"));
        assert!(flat.contains("For maker_claim_mismatch: set found to true only if"));
        assert!(flat.contains("\"maker_claim_mismatch\": { \"found\": false"));

        // Older answers without the new fields still parse.
        let json = r#"{
            "business_legitimacy": {"found": true, "evidence": ""},
            "registration_consistency": {"found": true, "evidence": ""},
            "listing_specificity": {"found": true, "evidence": ""},
            "pricing_transparency": {"verdict": "normal", "reasoning": ""},
            "contact_verifiability": {"found": true, "evidence": ""},
            "urgency_language": {"found": false, "evidence": ""},
            "advance_payment_request": {"found": false, "evidence": ""},
            "image_authenticity": {"verdict": "not verified", "reasoning": ""},
            "overall_risk_notes": ""
        }"#;
        let a: B2bClaudeAnalysis = serde_json::from_str(json).unwrap();
        assert!(!a.regulated_product.found);
        assert!(!a.maker_claim_mismatch.found);
    }

    #[test]
    fn empty_claude_answer_is_an_error_not_a_crash() {
        let envelope: ClaudeEnvelope =
            serde_json::from_str(r#"{"content": [], "stop_reason": "refusal"}"#).unwrap();
        assert!(answer_text(&envelope).is_err());
        let envelope: ClaudeEnvelope =
            serde_json::from_str(r#"{"content": [{"type": "text", "text": "{}"}]}"#).unwrap();
        assert_eq!(answer_text(&envelope).unwrap(), "{}");
    }

    #[test]
    fn found_meaning_is_spelled_out() {
        let p = b2b_content(&args("alibaba", "", ""));
        assert!(p.contains("\"found\": true\n          means the GOOD thing was found"));
        let flat = p.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat.contains("\"found\": true means the BAD thing was found"));
    }
}

#[cfg(test)]
mod photo_fallback_tests {
    use super::*;

    #[test]
    fn robots_txt_and_photo_download_errors_are_photo_errors() {
        let robots = r#"{"type":"error","error":{"type":"invalid_request_error","message":"This URL is disallowed by the website's robots.txt file."}}"#;
        assert!(is_photo_load_error(400, robots));
        assert!(is_photo_load_error(
            400,
            r#"{"error":{"message":"Unable to download the file at the image URL"}}"#
        ));
        // Other errors are not retried without photos.
        assert!(!is_photo_load_error(
            400,
            r#"{"error":{"message":"max_tokens is too large"}}"#
        ));
        assert!(!is_photo_load_error(429, robots));
        assert!(!is_photo_load_error(401, "invalid x-api-key"));
    }
}

#[cfg(test)]
mod photo_bytes_tests {
    use super::*;

    #[test]
    fn exporthub_photos_are_downloaded_by_safely_first() {
        let eh = vec![
            "https://img.exporthub.com/storage/app/images/products/7/5/o_1719417416_75.jpeg_.webp"
                .to_string(),
        ];
        assert!(photos_need_download(&eh));
        // Other sites keep sending the link first.
        for url in [
            "https://s.alicdn.com/@sc04/kf/H1.jpg",
            "https://b2bmap.com/product-image/202609/x.jpg",
            "https://cdn.b2brazil.com/storages/company/1/products/x.png.webp",
            "https://notexporthub.com/x.jpg",
        ] {
            assert!(!photos_need_download(&[url.to_string()]), "{url}");
        }
        assert!(!photos_need_download(&[]));
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(&[0xFF, 0xD8, 0xFF, 0xE0]), "/9j/4A==");
    }

    #[test]
    fn photo_format_comes_from_the_bytes() {
        assert_eq!(
            photo_media_type(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some("image/jpeg")
        );
        assert_eq!(photo_media_type(b"\x89PNG\r\n"), Some("image/png"));
        assert_eq!(
            photo_media_type(b"RIFF\x10\x00\x00\x00WEBPVP8 "),
            Some("image/webp")
        );
        assert_eq!(photo_media_type(b"<html>blocked</html>"), None);
    }

    #[test]
    fn photo_bytes_are_sent_in_claudes_base64_shape() {
        let content =
            content_with_photo_bytes("p".into(), vec![("image/jpeg".into(), "QUJD".into())]);
        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json[1]["type"], "image");
        assert_eq!(json[1]["source"]["type"], "base64");
        assert_eq!(json[1]["source"]["media_type"], "image/jpeg");
        assert_eq!(json[1]["source"]["data"], "QUJD");
        assert!(json[1]["source"].get("url").is_none());
        // Photos sent as links keep their old shape.
        let content = build_content_blocks("p".into(), &["https://x/1.jpg".to_string()]);
        let json = serde_json::to_value(&content).unwrap();
        if IMAGE_ANALYSIS_ENABLED {
            assert_eq!(json[1]["source"]["type"], "url");
            assert_eq!(json[1]["source"]["url"], "https://x/1.jpg");
            assert!(json[1]["source"].get("data").is_none());
        }
    }

    #[test]
    fn prompt_explains_brands_platform_text_and_mostly_risky_payment() {
        let p = b2b_content(&CallB2bClaudeArguments {
            platform: "exporthub",
            company_name: "HYM Textile",
            year_established: "Unknown",
            platform_verified: false,
            employee_count: "Unknown",
            company_description: "",
            contact_name: "",
            contact_phone: "",
            website_url: "",
            product_title: "Light Blue Suit",
            product_description: "",
            image_urls: &[],
            language: "en",
            unit_price: "",
            minimum_order_quantity: "",
            payment_type: "",
        });
        let flat = p.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat.contains("BRANDS: many real companies sell under a brand name"));
        assert!(flat.contains("PLATFORM TEXT: ExportHub writes a paragraph"));
        assert!(flat.contains("(d) most of the listed ways to pay are such methods or cash"));
    }
}
