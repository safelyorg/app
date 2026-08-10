use crate::{
    errors::auth::AuthError,
    models::users::{
        GoogleAfterLoginQuery, GoogleUserInfoEndpoint, MagicLinkAuthResponse, MagicLinkRequest,
        VerifyMagicLinkToken,
    },
    services::{
        auth::{
            check_last_login, create_session, delete_session, extract_user_id,
            find_or_create_user_by_email, find_or_create_user_by_google, find_user_by_google_id,
            find_user_by_id, insert_magic_link, link_google_account, set_login_method,
            validate_email_format, validate_magic_link,
        },
        email::{send_magic_link_email, send_welcome_email},
        google_oauth::{build_google_authorize_url, exchange_code_for_user},
    },
};
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, header},
    response::Redirect,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Pool, Postgres};
use std::env::var;
use time::Duration;
use uuid::Uuid;

const DASHBOARD_PATH: &str = "/dashboard/";
pub const OAUTH_STATE_COOKIE: &str = "oauth_state";
pub const OAUTH_LINK_USER_COOKIE: &str = "oauth_link_user_id";

#[derive(Debug, Deserialize)]
pub struct GoogleConnectQuery {
    pub session: String,
}

/// POST /api/v1/auth/magic-link
pub async fn request_magic_link(
    State(pool): State<Pool<Postgres>>,
    Json(req): Json<MagicLinkRequest>,
) -> Result<Json<MagicLinkAuthResponse>, AuthError> {
    let email_trimmed = validate_email_format(&req.email)?;
    let token = insert_magic_link(&pool, &email_trimmed)
        .await
        .map_err(|_| {
            AuthError::InternalServerError(
                "magic link needs to be inserted in the database".to_string(),
            )
        })?;

    let base_url = var("PUBLIC_BASE_URL").map_err(|_| {
        AuthError::InternalServerError("PUBLIC_BASE_URL needs to be configured".to_string())
    })?;

    let verify_url = format!("{}/api/v1/auth/verify?token={}", base_url, token);
    if let Err(e) = send_magic_link_email(&email_trimmed, &verify_url).await {
        eprintln!("Error: failed to send a magic link email: {:?}", e);
    }

    Ok(Json(MagicLinkAuthResponse {
        success: true,
        message: "If that email is valid, a sign-in link is on its way.".to_string(),
    }))
}

/// GET /api/v1/auth/verify?token=...
pub async fn verify_magic_link(
    State(pool): State<Pool<Postgres>>,
    Query(query): Query<VerifyMagicLinkToken>,
) -> Result<Redirect, AuthError> {
    let validate = validate_magic_link(&pool, &query.token)
        .await
        .map_err(|e| {
            eprintln!("verify_magic_link db error: {}", e);
            AuthError::DashboardPath("server_error".to_string())
        })?
        .ok_or_else(|| AuthError::DashboardPath("expired_link".to_string()))?;

    let (user, is_new) = find_or_create_user_by_email(&pool, &validate.email)
        .await
        .map_err(|e| {
            eprintln!("find_or_create_user_by_email error: {}", e);
            AuthError::DashboardPath("server_error".to_string())
        })?;
    let session_token = finish_sign_in(&pool, user.id, is_new, &validate.email, "email").await?;

    Ok(Redirect::to(&format!(
        "{}#session={}",
        DASHBOARD_PATH, session_token
    )))
}

/// Part A - Google is being connected onto an account that's ALREADY
/// logged in (started by google_connect_redirect). Handled completely
/// separately from a fresh sign-in, since it must attach Google to
/// THIS SPECIFIC account, not whichever account happens to share the
/// Google email.
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

/// GET /api/v1/auth/google
/// Fresh sign-in via Google - creates a session at the end, same as
/// magic link does. Not to be confused with google_connect_redirect
/// below, which links Google onto an account that's already logged in.
pub async fn google_redirect(jar: CookieJar) -> Result<(CookieJar, Redirect), AuthError> {
    let random_code = Uuid::new_v4().to_string();
    let check_code = random_code.trim();

    // create google sign-in web address
    let authorize_url = build_google_authorize_url(check_code).map_err(|e| {
        eprintln!("build_google_authorize_url error: {}", e);
        AuthError::DashboardPath("server_error".to_string())
    })?;

    let is_production = var("PUBLIC_BASE_URL")
        .map(|url| url.starts_with("https://"))
        .unwrap_or(false);

    let mut cookie = Cookie::new(OAUTH_STATE_COOKIE, check_code.to_string());
    // make it available across all the site
    cookie.set_path("/");
    cookie.set_max_age(Duration::minutes(10));
    // only my server should read it
    cookie.set_http_only(true);
    // stops this cookie from ever traveling over an unencrypted connection
    cookie.set_secure(is_production);
    // prevent the cookie to turn against you
    cookie.set_same_site(SameSite::Lax);
    let jar = jar.add(cookie);

    Ok((jar, Redirect::to(&authorize_url)))
}

