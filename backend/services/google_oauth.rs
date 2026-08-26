use crate::{
    errors::{auth::AuthServiceError, google_oauth::GoogleOauthConfigError},
    models::users::{GoogleTokenEndpoint, GoogleUserInfoEndpoint, User},
    services::auth::find_user_by_email,
};
use sqlx::{Error, Pool, Postgres, query, query_as};
use std::env::var;
use urlencoding::encode;
use uuid::Uuid;

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

/// It looks up a user by their linked Google account ID,
/// and returns them if found or nothing, if no user has that Google ID connected.
///
/// It runs the lookup query, and returns its result directly
pub async fn find_user_by_google_id(
    pool: &Pool<Postgres>,
    google_id: &str,
) -> Result<Option<User>, Error> {
    let result = query_as::<_, User>("SELECT * FROM users WHERE google_id = $1 LIMIT 1")
        .bind(google_id)
        .fetch_optional(pool)
        .await?;

    Ok(result)
}

/// It actually attaches a Google account to a specific existing Safely user,
/// by writing their Google ID into that user's row.
///
/// It runs the update and returns success, with nothing meaningful inside it.
pub async fn link_google_account(
    pool: &Pool<Postgres>,
    user_id: Uuid,
    google_id: &str,
) -> Result<(), Error> {
    query("UPDATE users SET google_id = $1 WHERE id = $2")
        .bind(google_id)
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// It figures out who is someone when they sign in with Google, using three checks in order
/// checking if this exact Google account is already known,
/// then checking if their email matches an existing account,
/// and only creating a brand-new account if neither of those find anyone.
///
/// First check — has this exact Google account been seen before?
/// Second check — does their email match an existing account, even if Google's never
/// been connected before? Neither check found anyone then it genuinely created a brand-new account.
pub async fn find_or_create_user_by_google(
    pool: &Pool<Postgres>,
    google_id: &str,
    email: &str,
    name: Option<&str>,
) -> Result<(User, bool), AuthServiceError> {
    if let Some(user) = find_user_by_google_id(pool, google_id).await? {
        return Ok((user, false));
    }

    if let Some(existing) = find_user_by_email(pool, email).await? {
        let user = query_as::<_, User>(
            "UPDATE users SET google_id = $1, name = COALESCE(name, $2) WHERE id = $3 RETURNING *",
        )
        .bind(google_id)
        .bind(name)
        .bind(existing.id)
        .fetch_one(pool)
        .await?;
        return Ok((user, false));
    }

    let id = Uuid::now_v7();
    let user = query_as::<_, User>(
        "INSERT INTO users (id, email, google_id, name) VALUES ($1, $2, $3, $4) RETURNING *",
    )
    .bind(id)
    .bind(email)
    .bind(google_id)
    .bind(name)
    .fetch_one(pool)
    .await?;

    Ok((user, true))
}
