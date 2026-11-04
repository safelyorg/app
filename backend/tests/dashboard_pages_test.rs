// The dashboard's own addresses (routes/dashboard_pages.rs): History,
// My Reports, Settings and one result each get the dashboard page; the
// dashboard's files are still sent; anything else is "page not found".
// No database is needed.

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use backend::routes::{
    analyze, auth, billing, dashboard, dashboard_pages, fraud_reports, outcomes, platform_domains,
    reviews, subscribe, tool_api, tools,
};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

const ID: &str = "0193f2a1-0000-7000-8000-000000000001";

async fn get(app: &Router, path: &str) -> (StatusCode, String, String) {
    let response = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let header_text = |name: header::HeaderName| {
        response
            .headers()
            .get(name)
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default()
    };
    let location = header_text(header::LOCATION);
    let content_type = header_text(header::CONTENT_TYPE);
    let body = to_bytes(response.into_body(), 10_000_000).await.unwrap();
    let text = String::from_utf8_lossy(&body).to_string();
    (
        status,
        if location.is_empty() {
            content_type
        } else {
            location
        },
        text,
    )
}

fn pages() -> Router {
    dashboard_pages::dashboard_pages_routes()
}

#[tokio::test]
async fn history_reports_settings_and_a_result_get_the_dashboard() {
    let app = pages();
    for path in [
        "/dashboard/",
        "/reports",
        "/settings",
        &format!("/dashboard/alibaba/{ID}"),
        &format!("/dashboard/website/{ID}"),
    ] {
        let (status, content_type, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(
            content_type.starts_with("text/html"),
            "{path}: {content_type}"
        );
        assert!(
            body.contains("view-settings"),
            "{path} is not the dashboard page"
        );
    }
}

#[tokio::test]
async fn addresses_with_a_slash_at_the_end_go_to_the_one_without() {
    let app = pages();
    for (from, to) in [
        ("/dashboard", "/dashboard/"),
        ("/settings/", "/settings"),
        ("/reports/", "/reports"),
    ] {
        let (status, location, _) = get(&app, from).await;
        assert_eq!(status, StatusCode::PERMANENT_REDIRECT, "{from}");
        assert_eq!(location, to, "{from}");
    }
}

#[tokio::test]
async fn the_dashboards_files_are_still_sent() {
    let app = pages();
    let (status, content_type, _) = get(&app, "/dashboard/js/init.js").await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.contains("javascript"), "{content_type}");
    let (status, content_type, _) = get(&app, "/dashboard/css/dashboard.css").await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.contains("css"), "{content_type}");
}

#[tokio::test]
async fn anything_else_is_page_not_found() {
    let app = pages();
    for path in [
        "/dashboard/alibaba/not-a-scan",
        "/dashboard/js/missing.js",
        "/dashboard/settings",
        "/dashboard/%2e%2e/backend/Cargo.toml",
    ] {
        let (status, _, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert!(!body.contains("view-settings"), "{path}");
        assert!(!body.contains("[package]"), "{path} leaked a file");
    }
}

// The real server joins every group of routes into one; two routes for
// the same address would stop it at start-up.
#[tokio::test]
async fn every_group_of_routes_fits_together() {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://localhost/not_used")
        .unwrap();
    let app: Router = Router::new()
        .merge(analyze::analyze_routes())
        .merge(fraud_reports::fraud_reports_routes())
        .merge(auth::auth_routes())
        .merge(dashboard::dashboard_routes())
        .merge(billing::billing_routes())
        .merge(subscribe::subscribe_routes())
        .merge(outcomes::outcomes_routes())
        .merge(platform_domains::platform_domains_routes())
        .merge(reviews::reviews_routes())
        .merge(tool_api::tool_api_routes())
        .merge(dashboard_pages::dashboard_pages_routes())
        .merge(tools::tools_routes())
        .with_state(pool);
    let (status, _, _) = get(&app, "/settings").await;
    assert_eq!(status, StatusCode::OK);
}
