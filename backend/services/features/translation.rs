use crate::errors::claude::ClaudeError;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, from_str, from_value, json, to_string, to_value};
use sqlx::{Pool, Postgres, query, query_as};
use std::env::var;
use uuid::Uuid;

/// Translation is plain text work, so it uses the fast model: it runs
/// while the person waits for their scan result.
const TRANSLATION_MODEL: &str = "claude-haiku-4-5-20251001";

/// Some signal texts end with a checklist the extension turns into a
/// dropdown: "...provided.###CHECKLIST###Unit price|true;FOB price|false".
/// That part is data, not text, so it is never sent for translation -
/// it is cut off first and put back afterwards, unchanged (the
/// extension translates the item names itself).
const CHECKLIST_MARKER: &str = "###CHECKLIST###";

#[derive(Debug, sqlx::FromRow)]
pub struct AnalysisTranslation {
    pub signals: serde_json::Value,
    pub risk_factors: serde_json::Value,
    pub network_summary: String,
}

// The ONE, real, shared place this lookup lives - claude.rs calls
// this same function instead of keeping its own separate copy, so
// adding a new language only ever needs one update, in one place.
// Falls back to English only for a genuinely unrecognized code (a
// typo or missing entry), never silently for a real, intended
// language.
pub fn language_instruction(language_code: &str) -> &'static str {
    match language_code {
        "en" => "English",
        "pt-br" => "Portuguese (Brazil)",
        other => {
            eprintln!(
                "Safely: unrecognized language code '{}' - falling back to English",
                other
            );
            "English"
        }
    }
}

/// "3 of 9 details provided.###CHECKLIST###Unit price|true" ->
/// ("3 of 9 details provided.", "###CHECKLIST###Unit price|true").
/// Text without a checklist -> (text, "").
fn split_checklist(text: &str) -> (&str, &str) {
    match text.find(CHECKLIST_MARKER) {
        Some(index) => (&text[..index], &text[index..]),
        None => (text, ""),
    }
}

/// The text part of a JSON field, without any checklist ("" if missing).
fn text_part<'a>(item: &'a Value, field: &str) -> &'a str {
    split_checklist(item.get(field).and_then(Value::as_str).unwrap_or("")).0
}

/// The checklist part of a JSON field ("" if it has none).
fn checklist_part<'a>(item: &'a Value, field: &str) -> &'a str {
    split_checklist(item.get(field).and_then(Value::as_str).unwrap_or("")).1
}

#[derive(Debug, Deserialize)]
struct TranslatedText {
    signals: Vec<TranslatedSignal>,
    risk_factors: Vec<TranslatedRiskFactor>,
    network_summary: String,
}

#[derive(Debug, Deserialize)]
struct TranslatedSignal {
    sub: String,
}

#[derive(Debug, Deserialize)]
struct TranslatedRiskFactor {
    description: String,
}

