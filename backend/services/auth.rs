use crate::models::users::{MagicLink, User};
use axum::http::HeaderMap;
use chrono::{DateTime, Duration, Utc};
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
    let row = sqlx::query(
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

    // Sliding expiration: an actively-used session keeps pushing its own
    // expiry forward, so someone who checks the dashboard every few days
    // effectively never gets logged out - only genuine inactivity for
    // the full window causes a real expiry, matching how most everyday
    // consumer apps behave rather than a hard wall from the moment of
    // login. To avoid writing to the database on every single request
    // (a page load can easily fire five or six authenticated calls at
    // once), this only re-extends once the session has already burned
    // through at least 5 of its 30 days - a regular user still triggers
    // this roughly once every few days of real use, not on every click.
    let refresh_threshold = Utc::now() + Duration::days(25);
    if expires_at < refresh_threshold {
        let _ = sqlx::query(
            "UPDATE sessions SET expires_at = NOW() + INTERVAL '30 days' WHERE token = $1",
        )
        .bind(token)
        .execute(pool)
        .await;
    }

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

// ============================================================
// Per-user rate limiting for expensive endpoints (currently just
// /analyze, since that's the one that costs real Claude API money per
// call). Requiring login already stops anonymous abuse; this stops a
// single signed-in account - by accident (a stuck script, a retry
// loop) or on purpose - from calling it far more often than any real
// person actually would.
//
// Kept in plain memory rather than the database or Redis: this is a
// single-server deployment, so an in-process map is both simpler and
// faster than a network round-trip for something checked on every
// request. If this ever runs across multiple server instances, this
// would need to move to something shared (Redis is the usual choice)
// since each instance would otherwise track its own separate counts.
// ============================================================
static RATE_LIMITS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<Uuid, (u32, std::time::Instant)>>,
> = std::sync::OnceLock::new();

const RATE_LIMIT_WINDOW: std::time::Duration = std::time::Duration::from_secs(300);
const RATE_LIMIT_MAX_REQUESTS: u32 = 10;

/// Returns true if this user is still within their allowed request
/// count for the current window, and counts this call toward it.
/// Returns false if they've already hit the limit - the caller decides
/// what to do with that (currently: reject with 429 Too Many Requests).
pub fn check_rate_limit(user_id: Uuid) -> bool {
    let map = RATE_LIMITS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    let mut map = map.lock().unwrap();
    let now = std::time::Instant::now();

    let entry = map.entry(user_id).or_insert((0, now));

    if now.duration_since(entry.1) > RATE_LIMIT_WINDOW {
        entry.0 = 0;
        entry.1 = now;
    }

    entry.0 += 1;
    entry.0 <= RATE_LIMIT_MAX_REQUESTS
}

pub async fn link_google_account(
    pool: &Pool<Postgres>,
    user_id: Uuid,
    google_id: &str,
) -> Result<(), Error> {
    sqlx::query("UPDATE users SET google_id = $1 WHERE id = $2")
        .bind(google_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn unlink_google_account(pool: &Pool<Postgres>, user_id: Uuid) -> Result<(), Error> {
    sqlx::query("UPDATE users SET google_id = NULL WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Permanently deletes an account and everything that belongs solely to
/// it - sessions, magic links, the account row itself. Data that
/// represents shared community value (fraud reports, listing analyses -
/// other users still benefit from knowing "this seller has N fraud
/// reports" regardless of who filed them) is anonymized rather than
/// deleted: user_id is set to NULL so the record survives, disconnected
/// from this person's identity, instead of quietly weakening fraud
/// protection for everyone else the moment one person closes their
/// account. Everything happens in one transaction - either all of it
/// succeeds, or none of it does, so an account can never end up
/// half-deleted.
pub async fn delete_user_account(pool: &Pool<Postgres>, user_id: Uuid) -> Result<(), Error> {
    let mut tx = pool.begin().await?;

    let email: String = sqlx::query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;

    sqlx::query("UPDATE analysis SET user_id = NULL WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("UPDATE fraud_reports SET user_id = NULL WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    // magic_links has no user_id column at all (it only ever existed as
    // an email + token pair before any account was necessarily created)
    // so cleanup here matches by email instead.
    sqlx::query("DELETE FROM magic_links WHERE email = $1")
        .bind(&email)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}