/// GET /api/v1/auth/google/connect?session=<token>
/// Starts the "connect Google to my already-logged-in account" flow.
/// This is a plain top-level page navigation triggered by clicking
/// "Connect" in Settings - it can't carry the Bearer token as a header
/// the way a fetch() call would, so the token travels as a query
/// parameter instead. It's verified here exactly like any other
/// authenticated request, just via a different transport.
pub async fn google_connect_redirect(
    State(pool): State<Pool<Postgres>>,
    Query(query): Query<GoogleConnectQuery>,
    jar: CookieJar,
) -> Result<(CookieJar, Redirect), AuthError> {
    let mut synthetic_headers = HeaderMap::new();
    if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", query.session)) {
        synthetic_headers.insert(header::AUTHORIZATION, val);
    }

    let user_id = extract_user_id(&synthetic_headers, &pool)
        .await
        .ok_or_else(|| AuthError::DashboardPath("session_expired".to_string()))?;

    let state = Uuid::new_v4().to_string();
    let authorize_url = build_google_authorize_url(&state).map_err(|e| {
        eprintln!("build_google_authorize_url error: {}", e);
        AuthError::DashboardPath("server_error".to_string())
    })?;

    let is_production = var("PUBLIC_BASE_URL")
        .map(|url| url.starts_with("https://"))
        .unwrap_or(false);

    let mut state_cookie = Cookie::new(OAUTH_STATE_COOKIE, state);
    state_cookie.set_path("/");
    state_cookie.set_max_age(Duration::minutes(10));
    state_cookie.set_http_only(true);
    state_cookie.set_secure(is_production);
    state_cookie.set_same_site(SameSite::Lax);

    let mut link_cookie = Cookie::new(OAUTH_LINK_USER_COOKIE, user_id.to_string());
    link_cookie.set_path("/");
    link_cookie.set_max_age(Duration::minutes(10));
    link_cookie.set_http_only(true);
    link_cookie.set_secure(is_production);
    link_cookie.set_same_site(SameSite::Lax);

    let jar = jar.add(state_cookie).add(link_cookie);
    Ok((jar, Redirect::to(&authorize_url)))
}

/// POST /api/v1/auth/logout
/// Deletes the session row on the server, not just the token sitting in
/// the browser's localStorage - without this, logging out only ever
/// removed Safely's own memory of being logged in, while the token
/// itself stayed valid indefinitely if it had ever leaked (XSS, a
/// compromised device, a copied localStorage value). Always reports
/// success regardless of whether a matching session was actually found,
/// since the person's intent (be logged out) is satisfied either way.
pub async fn logout(State(pool): State<Pool<Postgres>>, headers: HeaderMap) -> Json<Value> {
    if let Some(auth_header) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            let _ = delete_session(&pool, token).await;
        }
    }
    Json(json!({ "success": true }))
}

/// Shared final step for both sign-in flows (magic link, and fresh
/// Google sign-in). Once we already have a real User - whether
/// brand-new or existing - both flows do the exact same remaining
/// work: send a welcome email if genuinely new, update login
/// bookkeeping, and create a real session.
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

/// GET /api/v1/auth/google/callback
pub async fn google_callback(
    State(pool): State<Pool<Postgres>>,
    jar: CookieJar,
    Query(query): Query<GoogleAfterLoginQuery>,
) -> Result<(CookieJar, Redirect), AuthError> {
    if query.error.is_some() {
        return Err(AuthError::DashboardPath("google_denied".to_string()));
    }
    let Some(code) = query.code else {
        return Err(AuthError::DashboardPath("missing_code".to_string()));
    };
    let expected_state = jar.get(OAUTH_STATE_COOKIE).map(|c| c.value().to_string());
    if expected_state.is_none() || expected_state != query.state {
        return Err(AuthError::DashboardPath("state_mismatch".to_string()));
    }
    let google_user = exchange_code_for_user(&code).await.map_err(|e| {
        eprintln!("exchange_code_for_user error: {}", e);
        AuthError::DashboardPath("google_exchange_failed".to_string())
    })?;

    // Part A - connecting to an existing, already-logged-in account
    if let Some(link_cookie) = jar.get(OAUTH_LINK_USER_COOKIE).cloned() {
        return handle_google_connect(&pool, jar, link_cookie, &google_user).await;
    }

    // Part B - fresh sign-in, sharing the exact same finishing steps
    // that verify_magic_link also uses
    let (user, is_new) = find_or_create_user_by_google(
        &pool,
        &google_user.id,
        &google_user.email,
        google_user.name.as_deref(),
    )
    .await
    .map_err(|e| {
        eprintln!("find_or_create_user_by_google error: {}", e);
        AuthError::DashboardPath("server_error".to_string())
    })?;
    let session_token =
        finish_sign_in(&pool, user.id, is_new, &google_user.email, "google").await?;
    Ok((
        jar,
        Redirect::to(&format!("{}#session={}", DASHBOARD_PATH, session_token)),
    ))
}
