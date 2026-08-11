use crate::{
    errors::auth::AuthError,
    models::users::{MagicLink, User},
};
use axum::http::HeaderMap;
use chrono::{DateTime, Duration, Utc};
use sqlx::{Error, Pool, Postgres, Row, query, query_as, query_scalar};
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
pub async fn insert_magic_link(pool: &Pool<Postgres>, email: &str) -> Result<String, Error> {
    let id = Uuid::now_v7();
    let token = Uuid::new_v4().to_string();
    let expires_at = Utc::now() + Duration::minutes(15);
    let validated_email = validate_email_format(email).map_err(|_| Error::RowNotFound)?;

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
) -> Result<(User, bool), Error> {
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
pub async fn find_user_by_email(pool: &Pool<Postgres>, email: &str) -> Result<Option<User>, Error> {
    let formatted_email = validate_email_format(email).map_err(|_| Error::RowNotFound)?;

    query_as::<_, User>("SELECT * FROM users WHERE email = $1 LIMIT 1")
        .bind(formatted_email)
        .fetch_optional(pool)
        .await
}

/// It checks if a request includes a genuine, valid login token, and if so,
/// hands back the real user's ID — but if anything at all is missing or invalid,
/// it just quietly says "no one," rather than rejecting the request.
///
/// It looks for the Authorization header, pulls the actual token out of the header,
/// looks up the real user this token belongs to and if everything succeeded,
/// return the real user's ID.
pub async fn extract_user_id(headers: &HeaderMap, pool: &Pool<Postgres>) -> Option<Uuid> {
    let auth_header = headers.get("authorization")?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ")?;
    let user = get_user_from_token(pool, token).await.ok()??;
    Some(user.id)
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
) -> Result<(User, bool), Error> {
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

/// Resolves a bearer token from the Authorization header into a User, if
/// the session exists and hasn't expired. Returns None (not an error) for
/// any invalid/missing/expired token so callers can treat the request as
/// anonymous rather than failing it outright.
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
        let _ =
            query("UPDATE sessions SET expires_at = NOW() + INTERVAL '30 days' WHERE token = $1")
                .bind(token)
                .execute(pool)
                .await;
    }

    find_user_by_id(pool, user_id).await
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

/// Ok(()) if this user is still within their allowed request count for
/// the current window - this also counts the call toward it. Err(secs)
/// if they've already hit the limit, where secs is exactly how much
/// longer they need to wait - calculated here, once, from the real
/// window start time, rather than the caller (or the browser) having
/// to guess at it separately.
pub fn check_rate_limit(user_id: Uuid) -> Result<(), u64> {
    let map = RATE_LIMITS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    let mut map = map.lock().unwrap();
    let now = std::time::Instant::now();

    let entry = map.entry(user_id).or_insert((0, now));

    if now.duration_since(entry.1) > RATE_LIMIT_WINDOW {
        entry.0 = 0;
        entry.1 = now;
    }

    entry.0 += 1;

    if entry.0 <= RATE_LIMIT_MAX_REQUESTS {
        Ok(())
    } else {
        let elapsed = now.duration_since(entry.1);
        let remaining = RATE_LIMIT_WINDOW.saturating_sub(elapsed);
        Err(remaining.as_secs())
    }
}

pub async fn unlink_google_account(pool: &Pool<Postgres>, user_id: Uuid) -> Result<(), Error> {
    query("UPDATE users SET google_id = NULL WHERE id = $1")
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

    let email: String = query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;

    query("UPDATE analysis SET user_id = NULL WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    query("UPDATE fraud_reports SET user_id = NULL WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    query("DELETE FROM sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    // magic_links has no user_id column at all (it only ever existed as
    // an email + token pair before any account was necessarily created)
    // so cleanup here matches by email instead.
    query("DELETE FROM magic_links WHERE email = $1")
        .bind(&email)
        .execute(&mut *tx)
        .await?;

    query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}
