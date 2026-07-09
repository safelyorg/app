use crate::models::users::{
    AuthSuccessResponse, GoogleCallbackQuery, MagicLinkRequest, VerifyQuery,
};
use crate::services::{auth, email, google_oauth};
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

const DASHBOARD_PATH: &str = "/dashboard/";
const OAUTH_STATE_COOKIE: &str = "oauth_state";
const OAUTH_LINK_USER_COOKIE: &str = "oauth_link_user_id";

/// POST /api/v1/auth/magic-link
pub async fn request_magic_link(
    State(pool): State<Pool<Postgres>>,
    Json(req): Json<MagicLinkRequest>,
) -> Result<Json<AuthSuccessResponse>, (StatusCode, String)> {
    let email_trimmed = req.email.trim().to_lowercase();
    if email_trimmed.is_empty() || !email_trimmed.contains('@') {
        return Err((StatusCode::BAD_REQUEST, "Invalid email address".to_string()));
    }

    let token = auth::create_magic_link(&pool, &email_trimmed)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let base_url =
        std::env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let verify_url = format!("{}/api/v1/auth/verify?token={}", base_url, token);

    if let Err(e) = email::send_magic_link_email(&email_trimmed, &verify_url).await {
        eprintln!("Failed to send magic link email: {}", e);
    }

    Ok(Json(AuthSuccessResponse {
        success: true,
        message: "If that email is valid, a sign-in link is on its way.".to_string(),
    }))
}

/// GET /api/v1/auth/verify?token=...
pub async fn verify_magic_link(
    State(pool): State<Pool<Postgres>>,
    Query(query): Query<VerifyQuery>,
) -> impl IntoResponse {
    let link = match auth::verify_magic_link(&pool, &query.token).await {
        Ok(Some(l)) => l,
        Ok(None) => {
            return Redirect::to(&format!("{}?error=expired_link", DASHBOARD_PATH)).into_response();
        }
        Err(e) => {
            eprintln!("verify_magic_link db error: {}", e);
            return Redirect::to(&format!("{}?error=server_error", DASHBOARD_PATH)).into_response();
        }
    };

    let user = match auth::find_or_create_user_by_email(&pool, &link.email).await {
        Ok(u) => u,
        Err(e) => {
            eprintln!("find_or_create_user_by_email error: {}", e);
            return Redirect::to(&format!("{}?error=server_error", DASHBOARD_PATH)).into_response();
        }
    };

    let _ = auth::touch_last_login(&pool, user.id).await;
    let _ = auth::set_login_method(&pool, user.id, "email").await;

    let session_token = match auth::create_session(&pool, user.id).await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("create_session error: {}", e);
            return Redirect::to(&format!("{}?error=server_error", DASHBOARD_PATH)).into_response();
        }
    };

    Redirect::to(&format!("{}#session={}", DASHBOARD_PATH, session_token)).into_response()
}

