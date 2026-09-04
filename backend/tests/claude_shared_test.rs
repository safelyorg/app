use backend::services::claude::{
    b2b_content, b2c_content, call_b2b_claude, CallB2bClaudeArguments, CallClaudeArguments,
};
use backend::errors::claude::ClaudeError;
use std::env::{remove_var, set_var, var};

fn make_b2b_args() -> CallB2bClaudeArguments<'static> {
    CallB2bClaudeArguments {
        platform: "b2brazil",
        company_name: "Test Co",
        year_established: "2013",
        platform_verified: true,
        employee_count: "0-10",
        product_title: "Test Product",
        product_description: "Test description",
        image_urls: &[],
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
    assert_eq!(occurrences, 1, "expected exactly one, not a leftover duplicate");
}

// ─────────────────────────────────────────────────────────
// b2c_content - confirming the image_authenticity constraint was added
// ─────────────────────────────────────────────────────────

#[test]
fn b2c_content_constrains_image_authenticity_to_exact_words() {
    let image_urls: Vec<String> = vec![];
    let args = CallClaudeArguments {
        platform: "olx",
        seller_name: "Test",
        seller_account_age: "1 year",
        title: "Test",
        price: 1000,
        description: "Test",
        image_urls: &image_urls,
    };
    let prompt = b2c_content(&args);
    assert!(prompt.contains("not verified"));
}

// ─────────────────────────────────────────────────────────
// call_b2b_claude - missing API key (parallel to the existing b2c test)
// ─────────────────────────────────────────────────────────

#[tokio::test]
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
