use backend::services::claude::{CallB2bClaudeArguments, b2b_content};

#[test]
fn b2b_content_includes_the_real_company_name() {
    let args = CallB2bClaudeArguments {
        platform: "b2brazil",
        company_name: "Akurat Consultoria Empresarial",
        year_established: "2013",
        platform_verified: false,
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
        company_description: "Test description",
        contact_name: "",
        contact_phone: "",
        website_url: "",
        product_title: "Precision Microcast Parts",
        product_description: "Industrial casting components",
        image_urls: &[],
        language: "en",
        unit_price: "",
        minimum_order_quantity: "",
        payment_type: "",
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
        company_description: "Test description",
        contact_name: "",
        contact_phone: "",
        website_url: "",
        product_title: "Test",
        product_description: "Test",
        image_urls: &[],
        language: "en",
        unit_price: "",
        minimum_order_quantity: "",
        payment_type: "",
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
        company_description: "Description A",
        contact_name: "",
        contact_phone: "",
        website_url: "",
        product_title: "Product A",
        product_description: "Description A",
        image_urls: &[],
        language: "en",
        unit_price: "",
        minimum_order_quantity: "",
        payment_type: "",
    };
    let args_b = CallB2bClaudeArguments {
        platform: "b2brazil",
        company_name: "Company B",
        year_established: "2020",
        platform_verified: true,
        employee_count: "50-100",
        company_description: "Description B",
        contact_name: "",
        contact_phone: "",
        website_url: "",
        product_title: "Product B",
        product_description: "Description B",
        image_urls: &[],
        language: "en",
        unit_price: "",
        minimum_order_quantity: "",
        payment_type: "",
    };
    assert_ne!(b2b_content(&args_a), b2b_content(&args_b));
}

#[test]
fn b2b_content_includes_the_portuguese_instruction_when_language_is_pt_br() {
    let args = CallB2bClaudeArguments {
        platform: "b2brazil",
        company_name: "Test Co",
        year_established: "2013",
        platform_verified: false,
        employee_count: "0-10",
        company_description: "Test description",
        contact_name: "",
        contact_phone: "",
        website_url: "",
        product_title: "Test",
        product_description: "Test",
        image_urls: &[],
        language: "pt-br",
        unit_price: "",
        minimum_order_quantity: "",
        payment_type: "",
    };
    let prompt = b2b_content(&args);
    assert!(
        prompt.contains("Portuguese (Brazil)"),
        "expected the real language instruction to appear in the prompt when pt-br is requested"
    );
}

#[test]
fn b2b_content_defaults_to_english_for_an_unrecognized_language_code() {
    let args = CallB2bClaudeArguments {
        platform: "b2brazil",
        company_name: "Test Co",
        year_established: "2013",
        platform_verified: false,
        employee_count: "0-10",
        company_description: "Test description",
        contact_name: "",
        contact_phone: "",
        website_url: "",
        product_title: "Test",
        product_description: "Test",
        image_urls: &[],
        language: "xx-unknown",
        unit_price: "",
        minimum_order_quantity: "",
        payment_type: "",
    };
    let prompt = b2b_content(&args);
    assert!(
        prompt.contains("English"),
        "expected an unrecognized language code to safely fall back to English"
    );
}

// Shared, minimal set of otherwise-required arguments, so each test
// below only has to override the one field it's actually about.
fn base_args<'a>(company_description: &'a str) -> CallB2bClaudeArguments<'a> {
    CallB2bClaudeArguments {
        platform: "b2brazil",
        company_name: "Acme Corp",
        year_established: "2010",
        platform_verified: true,
        employee_count: "51-100",
        company_description,
        contact_name: "",
        contact_phone: "",
        website_url: "",
        product_title: "Industrial Widgets",
        product_description: "Bulk widgets for export.",
        image_urls: &[],
        language: "en",
        unit_price: "",
        minimum_order_quantity: "",
        payment_type: "",
    }
}

#[test]
fn b2b_content_shows_not_provided_when_company_description_is_genuinely_empty() {
    let content = b2b_content(&base_args(""));
    assert!(
        content.contains("Company description: Not provided"),
        "expected the real 'Not provided' fallback when no description was scraped, got:\n{}",
        content
    );
}

#[test]
fn b2b_content_includes_the_real_company_description_when_present() {
    let content = b2b_content(&base_args(
        "Founded in 1998, specializing in industrial equipment.",
    ));
    assert!(
        content.contains(
            "Company description: Founded in 1998, specializing in industrial equipment."
        ),
        "expected the real, actual scraped description to appear verbatim in the prompt, got:\n{}",
        content
    );
}

#[test]
fn b2b_content_never_shows_not_provided_when_a_real_description_exists() {
    // Guards against the empty-check being backwards (e.g. checking
    // is_some() on a &str instead of is_empty()).
    let content = b2b_content(&base_args("A real, non-empty description."));
    assert!(
        !content.contains("Company description: Not provided"),
        "expected the real description to replace the fallback text entirely, got:\n{}",
        content
    );
}

fn base_args_with_contact<'a>(
    contact_name: &'a str,
    contact_phone: &'a str,
    website_url: &'a str,
) -> CallB2bClaudeArguments<'a> {
    CallB2bClaudeArguments {
        platform: "kompass",
        company_name: "Acme Corp",
        year_established: "2010",
        platform_verified: true,
        employee_count: "51-100",
        company_description: "A real supplier.",
        contact_name,
        contact_phone,
        website_url,
        product_title: "Industrial Widgets",
        product_description: "Bulk widgets for export.",
        image_urls: &[],
        language: "en",
        unit_price: "",
        minimum_order_quantity: "",
        payment_type: "",
    }
}

#[test]
fn b2b_content_includes_the_real_contact_phone_when_present() {
    let content = b2b_content(&base_args_with_contact("", "+919588101303", ""));
    assert!(
        content.contains("+919588101303"),
        "expected the real, scraped contact phone to appear verbatim in the prompt, got:\n{}",
        content
    );
}

#[test]
fn b2b_content_includes_the_real_contact_name_and_website_when_present() {
    let content = base_args_with_contact("Mr. Yannick Koch", "", "https://beko-technologies.com");
    let prompt = b2b_content(&content);
    assert!(prompt.contains("Mr. Yannick Koch"));
    assert!(prompt.contains("https://beko-technologies.com"));
}

#[test]
fn b2b_content_shows_not_provided_for_genuinely_missing_contact_fields() {
    let content = b2b_content(&base_args_with_contact("", "", ""));
    assert!(content.contains("Contact name: Not provided"));
    assert!(content.contains("Contact phone: Not provided"));
    assert!(content.contains("Website: Not provided"));
}
