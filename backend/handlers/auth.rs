use crate::{
    errors::auth::AuthError,
    models::users::{
        GoogleAfterLoginQuery, GoogleConnectQuery, GoogleUserInfoEndpoint, MagicLinkAuthResponse,
        MagicLinkRequest, VerifyMagicLinkToken,
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
use serde_json::{Value, json};
use sqlx::{Pool, Postgres};
use std::env::var;
use time::Duration;
use uuid::Uuid;

const DASHBOARD_PATH: &str = "/dashboard/";
pub const OAUTH_STATE_COOKIE: &str = "oauth_state";
pub const OAUTH_LINK_USER_COOKIE: &str = "oauth_link_user_id";

/// POST /api/v1/auth/magic-link
///
/// This function validates the email address, insert the magic link
/// token, build the clickable link, try to send it without breaking anything
/// and always responds the same way, regardless of whether the email exists or not
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
///
/// It confirms the clicked link is genuinly real and hasn't expired or
/// been used before. It finds or creates their account, complete the sign-in
/// process and send them straight to the dashboard.
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

/// GET /api/v1/auth/google
///
/// The moment someone clicks "Sign in with Google",
/// it prepares a secure trip to Google's real sign-in page,
/// leaving itself a cookie to check when they come back.
///
/// It creates a random, one-time security code, builds the actual
/// Google sign-in web address, checks if this is genuinely running in
/// production, builds a cookie to remember that random security code,
/// adds the cookie to the person's browser and sends their browser
/// off to Google, carrying that cookie along.
pub async fn google_redirect(jar: CookieJar) -> Result<(CookieJar, Redirect), AuthError> {
    let random_code = Uuid::new_v4().to_string();
    let check_code = random_code.trim();

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
///
/// This runs the moment someone, already logged into their Safely account,
/// clicks "Connect" next to Google in Settings. It double-checks
/// they're really logged in, then sends them off to Google, carrying along
/// a cookie about who they are, so Google's response can later be correctly
/// linked back to their specific account.
///
/// It builds a normal login header from the session token in the URL,
/// confirms this is genuinely a real, currently valid session, creates
/// a random, one-time security code, builds the real Google sign-in address,
/// checks if this is genuinely production, builds the first cookie - the random security code
/// builds a second cookie — genuinely new here — recording who is asking,
/// attaches both cookies to the browser, and sends them to Google.
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
///
/// When someone clicks "Log out" — it deletes their real login session from the database,
/// so their old token genuinely stops working, not just appears logged out on their screen.
///
/// It checks if a real login header was actually sent, pulls the actual token out
/// of that header, actually deletes that session from the database, and always
/// reports success, regardless of what actually happened
pub async fn logout(State(pool): State<Pool<Postgres>>, headers: HeaderMap) -> Json<Value> {
    if let Some(auth_header) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            let _ = delete_session(&pool, token).await;
        }
    }

    Json(json!({ "success": true }))
}

/// GET /api/v1/auth/google/callback
///
/// The moment Google sends someone back to your site, after they've signed in on
/// Google's own page. It carefully verifies everything is legitimate, then decides
/// whether this is a brand-new sign-in or connecting Google to an already-logged-in account.
///
/// It checks that did Google itself report an error or Google actually
/// send back the expected authorization code? It also checks whether
/// the returned security code actually match what was sent? It exchanges the
/// code for real Google account details. It checks fresh sign-in, or connecting
/// to an existing account and upon success, sends it to the dashboard.
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
