use crate::errors::auth::AuthError;
use reqwest::Client;
use serde_json::json;
use std::{env::var, sync::OnceLock};
use tera::{Context, Tera};

static TERA: OnceLock<Tera> = OnceLock::new();
const EMAIL_TEMPLATE: &str = include_str!("../templates/email.html");
const WELCOME_EMAIL_TEMPLATE: &str = include_str!("../templates/welcome_email.html");

fn get_tera() -> &'static Tera {
    TERA.get_or_init(|| {
        let mut tera = Tera::default();
        tera.add_raw_template("email.html", EMAIL_TEMPLATE)
            .expect("tera should add the sign-in raw template");
        tera.add_raw_template("welcome_email.html", WELCOME_EMAIL_TEMPLATE)
            .expect("tera should add the welcome raw template");
        tera
    })
}

pub async fn send_magic_link_email(to_email: &str, verify_url: &str) -> Result<(), AuthError> {
    let api_key = var("RESEND_API_KEY").map_err(|_| {
        AuthError::InternalServerError("RESEND_API_KEY needs to be setup".to_string())
    })?;
    let base_url = var("PUBLIC_BASE_URL").map_err(|_| {
        AuthError::InternalServerError("PUBLIC_BASE_URL needs to be configured".to_string())
    })?;
    let from_address =
        var("RESEND_FROM_EMAIL").unwrap_or_else(|_| "onboarding@resend.dev".to_string());

    let client = Client::new();
    let mut context = Context::new();
    let logo_url = format!("{}/images/white_logo.png", base_url);
    let tera = get_tera();

    context.insert("verify_url", verify_url);
    context.insert("logo_url", &logo_url);

    let html_body = tera
        .render("email.html", &context)
        .expect("tera needs to render the html file");

    let body = json!({
        "from": format!("Safely <{}>", from_address),
        "to": [to_email],
        "subject": "Sign in to Safely",
        "html": html_body,
    });

    let response = client
        .post("https://api.resend.com/emails")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| AuthError::InternalServerError(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(AuthError::InternalServerError(format!(
            "Resend error ({}): {}",
            status, text
        )));
    }
    Ok(())
}

/// Sent exactly once per account - only when find_or_create_user_by_email
/// or find_or_create_user_by_google reports a genuine, brand-new signup,
/// never on an ordinary returning login. A failure here is deliberately
/// non-fatal to the caller (logged, not propagated) - a missing welcome
/// email should never block someone's actual sign-in from completing.
pub async fn send_welcome_email(to_email: &str) -> Result<(), AuthError> {
    let api_key = var("RESEND_API_KEY").map_err(|_| {
        AuthError::InternalServerError("RESEND_API_KEY needs to be setup".to_string())
    })?;
    let base_url = var("PUBLIC_BASE_URL").map_err(|_| {
        AuthError::InternalServerError("PUBLIC_BASE_URL needs to be configured".to_string())
    })?;
    let from_address =
        var("RESEND_FROM_EMAIL").unwrap_or_else(|_| "onboarding@resend.dev".to_string());

    let logo_url = format!("{}/images/white_logo.png", base_url);
    let linkedin_icon_url = format!("{}/images/linkedin_logo.png", base_url);
    let x_icon_url = format!("{}/images/x_logo.png", base_url);
    let dashboard_url = format!("{}/dashboard/", base_url);

    let tera = get_tera();
    let client = Client::new();
    let mut context = Context::new();

    context.insert("logo_url", &logo_url);
    context.insert("linkedin_icon_url", &linkedin_icon_url);
    context.insert("x_icon_url", &x_icon_url);
    context.insert("dashboard_url", &dashboard_url);

    let html_body = tera
        .render("welcome_email.html", &context)
        .expect("tera needs to render the welcome html file");

    let body = json!({
        "from": format!("Safely <{}>", from_address),
        "to": [to_email],
        "subject": "Welcome to Safely",
        "html": html_body,
    });

    let response = client
        .post("https://api.resend.com/emails")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| AuthError::InternalServerError(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(AuthError::InternalServerError(format!(
            "Resend error ({}): {}",
            status, text
        )));
    }
    Ok(())
}
