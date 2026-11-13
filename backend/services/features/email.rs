use crate::{
    errors::auth::AuthError,
    services::{features::auth::validate_email_format, features::templates::get_tera},
};
use reqwest::{Client, StatusCode};
use serde_json::json;
use std::env::var;
use tera::Context;

/// It sends the "click here to sign in" magic-link email to whoever's trying to log in,
/// and hands back the real status code from Resend once it's done.
///
/// Reads three settings from .env, validates both email addresses, sender and recipient,
/// builds the actual email content, packages everything into the request Resend expects,
/// actually sends it to Resend, checks if Resend actually accepted it. At the end,
/// it hands back the real status code.
pub async fn send_magic_link_email(
    to_email: &str,
    verify_url: &str,
) -> Result<StatusCode, AuthError> {
    let api_key = var("RESEND_API_KEY").map_err(|_| {
        AuthError::InternalServerError("RESEND_API_KEY needs to be setup".to_string())
    })?;

    let base_url = var("PUBLIC_BASE_URL").map_err(|_| {
        AuthError::InternalServerError("PUBLIC_BASE_URL needs to be configured".to_string())
    })?;

    let from_email =
        var("RESEND_FROM_EMAIL").unwrap_or_else(|_| "onboarding@resend.dev".to_string());
    let from_validated =
        validate_email_format(&from_email).expect("needs to have a formatted email address");
    let to_validated = validate_email_format(to_email).expect("needs to have a formatted address");

    let tera = get_tera();
    let client = Client::new();
    let mut context = Context::new();
    let logo_url = format!("{}/images/white_logo.png", base_url);

    context.insert("verify_url", verify_url);
    context.insert("logo_url", &logo_url);

    let html_body = tera
        .render("email.html", &context)
        .expect("tera needs to render the html file");

    let body = json!({
        "from": format!("Safely <{}>", from_validated),
        "to": [to_validated],
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
        let text = response
            .text()
            .await
            .unwrap_or_else(|_| "could not read the response text".to_string());

        return Err(AuthError::InternalServerError(format!(
            "Resend error ({}): {}",
            status, text
        )));
    }

    Ok(response.status())
}

/// It sends someone a welcome email, exactly once, the very first time they genuinely sign up.
///
/// Reads the required settings, builds several image and link addresses needed for the email,
/// fills in the welcome email template with all four values, packages the email,
/// sends it to Resend, checks if Resend actually accepted it and report success or
/// Resend's exact rejection reason.
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

/// Sent exactly once, the moment a subscription payment first fails
/// (subscription.past_due). Access stays active during this window -
/// Creem is still silently retrying the same card in the background -
/// this email is purely to prompt the person to update their payment
/// method before it's genuinely too late.
pub async fn send_payment_failed_email(to_email: &str, portal_url: &str) -> Result<(), AuthError> {
    let api_key = var("RESEND_API_KEY").map_err(|_| {
        AuthError::InternalServerError("RESEND_API_KEY needs to be setup".to_string())
    })?;

    let base_url = var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let logo_url = format!("{}/images/white_logo.png", base_url);
    let from_address =
        var("RESEND_FROM_EMAIL").unwrap_or_else(|_| "onboarding@resend.dev".to_string());

    let tera = get_tera();
    let client = Client::new();
    let mut context = Context::new();

    context.insert("portal_url", portal_url);
    context.insert("logo_url", &logo_url);

    let html_body = tera
        .render("payment_failed_email.html", &context)
        .expect("tera needs to render the payment failed email");

    let body = json!({
        "from": format!("Safely <{}>", from_address),
        "to": [to_email],
        "subject": "Payment issue with your Safely subscription",
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

/// Sent exactly once, the moment a subscription is genuinely and
/// finally canceled (subscription.canceled) - meaning Creem has
/// already exhausted its retries. This is the point access actually
/// gets revoked, paired with this one final, different message.
pub async fn send_subscription_ended_email(to_email: &str) -> Result<(), AuthError> {
    let api_key = var("RESEND_API_KEY").map_err(|_| {
        AuthError::InternalServerError("RESEND_API_KEY needs to be setup".to_string())
    })?;

    let base_url = var("PUBLIC_BASE_URL").map_err(|_| {
        AuthError::InternalServerError("PUBLIC_BASE_URL needs to be configured".to_string())
    })?;

    let from_address =
        var("RESEND_FROM_EMAIL").unwrap_or_else(|_| "onboarding@resend.dev".to_string());

    let dashboard_url = format!("{}/dashboard/", base_url);
    let logo_url = format!("{}/images/white_logo.png", base_url);
    let tera = get_tera();
    let client = Client::new();
    let mut context = Context::new();

    context.insert("dashboard_url", &dashboard_url);
    context.insert("logo_url", &logo_url);

    let html_body = tera
        .render("subscription_ended_email.html", &context)
        .expect("tera needs to render the subscription ended email");

    let body = json!({
        "from": format!("Safely <{}>", from_address),
        "to": [to_email],
        "subject": "Your Safely subscription has ended",
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

/// Sent when the person cancels through our own dashboard - genuinely
/// different from send_subscription_ended_email, which is specifically
/// for Creem exhausting its automatic payment retries. Same webhook
/// event fires either way, so this distinction can only be made by our
/// own code, at the moment we know for certain which case we're in.
pub async fn send_subscription_canceled_email(to_email: &str) -> Result<(), AuthError> {
    let client = Client::new();
    let mut context = Context::new();

    let api_key = var("RESEND_API_KEY").map_err(|_| {
        AuthError::InternalServerError("RESEND_API_KEY needs to be setup".to_string())
    })?;

    let base_url = var("PUBLIC_BASE_URL").map_err(|_| {
        AuthError::InternalServerError("PUBLIC_BASE_URL needs to be configured".to_string())
    })?;

    let from_address =
        var("RESEND_FROM_EMAIL").unwrap_or_else(|_| "onboarding@resend.dev".to_string());
    let dashboard_url = format!("{}/dashboard/", base_url);
    let logo_url = format!("{}/images/white_logo.png", base_url);
    let tera = get_tera();

    context.insert("dashboard_url", &dashboard_url);
    context.insert("logo_url", &logo_url);

    let html_body = tera
        .render("subscription_canceled_email.html", &context)
        .expect("tera needs to render the subscription canceled email");

    let body = json!({
        "from": format!("Safely <{}>", from_address),
        "to": [to_email],
        "subject": "Your Safely subscription has been canceled",
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

/// Tells someone they were added to a Safely team (team.rs invite).
/// They use the owner's plan once they sign in with this email.
pub async fn send_team_invite_email(
    to_email: &str,
    owner_email: &str,
    plan: &str,
) -> Result<(), AuthError> {
    let api_key = var("RESEND_API_KEY").map_err(|_| {
        AuthError::InternalServerError("RESEND_API_KEY needs to be setup".to_string())
    })?;

    let base_url = var("PUBLIC_BASE_URL").map_err(|_| {
        AuthError::InternalServerError("PUBLIC_BASE_URL needs to be configured".to_string())
    })?;

    let from_address =
        var("RESEND_FROM_EMAIL").unwrap_or_else(|_| "onboarding@resend.dev".to_string());
    let dashboard_url = format!("{}/dashboard/", base_url);
    let logo_url = format!("{}/images/white_logo.png", base_url);
    let tera = get_tera();
    let mut context = Context::new();

    context.insert("dashboard_url", &dashboard_url);
    context.insert("logo_url", &logo_url);
    context.insert("owner_email", owner_email);
    context.insert("member_email", to_email);
    context.insert("plan", plan);

    let html_body = tera
        .render("team_invite_email.html", &context)
        .map_err(|e| AuthError::InternalServerError(e.to_string()))?;

    let body = json!({
        "from": format!("Safely <{}>", from_address),
        "to": [to_email],
        "subject": format!("{} added you to their Safely team", owner_email),
        "html": html_body,
    });

    let response = Client::new()
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

/// Subscribes an email address to your Kit newsletter form. Kit's
/// subscribe endpoint is deliberately permissive - it succeeds even for
/// most malformed input, so the real validation happens on our own
/// side first, matching the same pattern already used for magic links.
pub async fn subscribe_to_newsletter(email: &str) -> Result<(), AuthError> {
    let api_key = var("KIT_API_KEY")
        .map_err(|_| AuthError::InternalServerError("KIT_API_KEY needs to be setup".to_string()))?;

    let client = Client::new();
    let body = json!({
        "api_key": api_key,
        "email": email,
    });

    let response = client
        .post("https://api.convertkit.com/v3/forms/9638009/subscribe")
        .json(&body)
        .send()
        .await
        .map_err(|e| AuthError::InternalServerError(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response
            .text()
            .await
            .unwrap_or_else(|_| "could not read the response text".to_string());
        return Err(AuthError::InternalServerError(format!(
            "Kit error ({}): {}",
            status, text
        )));
    }

    Ok(())
}

/// Tells someone that a supplier they watch got worse (watch.rs): a new
/// scam report, a sanctions list, a closed company, a website down...
pub async fn send_watch_alert_email(
    to_email: &str,
    supplier: &str,
    platform: &str,
    analysis_id: Option<uuid::Uuid>,
    changes: &[crate::services::features::watch::WatchChange],
) -> Result<(), AuthError> {
    let api_key = var("RESEND_API_KEY").map_err(|_| {
        AuthError::InternalServerError("RESEND_API_KEY needs to be setup".to_string())
    })?;
    let base_url = var("PUBLIC_BASE_URL").map_err(|_| {
        AuthError::InternalServerError("PUBLIC_BASE_URL needs to be configured".to_string())
    })?;
    let from_address =
        var("RESEND_FROM_EMAIL").unwrap_or_else(|_| "onboarding@resend.dev".to_string());

    let watch_url = format!("{}/watch", base_url);
    // The scan's own page when there is one, else the watch list.
    let open_url = match analysis_id {
        Some(id) => format!(
            "{}/dashboard/{}/{}",
            base_url,
            if platform.is_empty() {
                "scan"
            } else {
                platform
            },
            id
        ),
        None => watch_url.clone(),
    };
    let platform_name = crate::services::features::signals::platform_display_name(platform);
    let logo_url = format!("{}/images/white_logo.png", base_url);
    let tera = get_tera();
    let mut context = Context::new();
    context.insert("supplier", supplier);
    context.insert("platform", &platform_name);
    context.insert("changes", changes);
    context.insert("open_url", &open_url);
    context.insert("watch_url", &watch_url);
    context.insert("logo_url", &logo_url);

    let html_body = tera
        .render("watch_alert_email.html", &context)
        .map_err(|e| AuthError::InternalServerError(e.to_string()))?;

    let first = changes
        .first()
        .map(|c| c.title.as_str())
        .unwrap_or("Changed");
    let body = json!({
        "from": format!("Safely <{}>", from_address),
        "to": [to_email],
        "subject": format!("{}: {}", supplier, first),
        "html": html_body,
    });

    let response = Client::new()
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
