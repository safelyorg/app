//! The dashboard's own pages. All of them are the same dashboard/index.html,
//! which opens the page in the address (dashboard/ts/shared.ts), so a
//! refresh stays on the page and a link opens it:
//!
//! - /dashboard/                          History
//! - /reports                             My Reports
//! - /settings                            Settings
//! - /dashboard/<platform>/<scan id>      one scan's result
//!   (/dashboard/alibaba/0193..., /dashboard/website/0193...)
//!
//! The dashboard's own files (/dashboard/js/init.js...) are sent from
//! the same dashboard folder as before. Signed out, the page shows the
//! sign-in page; the data itself is only sent to a signed-in person
//! (routes/dashboard.rs).

use axum::{
    Router,
    extract::{Path, Request},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

/// The dashboard folder and its page.
const DASHBOARD_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../dashboard");
const DASHBOARD_PAGE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../dashboard/index.html");
/// The "page not found" page (the same one as in src/main.rs).
const NOT_FOUND_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../site/templates/404.html");

pub fn dashboard_pages_routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route(
            "/dashboard",
            get(|| async { Redirect::permanent("/dashboard/") }),
        )
        .route_service("/dashboard/", ServeFile::new(DASHBOARD_PAGE))
        .route_service("/reports", ServeFile::new(DASHBOARD_PAGE))
        .route_service("/settings", ServeFile::new(DASHBOARD_PAGE))
        // With a "/" at the end, the page's files would be looked for
        // under /settings/, so those go to the address without it.
        .route(
            "/reports/",
            get(|| async { Redirect::permanent("/reports") }),
        )
        .route(
            "/settings/",
            get(|| async { Redirect::permanent("/settings") }),
        )
        .route("/dashboard/{*path}", get(dashboard_address))
}

/// Any address under /dashboard/. One scan's result
/// (/dashboard/alibaba/<scan id>) gets the dashboard page, which opens
/// the result itself after checking the person is signed in. Anything
/// else is one of the dashboard's own files (/dashboard/js/init.js),
/// sent as before - or the "page not found" page. (One route does
/// both: the router does not allow a separate route for each.)
pub async fn dashboard_address(Path(path): Path<String>, request: Request) -> Response {
    if is_result_address(&path) {
        return match tokio::fs::read_to_string(DASHBOARD_PAGE).await {
            Ok(html) => Html(html).into_response(),
            Err(_) => StatusCode::NOT_FOUND.into_response(),
        };
    }
    let (mut parts, body) = request.into_parts();
    let Ok(file_path) = format!("/{path}").parse() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    parts.uri = file_path;
    ServeDir::new(DASHBOARD_DIR)
        .not_found_service(ServeFile::new(NOT_FOUND_FILE))
        .try_call(Request::from_parts(parts, body))
        .await
        .map(IntoResponse::into_response)
        .unwrap_or_else(|_| StatusCode::NOT_FOUND.into_response())
}

/// "alibaba/<scan id>": a platform name, then a scan id.
pub fn is_result_address(path: &str) -> bool {
    let mut parts = path.trim_end_matches('/').split('/');
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(platform), Some(id), None)
            if !platform.is_empty() && Uuid::parse_str(id).is_ok()
    )
}

#[cfg(test)]
mod tests {
    use super::is_result_address;

    #[test]
    fn a_result_address_is_a_platform_then_a_scan_id() {
        assert!(is_result_address(
            "alibaba/0193f2a1-0000-7000-8000-000000000001"
        ));
        assert!(is_result_address(
            "website/0193f2a1-0000-7000-8000-000000000001/"
        ));
        assert!(!is_result_address("js/init.js"));
        assert!(!is_result_address("alibaba/not-a-scan"));
        assert!(!is_result_address(
            "a/b/0193f2a1-0000-7000-8000-000000000001"
        ));
        assert!(!is_result_address("/0193f2a1-0000-7000-8000-000000000001"));
    }
}