/// GET /api/v1/auth/google
/// Fresh sign-in via Google - creates a session at the end, same as
/// magic link does. Not to be confused with google_connect_redirect
/// below, which links Google onto an account that's already logged in.
pub async fn google_redirect(jar: CookieJar) -> impl IntoResponse {
    let state = Uuid::new_v4().to_string();

    let authorize_url = match google_oauth::build_google_authorize_url(&state) {
        Ok(url) => url,
        Err(e) => {
            eprintln!("build_google_authorize_url error: {}", e);
            return (
                jar,
                Redirect::to(&format!("{}?error=server_error", DASHBOARD_PATH)),
            )
                .into_response();
        }
    };

    let mut cookie = Cookie::new(OAUTH_STATE_COOKIE, state);
    cookie.set_path("/");
    cookie.set_max_age(time::Duration::minutes(10));
    cookie.set_http_only(true);

    let jar = jar.add(cookie);
    (jar, Redirect::to(&authorize_url)).into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct GoogleConnectQuery {
    pub session: String,
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
) -> impl IntoResponse {
    let mut synthetic_headers = HeaderMap::new();
    if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", query.session)) {
        synthetic_headers.insert(header::AUTHORIZATION, val);
    }

    let user_id = match auth::extract_user_id(&synthetic_headers, &pool).await {
        Some(id) => id,
        None => {
            return (
                jar,
                Redirect::to(&format!("{}?error=session_expired", DASHBOARD_PATH)),
            )
                .into_response();
        }
    };

    let state = Uuid::new_v4().to_string();
    let authorize_url = match google_oauth::build_google_authorize_url(&state) {
        Ok(url) => url,
        Err(e) => {
            eprintln!("build_google_authorize_url error: {}", e);
            return (
                jar,
                Redirect::to(&format!("{}?error=server_error", DASHBOARD_PATH)),
            )
                .into_response();
        }
    };

    let mut state_cookie = Cookie::new(OAUTH_STATE_COOKIE, state);
    state_cookie.set_path("/");
    state_cookie.set_max_age(time::Duration::minutes(10));
    state_cookie.set_http_only(true);

    let mut link_cookie = Cookie::new(OAUTH_LINK_USER_COOKIE, user_id.to_string());
    link_cookie.set_path("/");
    link_cookie.set_max_age(time::Duration::minutes(10));
    link_cookie.set_http_only(true);

    let jar = jar.add(state_cookie).add(link_cookie);
    (jar, Redirect::to(&authorize_url)).into_response()
}

/// POST /api/v1/auth/logout
/// Deletes the session row on the server, not just the token sitting in
/// the browser's localStorage - without this, logging out only ever
/// removed Safely's own memory of being logged in, while the token
/// itself stayed valid indefinitely if it had ever leaked (XSS, a
/// compromised device, a copied localStorage value). Always reports
/// success regardless of whether a matching session was actually found,
/// since the person's intent (be logged out) is satisfied either way.
pub async fn logout(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    if let Some(auth_header) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            let _ = auth::delete_session(&pool, token).await;
        }
    }
    Json(serde_json::json!({ "success": true }))
}

