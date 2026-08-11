use crate::{
    errors::google_oauth::GoogleOauthConfigError,
    models::users::{GoogleTokenEndpoint, GoogleUserInfoEndpoint},
};
use std::env::var;
use urlencoding::encode;

/// It builds the actual, complete web address that sends someone to Google's real sign-in page.
///
/// It reads your app's registered Google identity, reads where Google should send someone back to,
/// builds the actual, complete Google web address and returns the finished URL
pub fn build_google_authorize_url(state: &str) -> Result<String, GoogleOauthConfigError> {
    let client_id = var("GOOGLE_CLIENT_ID").map_err(|_| GoogleOauthConfigError::GoogleClientId)?;
    let redirect_uri =
        var("GOOGLE_REDIRECT_URI").map_err(|_| GoogleOauthConfigError::GoogleRedirectUri)?;

    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?\
         client_id={}&redirect_uri={}&response_type=code&\
         scope=openid%20email%20profile&state={}&prompt=select_account",
        encode(&client_id),
        encode(&redirect_uri),
        encode(state),
    );

    Ok(url)
}

/// It takes the one-time authorization code Google gave you, trades it with Google
/// for a real access token, then uses that token to actually ask Google "who is this person?",
/// getting back their real email, name, and Google ID.
///
/// It gathers the settings needed to talk to Google, sends Google the sign-in code to trade
/// for a real access pass, checks that Google actually gave it back correctly, then uses
/// that access pass to ask Google directly "who is this person," checks that answer came back
/// correctly too, and finally hands back their real name, email, and Google ID.
pub async fn exchange_code_for_user(
    code: &str,
) -> Result<GoogleUserInfoEndpoint, GoogleOauthConfigError> {
    let client_id = var("GOOGLE_CLIENT_ID").map_err(|_| GoogleOauthConfigError::GoogleClientId)?;
    let client_secret =
        var("GOOGLE_CLIENT_SECRET").map_err(|_| GoogleOauthConfigError::GoogleClientSecret)?;
    let redirect_uri =
        var("GOOGLE_REDIRECT_URI").map_err(|_| GoogleOauthConfigError::GoogleRedirectUri)?;
    let client = reqwest::Client::new();

    let form_body = format!(
        "code={}&client_id={}&client_secret={}&redirect_uri={}&grant_type=authorization_code",
        encode(code),
        encode(&client_id),
        encode(&client_secret),
        encode(&redirect_uri),
    );

    let token_res = client
        .post("https://oauth2.googleapis.com/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_body)
        .send()
        .await
        .map_err(|e| GoogleOauthConfigError::TokenRequestFailed(e.to_string()))?;

    if !token_res.status().is_success() {
        let text = token_res.text().await.unwrap_or_default();
        return Err(GoogleOauthConfigError::TokenExchangeFailed(text));
    }

    let token_data: GoogleTokenEndpoint = token_res
        .json()
        .await
        .map_err(|e| GoogleOauthConfigError::TokenResponseParseFailed(e.to_string()))?;

    let userinfo_res = client
        .get("https://www.googleapis.com/oauth2/v3/userinfo")
        .bearer_auth(&token_data.access_token)
        .send()
        .await
        .map_err(|e| GoogleOauthConfigError::UserInfoRequestFailed(e.to_string()))?;

    if !userinfo_res.status().is_success() {
        let text = userinfo_res.text().await.unwrap_or_default();
        return Err(GoogleOauthConfigError::UserInfoFetchFailed(text));
    }

    userinfo_res
        .json::<GoogleUserInfoEndpoint>()
        .await
        .map_err(|e| GoogleOauthConfigError::UserInfoResponseParseFailed(e.to_string()))
}