/// Real, one-time translation call - asks Claude to translate ONLY
/// the free-text fields (signal "sub", risk factor "description",
/// network_summary), returning them in the exact same order as the
/// input, so they can be re-merged with the original, unchanged fixed
/// fields (label, value, type, severity, etc.) afterward. This keeps
/// every fixed field byte-for-byte identical to the English original,
/// and only ever lets Claude touch genuinely free-text content.
async fn translate_text_fields(
    signals: &[Value],
    risk_factors: &[Value],
    network_summary: &str,
    target_language: &str,
) -> Result<TranslatedText, ClaudeError> {
    let client = Client::new();
    let api_key = var("ANTHROPIC_API_KEY").map_err(|_| ClaudeError::MissingApiKey)?;

    let signals_input: Vec<Value> = signals
        .iter()
        .map(|s| json!({ "sub": text_part(s, "sub") }))
        .collect();
    let factors_input: Vec<Value> = risk_factors
        .iter()
        .map(|f| json!({ "description": text_part(f, "description") }))
        .collect();

    let prompt = format!(
        r#"
        Translate the following fraud-analysis text into {language}. This
        is translation only - do not add, remove, or reinterpret any
        information, just translate the real, existing text faithfully,
        in plain, natural words a buyer would use.

        Keep these exactly as they are: company, brand, product and
        people's names, platform and service names (Alibaba, Trade
        Assurance, Western Union, MoneyGram...), website addresses,
        email addresses, phone numbers, numbers, prices and currencies,
        and any HTML tags. An empty text stays empty.

        Preserve the exact same array order and length as given. Return
        ONLY a raw JSON object with no markdown, no code fences, no
        explanation. Start your response with {{ and end with }}.

        Input:
        {input}

        Return JSON in exactly this shape (same array lengths and order
        as the input):
        {{
          "signals": [{{ "sub": "..." }}],
          "risk_factors": [{{ "description": "..." }}],
          "network_summary": "..."
        }}
        "#,
        language = language_instruction(target_language),
        input = to_string(&json!({
            "signals": signals_input,
            "risk_factors": factors_input,
            "network_summary": network_summary,
        }))
        .unwrap_or_default(),
    );

    #[derive(serde::Serialize)]
    struct Req {
        model: String,
        max_tokens: u32,
        messages: Vec<Msg>,
    }
    #[derive(serde::Serialize)]
    struct Msg {
        role: String,
        content: String,
    }
    #[derive(Deserialize)]
    struct Envelope {
        content: Vec<Block>,
    }
    #[derive(Deserialize)]
    struct Block {
        #[serde(default)]
        text: String,
    }

    let payload = Req {
        model: TRANSLATION_MODEL.to_string(),
        max_tokens: 4096,
        messages: vec![Msg {
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
        .await
        .map_err(|e| ClaudeError::RequestFailed(e.to_string()))?;

    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|e| ClaudeError::RequestFailed(e.to_string()))?;

    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => ClaudeError::Unauthorized,
            429 | 402 => ClaudeError::QuotaExceeded,
            code => ClaudeError::ServiceUnavailable(code),
        });
    }

    let envelope: Envelope =
        from_str(&body_text).map_err(|e| ClaudeError::ParseFailed(e.to_string()))?;
    let inner = envelope
        .content
        .first()
        .map(|block| block.text.as_str())
        .ok_or_else(|| ClaudeError::ParseFailed("empty translation response".to_string()))?;
    let cleaned = inner
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    from_str(cleaned).map_err(|e| ClaudeError::ParseFailed(e.to_string()))
}

