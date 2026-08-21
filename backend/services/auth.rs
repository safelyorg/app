use crate::{
    errors::auth::{AuthError, AuthServiceError},
    models::users::{MagicLink, User},
    services::email::send_welcome_email,
};
use axum::http::HeaderMap;
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
