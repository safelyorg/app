use crate::models::users::{
    AuthSuccessResponse, GoogleCallbackQuery, MagicLinkRequest, VerifyQuery,
};
use crate::services::{auth, email, google_oauth};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

const DASHBOARD_PATH: &str = "/dashboard/";
const OAUTH_STATE_COOKIE: &str = "oauth_state";

/// POST /api/v1/auth/magic-link
/// Sends a sign-in email. Always returns success (even for unknown emails)
/// so this endpoint can't be used to check which emails have accounts.
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
        // Log but don't leak email-sending failures to the client - same
        // reasoning as not revealing whether the email exists.
        eprintln!("Failed to send magic link email: {}", e);
    }

    Ok(Json(AuthSuccessResponse {
        success: true,
        message: "If that email is valid, a sign-in link is on its way.".to_string(),
    }))
}

/// GET /api/v1/auth/verify?token=...
/// This is the link the person clicks from their email client - a real
/// browser navigation, not a fetch call - so it responds with a redirect.
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

    let session_token = match auth::create_session(&pool, user.id).await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("create_session error: {}", e);
            return Redirect::to(&format!("{}?error=server_error", DASHBOARD_PATH)).into_response();
        }
    };

    // Session token goes in the URL fragment (#), not a query string, so it
    // never gets logged by the server or shows up in browser history search.
    // The dashboard's JS reads it from location.hash on load, saves it to
    // localStorage, then strips the hash from the visible URL.
    Redirect::to(&format!("{}#session={}", DASHBOARD_PATH, session_token)).into_response()
}

/// GET /api/v1/auth/google
/// Redirects the browser to Google's consent screen. Sets a short-lived
/// state cookie so the callback can confirm the response actually came
/// from a request we initiated (basic CSRF protection).
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

/// GET /api/v1/auth/google/callback
pub async fn google_callback(
    State(pool): State<Pool<Postgres>>,
    jar: CookieJar,
    Query(query): Query<GoogleCallbackQuery>,
) -> impl IntoResponse {
    if query.error.is_some() {
        return Redirect::to(&format!("{}?error=google_denied", DASHBOARD_PATH));
    }

    let Some(code) = query.code else {
        return Redirect::to(&format!("{}?error=missing_code", DASHBOARD_PATH));
    };

    let expected_state = jar.get(OAUTH_STATE_COOKIE).map(|c| c.value().to_string());
    if expected_state.is_none() || expected_state != query.state {
        return Redirect::to(&format!("{}?error=state_mismatch", DASHBOARD_PATH));
    }

    let google_user = match google_oauth::exchange_code_for_user(&code).await {
        Ok(u) => u,
        Err(e) => {
            eprintln!("exchange_code_for_user error: {}", e);
            return Redirect::to(&format!("{}?error=google_exchange_failed", DASHBOARD_PATH));
        }
    };

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
            return Redirect::to(&format!("{}?error=server_error", DASHBOARD_PATH));
        }
    };

    let _ = auth::touch_last_login(&pool, user.id).await;

    let session_token = match auth::create_session(&pool, user.id).await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("create_session error: {}", e);
            return Redirect::to(&format!("{}?error=server_error", DASHBOARD_PATH));
        }
    };

    Redirect::to(&format!("{}#session={}", DASHBOARD_PATH, session_token))
}
