use backend::errors::claude::ClaudeError;
use backend::services::shared::claude::{CallB2bClaudeArguments, b2b_content, call_b2b_claude};
use serial_test::serial;
use std::env::{remove_var, set_var, var};

fn make_b2b_args() -> CallB2bClaudeArguments<'static> {
    CallB2bClaudeArguments {
        platform: "b2brazil",
        company_name: "Test Co",
        year_established: "2013",
        platform_verified: true,
        employee_count: "0-10",
        company_description: "Test description",
        contact_name: "",
        contact_phone: "",
        website_url: "",
        product_title: "Test Product",
        product_description: "Test description",
        image_urls: &[],
        language: "en",
        unit_price: "",
        minimum_order_quantity: "",
        payment_type: "",
    }
}

// ─────────────────────────────────────────────────────────
// b2b_content - confirming today's new additions are genuinely present
// ─────────────────────────────────────────────────────────

#[test]
fn b2b_content_explicitly_defines_the_urgency_language_question() {
    let args = make_b2b_args();
    let prompt = b2b_content(&args);
    assert!(prompt.contains("urgency_language"));
    assert!(prompt.to_lowercase().contains("deal expires today"));
}

#[test]
fn b2b_content_constrains_image_authenticity_to_exact_words() {
    let args = make_b2b_args();
    let prompt = b2b_content(&args);
    assert!(prompt.contains("\"original\" or"));
    assert!(prompt.contains("not verified"));
    assert!(prompt.contains("\"verdict\": \"not verified\""));
}

#[test]
fn b2b_content_has_no_leftover_duplicate_json_shape_line() {
    let args = make_b2b_args();
    let prompt = b2b_content(&args);
    let occurrences = prompt.matches("Return JSON in exactly this shape:").count();
    assert_eq!(
        occurrences, 1,
        "expected exactly one, not a leftover duplicate"
    );
}

// ─────────────────────────────────────────────────────────
// language - confirming today's language instruction is genuinely
// present in the prompt, not just accepted as a parameter
// ─────────────────────────────────────────────────────────

#[test]
fn b2b_content_writes_the_real_portuguese_instruction_when_requested() {
    let mut args = make_b2b_args();
    args.language = "pt-br";
    let prompt = b2b_content(&args);
    assert!(prompt.contains("Portuguese (Brazil)"));
}

// ─────────────────────────────────────────────────────────
// call_b2b_claude - missing API key
// ─────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn call_b2b_claude_missing_api_key() {
    dotenvy::dotenv().ok();
    let original_key = var("ANTHROPIC_API_KEY").ok();
    unsafe {
        remove_var("ANTHROPIC_API_KEY");
    }
    let result = call_b2b_claude(make_b2b_args()).await;
    match result {
        Err(ClaudeError::MissingApiKey) => {}
        Err(other) => panic!("expected MissingApiKey, got a different error: {:?}", other),
        Ok(_) => panic!("expected the call to fail without a real API key, but it succeeded"),
    }
    unsafe {
        if let Some(key) = original_key {
            set_var("ANTHROPIC_API_KEY", key);
        }
    }
}
