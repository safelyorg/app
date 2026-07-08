use crate::models::users::{MagicLink, User};
use axum::http::HeaderMap;
use chrono::{Duration, Utc};
use sqlx::{Error, Pool, Postgres, Row};
use uuid::Uuid;

pub async fn find_user_by_email(
    pool: &Pool<Postgres>,
    email: &str,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1 LIMIT 1")
        .bind(email)
        .fetch_optional(pool)
        .await
}

pub async fn find_user_by_google_id(
    pool: &Pool<Postgres>,
    google_id: &str,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE google_id = $1 LIMIT 1")
        .bind(google_id)
        .fetch_optional(pool)
        .await
}

pub async fn find_user_by_id(pool: &Pool<Postgres>, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1 LIMIT 1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Finds a user by email, or creates one if none exists.
/// Used by the magic-link flow, where the only identity we have is an email.
pub async fn find_or_create_user_by_email(
    pool: &Pool<Postgres>,
    email: &str,
) -> Result<User, sqlx::Error> {
    if let Some(user) = find_user_by_email(pool, email).await? {
        return Ok(user);
    }

    let id = Uuid::now_v7();
    sqlx::query_as::<_, User>("INSERT INTO users (id, email) VALUES ($1, $2) RETURNING *")
        .bind(id)
        .bind(email)
        .fetch_one(pool)
        .await
}

/// Finds a user by google_id, falling back to matching by email so a person
/// who first signed in via magic link and later uses Google ends up as the
/// same account rather than a duplicate.
pub async fn find_or_create_user_by_google(
    pool: &Pool<Postgres>,
    google_id: &str,
    email: &str,
    name: Option<&str>,
) -> Result<User, sqlx::Error> {
    if let Some(user) = find_user_by_google_id(pool, google_id).await? {
        return Ok(user);
    }

    if let Some(existing) = find_user_by_email(pool, email).await? {
        // link the google_id onto the existing email-based account
        return sqlx::query_as::<_, User>(
            "UPDATE users SET google_id = $1, name = COALESCE(name, $2) WHERE id = $3 RETURNING *",
        )
        .bind(google_id)
        .bind(name)
        .bind(existing.id)
        .fetch_one(pool)
        .await;
    }

    let id = Uuid::now_v7();
    sqlx::query_as::<_, User>(
        "INSERT INTO users (id, email, google_id, name) VALUES ($1, $2, $3, $4) RETURNING *",
    )
    .bind(id)
    .bind(email)
    .bind(google_id)
    .bind(name)
    .fetch_one(pool)
    .await
}

pub async fn touch_last_login(pool: &Pool<Postgres>, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET last_login_at = NOW() WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Creates a magic link record and returns the raw token to embed in the email URL.
pub async fn create_magic_link(pool: &Pool<Postgres>, email: &str) -> Result<String, sqlx::Error> {
    let id = Uuid::now_v7();
    let token = Uuid::new_v4().to_string();
    let expires_at = Utc::now() + Duration::minutes(15);

    sqlx::query("INSERT INTO magic_links (id, email, token, expires_at) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind(email)
        .bind(&token)
        .bind(expires_at)
        .execute(pool)
        .await?;

    Ok(token)
}

pub async fn set_login_method(
    pool: &Pool<Postgres>,
    user_id: Uuid,
    method: &str,
) -> Result<(), Error> {
    sqlx::query("UPDATE users SET last_login_method = $1 WHERE id = $2")
        .bind(method)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Validates a magic link token: must exist, be unused, and not expired.
/// Marks it used on success so it can never be replayed.
pub async fn verify_magic_link(
    pool: &Pool<Postgres>,
    token: &str,
) -> Result<Option<MagicLink>, sqlx::Error> {
    let link = sqlx::query_as::<_, MagicLink>(
        "SELECT * FROM magic_links WHERE token = $1 AND used_at IS NULL AND expires_at > NOW() LIMIT 1",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;

    if let Some(ref l) = link {
        sqlx::query("UPDATE magic_links SET used_at = NOW() WHERE id = $1")
            .bind(l.id)
            .execute(pool)
            .await?;
    }

    Ok(link)
}

/// Creates a bearer session token for a user, valid 30 days.
pub async fn create_session(pool: &Pool<Postgres>, user_id: Uuid) -> Result<String, sqlx::Error> {
    let id = Uuid::now_v7();
    let token = format!("{}{}", Uuid::new_v4(), Uuid::new_v4()).replace('-', "");
    let expires_at = Utc::now() + Duration::days(30);

    sqlx::query("INSERT INTO sessions (id, user_id, token, expires_at) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind(user_id)
        .bind(&token)
        .bind(expires_at)
        .execute(pool)
        .await?;

    Ok(token)
}

/// Resolves a bearer token from the Authorization header into a User, if
/// the session exists and hasn't expired. Returns None (not an error) for
/// any invalid/missing/expired token so callers can treat the request as
/// anonymous rather than failing it outright.
pub async fn get_user_from_token(
    pool: &Pool<Postgres>,
    token: &str,
) -> Result<Option<User>, sqlx::Error> {
    let row =
        sqlx::query("SELECT user_id FROM sessions WHERE token = $1 AND expires_at > NOW() LIMIT 1")
            .bind(token)
            .fetch_optional(pool)
            .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let user_id: Uuid = row.get("user_id");
    find_user_by_id(pool, user_id).await
}

pub async fn delete_session(pool: &Pool<Postgres>, token: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sessions WHERE token = $1")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}

/// Reads the Authorization header (if present) and resolves it to a user
/// ID. Returns None for any of: missing header, malformed header, or a
/// token that doesn't match a valid session - all treated the same way,
/// as "proceed anonymously" rather than reject the request. This is what
/// keeps every existing anonymous extension user working unchanged; being
/// logged in only ever adds a user_id, it never becomes a requirement.
pub async fn extract_user_id(headers: &HeaderMap, pool: &Pool<Postgres>) -> Option<Uuid> {
    let auth_header = headers.get("authorization")?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ")?;
    let user = get_user_from_token(pool, token).await.ok()??;
    Some(user.id)
}
