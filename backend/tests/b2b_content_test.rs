use backend::services::claude::{CallB2bClaudeArguments, b2b_content};

#[test]
fn b2b_content_includes_the_real_company_name() {
    let args = CallB2bClaudeArguments {
        platform: "b2brazil",
        company_name: "Akurat Consultoria Empresarial",
        year_established: "2013",
        platform_verified: false,
        employee_count: "0-10",
        product_title: "Test Product",
        product_description: "Test description",
    };
    let prompt = b2b_content(&args);
    assert!(prompt.contains("Akurat Consultoria Empresarial"));
}

#[test]
fn b2b_content_includes_the_real_year_and_employee_count() {
    let args = CallB2bClaudeArguments {
        platform: "b2brazil",
        company_name: "Test Co",
        year_established: "2013",
        platform_verified: true,
        employee_count: "0-10",
        product_title: "Test Product",
        product_description: "Test description",
    };
    let prompt = b2b_content(&args);
    assert!(prompt.contains("2013"));
    assert!(prompt.contains("0-10"));
    assert!(prompt.contains("true"));
}

#[test]
fn b2b_content_includes_the_real_product_details() {
    let args = CallB2bClaudeArguments {
        platform: "b2brazil",
        company_name: "Test Co",
        year_established: "2013",
        platform_verified: false,
        employee_count: "0-10",
        product_title: "Precision Microcast Parts",
        product_description: "Industrial casting components",
    };
    let prompt = b2b_content(&args);
    assert!(prompt.contains("Precision Microcast Parts"));
    assert!(prompt.contains("Industrial casting components"));
}

#[test]
fn b2b_content_explicitly_tells_claude_not_to_apply_consumer_fraud_patterns() {
    let args = CallB2bClaudeArguments {
        platform: "b2brazil",
        company_name: "Test Co",
        year_established: "2013",
        platform_verified: false,
        employee_count: "0-10",
        product_title: "Test",
        product_description: "Test",
    };
    let prompt = b2b_content(&args);
    assert!(prompt.to_lowercase().contains("not a consumer marketplace"));
    assert!(prompt.contains("urgency language"));
}

#[test]
fn b2b_content_produces_genuinely_different_text_for_different_inputs() {
    let args_a = CallB2bClaudeArguments {
        platform: "b2brazil",
        company_name: "Company A",
        year_established: "2010",
        platform_verified: false,
        employee_count: "0-10",
        product_title: "Product A",
        product_description: "Description A",
    };
    let args_b = CallB2bClaudeArguments {
        platform: "b2brazil",
        company_name: "Company B",
        year_established: "2020",
        platform_verified: true,
        employee_count: "50-100",
        product_title: "Product B",
        product_description: "Description B",
    };
    assert_ne!(b2b_content(&args_a), b2b_content(&args_b));
}