/// GET /api/v1/auth/google/callback
pub async fn google_callback(
    State(pool): State<Pool<Postgres>>,
    jar: CookieJar,
    Query(query): Query<GoogleCallbackQuery>,
) -> impl IntoResponse {
    // Every return path in this function ends with .into_response() so
    // they all share one concrete type (Response<Body>) - `impl
    // IntoResponse` in a return position still requires every branch to
    // return the SAME underlying type, it isn't a trait object that can
    // hold different concrete types across branches.
    if query.error.is_some() {
        return Redirect::to(&format!("{}?error=google_denied", DASHBOARD_PATH)).into_response();
    }

    let Some(code) = query.code else {
        return Redirect::to(&format!("{}?error=missing_code", DASHBOARD_PATH)).into_response();
    };

    let expected_state = jar.get(OAUTH_STATE_COOKIE).map(|c| c.value().to_string());
    if expected_state.is_none() || expected_state != query.state {
        return Redirect::to(&format!("{}?error=state_mismatch", DASHBOARD_PATH)).into_response();
    }

    let google_user = match google_oauth::exchange_code_for_user(&code).await {
        Ok(u) => u,
        Err(e) => {
            eprintln!("exchange_code_for_user error: {}", e);
            return Redirect::to(&format!("{}?error=google_exchange_failed", DASHBOARD_PATH))
                .into_response();
        }
    };

    // If the link-user cookie is present, this is a "connect Google to my
    // already-logged-in account" flow (started by google_connect_redirect
    // above) - handled entirely separately from fresh sign-in below,
    // since it must attach Google to THIS SPECIFIC account rather than
    // whichever account happens to match the Google email.
    //
    // The cookie is removed the INSTANT it's read, before any further
    // logic runs, on every single exit path below - regardless of
    // success or failure. Without this, the cookie would linger in the
    // browser for its full lifetime and incorrectly hijack a completely
    // separate, later login attempt as if it were another "connect"
    // request - which never issues a session token, since it assumes
    // you're already logged in. That's exactly what caused a normal
    // Google sign-in to loop back to the login screen instead of
    // reaching the dashboard.
    if let Some(link_cookie) = jar.get(OAUTH_LINK_USER_COOKIE).cloned() {
        let mut removed_state = Cookie::from(OAUTH_STATE_COOKIE);
        removed_state.set_path("/");
        let mut removed_link = Cookie::from(OAUTH_LINK_USER_COOKIE);
        removed_link.set_path("/");
        let jar = jar.remove(removed_state).remove(removed_link);

        let linking_user_id = match Uuid::parse_str(link_cookie.value()) {
            Ok(id) => id,
            Err(_) => {
                return (
                    jar,
                    Redirect::to(&format!("{}?error=server_error", DASHBOARD_PATH)),
                )
                    .into_response();
            }
        };

        // The Google account being connected must use the SAME email as
        // the account already logged in - otherwise this would silently
        // attach an unrelated Google identity to this account, creating
        // exactly the kind of email mismatch this check exists to
        // prevent. Comparison is case-insensitive since email addresses
        // are conventionally treated that way.
        let linking_user = match auth::find_user_by_id(&pool, linking_user_id).await {
            Ok(Some(u)) => u,
            Ok(None) => {
                return (
                    jar,
                    Redirect::to(&format!("{}?error=server_error", DASHBOARD_PATH)),
                )
                    .into_response();
            }
            Err(e) => {
                eprintln!("find_user_by_id error: {}", e);
                return (
                    jar,
                    Redirect::to(&format!("{}?error=server_error", DASHBOARD_PATH)),
                )
                    .into_response();
            }
        };

        if linking_user.email.trim().to_lowercase() != google_user.email.trim().to_lowercase() {
            return (
                jar,
                Redirect::to(&format!("{}?error=google_email_mismatch", DASHBOARD_PATH)),
            )
                .into_response();
        }

        // Refuse to link a Google account already tied to a DIFFERENT
        // Safely account - each Google account can only ever be linked
        // to one Safely account at a time.
        match auth::find_user_by_google_id(&pool, &google_user.sub).await {
            Ok(Some(existing)) if existing.id != linking_user_id => {
                return (
                    jar,
                    Redirect::to(&format!("{}?error=google_already_linked", DASHBOARD_PATH)),
                )
                    .into_response();
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("find_user_by_google_id error: {}", e);
                return (
                    jar,
                    Redirect::to(&format!("{}?error=server_error", DASHBOARD_PATH)),
                )
                    .into_response();
            }
        }

        if let Err(e) = auth::link_google_account(&pool, linking_user_id, &google_user.sub).await {
            eprintln!("link_google_account error: {}", e);
            return (
                jar,
                Redirect::to(&format!("{}?error=server_error", DASHBOARD_PATH)),
            )
                .into_response();
        }

        return (
            jar,
            Redirect::to(&format!("{}?google_connected=1", DASHBOARD_PATH)),
        )
            .into_response();
    }

    let user = match auth::find_or_create_user_by_google(
        &pool,
        &google_user.sub,
        &google_user.email,
        google_user.name.as_deref(),
    )
    .await
    {
        Ok(u) => u,
        Err(e) => {
            eprintln!("find_or_create_user_by_google error: {}", e);
            return Redirect::to(&format!("{}?error=server_error", DASHBOARD_PATH)).into_response();
        }
    };

    let _ = auth::touch_last_login(&pool, user.id).await;
    let _ = auth::set_login_method(&pool, user.id, "google").await;

    let session_token = match auth::create_session(&pool, user.id).await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("create_session error: {}", e);
            return Redirect::to(&format!("{}?error=server_error", DASHBOARD_PATH)).into_response();
        }
    };

    Redirect::to(&format!("{}#session={}", DASHBOARD_PATH, session_token)).into_response()
}
