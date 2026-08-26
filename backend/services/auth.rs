use crate::{
    errors::auth::{AuthError, AuthServiceError},
    handlers::auth::{DASHBOARD_PATH, OAUTH_LINK_USER_COOKIE, OAUTH_STATE_COOKIE},
    models::users::{GoogleUserInfoEndpoint, MagicLink, User},
    services::{
        email::send_welcome_email,
        google_oauth::{find_user_by_google_id, link_google_account},
    },
};
use axum::{http::HeaderMap, response::Redirect};
use axum_extra::extract::{CookieJar, cookie::Cookie};
use chrono::{DateTime, Duration, Utc};
use sqlx::{Error, Pool, Postgres, Row, query, query_as};
use uuid::Uuid;

/// It cleans up an email address and checks it at least looks like a real one.
///
/// Cleans up the email, checks if it's genuinely valid email and returns the cleaned-up version.
pub fn validate_email_format(email: &str) -> Result<String, AuthError> {
    let trimmed = email.trim().to_lowercase();
    if trimmed.is_empty() || !trimmed.contains('@') {
        return Err(AuthError::BadRequest);
    }

    Ok(trimmed)
}

/// It creates a real, one-time login token, and saves it to the database, tied to a specific email.
///
/// I generates the pieces needed for a new row, validates the email to see if there's a real bug,
/// inserts the row into the database and returns the token.
pub async fn insert_magic_link(
    pool: &Pool<Postgres>,
    email: &str,
) -> Result<String, AuthServiceError> {
    let id = Uuid::now_v7();
    let token = Uuid::new_v4().to_string();
    let expires_at = Utc::now() + Duration::minutes(15);
    let validated_email =
        validate_email_format(email).map_err(|_| AuthServiceError::InvalidEmail)?;

    query("INSERT INTO magic_links (id, email, token, expires_at) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind(validated_email)
        .bind(&token)
        .bind(expires_at)
        .execute(pool)
        .await?;

    Ok(token)
}

/// It checks if a magic link token is genuinely real, unexpired, and hasn't been used before
/// and if it is, immediately marks it as used, so it can never be used a second time.
///
/// It looks up the token, with three conditions all required at once. If a valid link
/// was genuinely found, immediately marks it as used and returns whatever was found or wasn't
pub async fn validate_magic_link(
    pool: &Pool<Postgres>,
    token: &str,
) -> Result<Option<MagicLink>, Error> {
    let link = query_as::<_, MagicLink>(
        "SELECT * FROM magic_links WHERE token = $1 AND used_at IS NULL AND expires_at > NOW() LIMIT 1",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;

    if let Some(ref l) = link {
        query("UPDATE magic_links SET used_at = NOW() WHERE id = $1")
            .bind(l.id)
            .execute(pool)
            .await?;
    }

    Ok(link)
}

/// It finds someone's existing account by their email, or creates a brand-new one if they've never
/// signed up before and tells the caller which of those two things just happened.
///
/// It prepares an ID, just in case a new user needs to be created, checks if a user with
/// this email already exists. If no existing user was found, create a brand-new one and
/// returns the new user, with true.
pub async fn find_or_create_user_by_email(
    pool: &Pool<Postgres>,
    email: &str,
) -> Result<(User, bool), AuthServiceError> {
    let id = Uuid::now_v7();

    if let Some(user) = find_user_by_email(pool, email).await? {
        return Ok((user, false));
    }
    let user = query_as::<_, User>("INSERT INTO users (id, email) VALUES ($1, $2) RETURNING *")
        .bind(id)
        .bind(email)
        .fetch_one(pool)
        .await?;

    Ok((user, true))
}

/// It cleans and validates an email address, then looks up a user matching it — returning them if found,
/// or nothing if not, and correctly failing if the email itself was invalid.
///
/// Validates and cleans the email — now correctly handling failure, looks up the user,
/// and returns the result directly
pub async fn find_user_by_email(
    pool: &Pool<Postgres>,
    email: &str,
) -> Result<Option<User>, AuthServiceError> {
    let formatted_email =
        validate_email_format(email).map_err(|_| AuthServiceError::InvalidEmail)?;

    let result = query_as::<_, User>("SELECT * FROM users WHERE email = $1 LIMIT 1")
        .bind(formatted_email)
        .fetch_optional(pool)
        .await?;

    Ok(result)
}

/// It runs at the final, shared step of both sign-in methods (magic link and Google).
/// It handles the welcome email if they're brand new, updates some login bookkeeping,
/// and creates their real session.
///
/// It sends a welcome email — but only for genuinely new people, updates
/// "last login" bookkeeping, records how they logged in this time,
/// creates the real session — the one part that genuinely matters otherwise
/// the whole sign-in fails.
pub async fn finish_sign_in(
    pool: &Pool<Postgres>,
    user_id: Uuid,
    is_new: bool,
    email_for_welcome: &str,
    login_method: &str,
) -> Result<String, AuthError> {
    if is_new {
        if let Err(e) = send_welcome_email(email_for_welcome).await {
            eprintln!("Failed to send welcome email: {:?}", e);
        }
    }
    let _ = check_last_login(pool, user_id).await;
    let _ = set_login_method(pool, user_id, login_method).await;

    create_session(pool, user_id).await.map_err(|e| {
        eprintln!("create_session error: {}", e);
        AuthError::DashboardPath("server_error".to_string())
    })
}

/// It records the current moment as this user's most recent login time.
///
/// Updates the user's row using the query and returns success, with nothing meaningful inside it
pub async fn check_last_login(pool: &Pool<Postgres>, user_id: Uuid) -> Result<(), Error> {
    query("UPDATE users SET last_login_at = NOW() WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// It records which method someone just used to sign in — either "email" or "google".
///
/// Updates the user's row using the query and returns success,
/// with nothing meaningful inside it
pub async fn set_login_method(
    pool: &Pool<Postgres>,
    user_id: Uuid,
    method: &str,
) -> Result<(), Error> {
    query("UPDATE users SET last_login_method = $1 WHERE id = $2")
        .bind(method)
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// It creates a genuine, real login session for a user, valid for 30 days,
/// and hands back the actual token they'll use to prove they're logged in.
///
/// It generates an ID for this session row, builds the actual session token,
/// sets when this session expires, saves the session to the database and
/// returns the real token.
pub async fn create_session(pool: &Pool<Postgres>, user_id: Uuid) -> Result<String, Error> {
    let id = Uuid::now_v7();
    let token = format!("{}{}", Uuid::new_v4(), Uuid::new_v4()).replace('-', "");
    let expires_at = Utc::now() + Duration::days(30);

    query("INSERT INTO sessions (id, user_id, token, expires_at) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind(user_id)
        .bind(&token)
        .bind(expires_at)
        .execute(pool)
        .await?;

    Ok(token)
}

/// It checks if a request includes a genuine, valid login token,
/// and hands back the real user's ID if so — quietly returning "no one"
/// for a missing or malformed token, but now genuinely reporting a real
/// error if something actually breaks, like the database.
///
/// It checks if the Authorization header exists and is readable,
/// checks if it's correctly formatted as a Bearer token, looks up the real
/// user this token belongs to — and this is where it's genuinely different
/// from before and converts the found user (if any) into just their ID.
pub async fn extract_user_id(
    headers: &HeaderMap,
    pool: &Pool<Postgres>,
) -> Result<Option<Uuid>, Error> {
    let Some(auth_header) = headers.get("authorization").and_then(|v| v.to_str().ok()) else {
        return Ok(None);
    };
    let Some(token) = auth_header.strip_prefix("Bearer ") else {
        return Ok(None);
    };
    let user = get_user_from_token(pool, token).await?;

    Ok(user.map(|u| u.id))
}

/// It looks up who a session token belongs to, and quietly extends that session's
/// expiration if it's getting close to running out — so an actively-used account
/// never gets logged out just from the passage of time.
///
/// It looks up the session, but only if it hasn't already expired. If nothing
/// was found, quietly return "no one" — not an error. Pulls the two real values
/// out of the found row and finally deciding whether to extend the session.
pub async fn get_user_from_token(
    pool: &Pool<Postgres>,
    token: &str,
) -> Result<Option<User>, Error> {
    let row = query(
        "SELECT user_id, expires_at FROM sessions WHERE token = $1 AND expires_at > NOW() LIMIT 1",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let user_id: Uuid = row.get("user_id");
    let expires_at: DateTime<Utc> = row.get("expires_at");

    let refresh_threshold = Utc::now() + Duration::days(25);
    if expires_at < refresh_threshold {
        let _ =
            query("UPDATE sessions SET expires_at = NOW() + INTERVAL '30 days' WHERE token = $1")
                .bind(token)
                .execute(pool)
                .await;
    }

    find_user_by_id(pool, user_id).await
}

/// It looks up a user by their exact ID, and returns them if
/// found or nothing, if no such user exists.
///
/// It runs the actual lookup query and returns whatever was found.
pub async fn find_user_by_id(pool: &Pool<Postgres>, id: Uuid) -> Result<Option<User>, Error> {
    let result = query_as::<_, User>("SELECT * FROM users WHERE id = $1 LIMIT 1")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    Ok(result)
}

/// It runs when someone who's already logged into Safely finishes
/// connecting their Google account and it makes sure Google gets
/// attached to their specific account.
///
/// It cleans up two temporary cookies that were no longer needed,
/// reads which account this connection request belongs to, confirms
/// that the account genuinly exists.
///
/// First checks that the emails actually match and the secondly
/// checks if the Google account already claimed by someone else.
pub async fn handle_google_connect(
    pool: &Pool<Postgres>,
    jar: CookieJar,
    link_cookie: Cookie<'static>,
    google_user: &GoogleUserInfoEndpoint,
) -> Result<(CookieJar, Redirect), AuthError> {
    let mut removed_state = Cookie::from(OAUTH_STATE_COOKIE);
    let mut removed_link = Cookie::from(OAUTH_LINK_USER_COOKIE);
    removed_state.set_path("/");
    removed_link.set_path("/");
    let jar = jar.remove(removed_state).remove(removed_link);

    let linking_user_id = Uuid::parse_str(link_cookie.value())
        .map_err(|_| AuthError::DashboardPathWithJar(jar.clone(), "server_error".to_string()))?;

    let linking_user = find_user_by_id(pool, linking_user_id)
        .await
        .map_err(|e| {
            eprintln!("find_user_by_id error: {}", e);
            AuthError::DashboardPath("server_error".to_string())
        })?
        .ok_or_else(|| AuthError::DashboardPath("server_error".to_string()))?;

    if linking_user.email.trim().to_lowercase() != google_user.email.trim().to_lowercase() {
        return Err(AuthError::DashboardPathWithJar(
            jar,
            "google_email_mismatch".to_string(),
        ));
    }

    let existing_user = find_user_by_google_id(pool, &google_user.id)
        .await
        .map_err(|e| {
            eprintln!("find_user_by_google_id error: {}", e);
            AuthError::DashboardPathWithJar(jar.clone(), "server_error".to_string())
        })?;

    if let Some(existing) = existing_user {
        if existing.id != linking_user_id {
            return Err(AuthError::DashboardPathWithJar(
                jar,
                "google_already_linked".to_string(),
            ));
        }
    }

    if let Err(e) = link_google_account(pool, linking_user_id, &google_user.id).await {
        eprintln!("link_google_account error: {}", e);
        return Err(AuthError::DashboardPathWithJar(
            jar,
            "server_error".to_string(),
        ));
    }

    Ok((
        jar,
        Redirect::to(&format!("{}?google_connected=1", DASHBOARD_PATH)),
    ))
}

/// It deletes a specific session from the database, using its token to find it.
///
/// It runs the deletion query and returns success, with nothing meaningful inside it
pub async fn delete_session(pool: &Pool<Postgres>, token: &str) -> Result<(), Error> {
    query("DELETE FROM sessions WHERE token = $1")
        .bind(token)
        .execute(pool)
        .await?;

    Ok(())
}