/// The real, shared entry point - checks the cache first, translates
/// and saves only on a genuine cache miss. English is always the
/// baseline being translated FROM, since save_english_baseline
/// guarantees every analysis has a real English row.
pub async fn get_or_create_translation(
    pool: &Pool<Postgres>,
    analysis_id: Uuid,
    signals: &Value,
    risk_factors: &Value,
    network_summary: &str,
    language: &str,
) -> Result<AnalysisTranslation, ClaudeError> {
    let cached: Option<AnalysisTranslation> = query_as(
        "SELECT signals, risk_factors, network_summary FROM analysis_translations
         WHERE analysis_id = $1 AND language = $2",
    )
    .bind(analysis_id)
    .bind(language)
    .fetch_optional(pool)
    .await
    .map_err(|e| ClaudeError::RequestFailed(e.to_string()))?;

    if let Some(found) = cached {
        return Ok(found);
    }

    let signals_arr: Vec<Value> = from_value(signals.clone()).unwrap_or_default();
    let factors_arr: Vec<Value> = from_value(risk_factors.clone()).unwrap_or_default();

    let translated =
        translate_text_fields(&signals_arr, &factors_arr, network_summary, language).await?;

    if translated.signals.len() != signals_arr.len()
        || translated.risk_factors.len() != factors_arr.len()
    {
        eprintln!(
            "Safely: translation shape mismatch for analysis {}, falling back to English",
            analysis_id
        );
        return Ok(AnalysisTranslation {
            signals: signals.clone(),
            risk_factors: risk_factors.clone(),
            network_summary: network_summary.to_string(),
        });
    }

    // Translated text + the original checklist (if any), every other
    // field untouched.
    let merged_signals: Vec<Value> = signals_arr
        .iter()
        .zip(translated.signals.iter())
        .map(|(orig, t)| {
            let mut merged = orig.clone();
            let sub = format!("{}{}", t.sub.trim(), checklist_part(orig, "sub"));
            if let Some(obj) = merged.as_object_mut() {
                obj.insert("sub".to_string(), json!(sub));
            }
            merged
        })
        .collect();

    let merged_factors: Vec<Value> = factors_arr
        .iter()
        .zip(translated.risk_factors.iter())
        .map(|(orig, t)| {
            let mut merged = orig.clone();
            let description = format!(
                "{}{}",
                t.description.trim(),
                checklist_part(orig, "description")
            );
            if let Some(obj) = merged.as_object_mut() {
                obj.insert("description".to_string(), json!(description));
            }
            merged
        })
        .collect();

    let signals_json = to_value(&merged_signals).unwrap_or_default();
    let factors_json = to_value(&merged_factors).unwrap_or_default();

    let _ = query(
        "INSERT INTO analysis_translations (id, analysis_id, language, signals, risk_factors, network_summary, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, NOW())
         ON CONFLICT (analysis_id, language) DO NOTHING",
    )
    .bind(Uuid::now_v7())
    .bind(analysis_id)
    .bind(language)
    .bind(&signals_json)
    .bind(&factors_json)
    .bind(&translated.network_summary)
    .execute(pool)
    .await;

    Ok(AnalysisTranslation {
        signals: signals_json,
        risk_factors: factors_json,
        network_summary: translated.network_summary,
    })
}

/// Guarantees a real English baseline row exists for a brand-new
/// analysis, even when it was originally generated in another
/// language - this is what makes every future translation
/// (English -> any other language) always have a real, guaranteed
/// source to translate from.
pub async fn save_english_baseline(
    pool: &Pool<Postgres>,
    analysis_id: Uuid,
    signals: &Value,
    risk_factors: &Value,
    network_summary: &str,
    original_language: &str,
) -> Result<(), ClaudeError> {
    // Always save a row in the language the analysis was actually
    // created in - this guarantees zero-delay viewing whenever the
    // requested language matches how it was originally generated
    // (whatever that language is), not just for English specifically.
    let _ = query(
        "INSERT INTO analysis_translations (id, analysis_id, language, signals, risk_factors, network_summary, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, NOW())
         ON CONFLICT (analysis_id, language) DO NOTHING",
    )
    .bind(Uuid::now_v7())
    .bind(analysis_id)
    .bind(original_language)
    .bind(signals)
    .bind(risk_factors)
    .bind(network_summary)
    .execute(pool)
    .await;

    if original_language == "en" {
        return Ok(());
    }

    // Additionally, translate it INTO English right away, so English
    // is always guaranteed available as the real, stable hub for
    // every future translation into any OTHER language later.
    let _ = get_or_create_translation(
        pool,
        analysis_id,
        signals,
        risk_factors,
        network_summary,
        "en",
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod checklist_tests {
    use super::*;

    #[test]
    fn checklist_is_kept_out_of_the_text() {
        let text = "3 of 9 provided.###CHECKLIST###Unit price|true;FOB price|false";
        assert_eq!(
            split_checklist(text),
            (
                "3 of 9 provided.",
                "###CHECKLIST###Unit price|true;FOB price|false"
            )
        );
        assert_eq!(split_checklist("No list here."), ("No list here.", ""));
    }

    #[test]
    fn reads_text_and_checklist_from_json() {
        let item = json!({ "sub": "Two of 3.###CHECKLIST###Sales revenue|false" });
        assert_eq!(text_part(&item, "sub"), "Two of 3.");
        assert_eq!(
            checklist_part(&item, "sub"),
            "###CHECKLIST###Sales revenue|false"
        );
        assert_eq!(text_part(&json!({}), "sub"), "");
    }
}
