use crate::models::users::{GoogleTokenResponse, GoogleUserInfo};
use std::env;

pub fn build_google_authorize_url(state: &str) -> Result<String, String> {
    let client_id =
        env::var("GOOGLE_CLIENT_ID").map_err(|_| "GOOGLE_CLIENT_ID not set".to_string())?;
    let redirect_uri =
        env::var("GOOGLE_REDIRECT_URI").map_err(|_| "GOOGLE_REDIRECT_URI not set".to_string())?;

    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?\
         client_id={}&redirect_uri={}&response_type=code&\
         scope=openid%20email%20profile&state={}&prompt=select_account",
        urlencoding::encode(&client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(state),
    );

    Ok(url)
}

/// Exchanges an authorization code for an access token, then fetches the
/// user's profile. Two plain HTTP calls — no OAuth crate needed for this.
pub async fn exchange_code_for_user(code: &str) -> Result<GoogleUserInfo, String> {
    let client_id =
        env::var("GOOGLE_CLIENT_ID").map_err(|_| "GOOGLE_CLIENT_ID not set".to_string())?;
    let client_secret =
        env::var("GOOGLE_CLIENT_SECRET").map_err(|_| "GOOGLE_CLIENT_SECRET not set".to_string())?;
    let redirect_uri =
        env::var("GOOGLE_REDIRECT_URI").map_err(|_| "GOOGLE_REDIRECT_URI not set".to_string())?;

    let client = reqwest::Client::new();

    let form_body = format!(
        "code={}&client_id={}&client_secret={}&redirect_uri={}&grant_type=authorization_code",
        urlencoding::encode(code),
        urlencoding::encode(&client_id),
        urlencoding::encode(&client_secret),
        urlencoding::encode(&redirect_uri),
    );

    let token_res = client
        .post("https://oauth2.googleapis.com/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !token_res.status().is_success() {
        let text = token_res.text().await.unwrap_or_default();
        return Err(format!("Google token exchange failed: {}", text));
    }

    let token_data: GoogleTokenResponse = token_res.json().await.map_err(|e| e.to_string())?;

    let userinfo_res = client
        .get("https://www.googleapis.com/oauth2/v3/userinfo")
        .bearer_auth(&token_data.access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !userinfo_res.status().is_success() {
        let text = userinfo_res.text().await.unwrap_or_default();
        return Err(format!("Google userinfo fetch failed: {}", text));
    }

    userinfo_res
        .json::<GoogleUserInfo>()
        .await
        .map_err(|e| e.to_string())
}
