#[derive(Debug)]
pub struct SellerIdentifiers {
    pub name: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub website: Option<String>,
}

#[derive(Debug)]
pub struct OsintMatch {
    pub matched_identifiers: Vec<String>,
    pub confidence: String, // "strong", "weak", "none"
}

pub fn score_identifier_match(seller: &SellerIdentifiers, found_text: &str) -> OsintMatch {
    let lower_text = found_text.to_lowercase();
    let mut matched = Vec::new();

    if let Some(name) = &seller.name {
        if !name.trim().is_empty() && lower_text.contains(&name.to_lowercase()) {
            matched.push("name".to_string());
        }
    }
    if let Some(phone) = &seller.phone {
        let digits_only: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
        if !digits_only.is_empty()
            && lower_text
                .replace(|c: char| !c.is_ascii_digit(), "")
                .contains(&digits_only)
        {
            matched.push("phone".to_string());
        }
    }
    if let Some(email) = &seller.email {
        if !email.trim().is_empty() && lower_text.contains(&email.to_lowercase()) {
            matched.push("email".to_string());
        }
    }
    if let Some(website) = &seller.website {
        if !website.trim().is_empty() && lower_text.contains(&website.to_lowercase()) {
            matched.push("website".to_string());
        }
    }

    let confidence = if matched.len() >= 2 {
        "strong"
    } else if matched.len() == 1 {
        "weak"
    } else {
        "none"
    };

    OsintMatch {
        matched_identifiers: matched,
        confidence: confidence.to_string(),
    }
}
