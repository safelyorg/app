use axum::http::{HeaderValue, Method, header};
use axum::{Router, response::Redirect, routing::get};
use backend::db::{bootstrap::run_grants, connection::load_pool};
use backend::routes::{analyze, auth, billing, dashboard, fraud_reports, subscribe};
use tower_http::{
    cors::{Any, CorsLayer},
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let admin_pool = load_pool("ADMIN_URL").await;
    let app_pool = load_pool("APP_URL").await;
    sqlx::migrate!("../migrations")
        .run(&admin_pool)
        .await
        .expect("migration expected");
    run_grants(&admin_pool).await;
    let app = Router::new()
        .merge(analyze::analyze_routes())
        .merge(fraud_reports::fraud_reports_routes())
        .merge(auth::auth_routes())
        .merge(dashboard::dashboard_routes())
        .merge(billing::billing_routes())
        .merge(subscribe::subscribe_routes())
        .route(
            "/dashboard",
            get(|| async { Redirect::permanent("/dashboard/") }),
        )
        .nest_service(
            "/dashboard/",
            ServeDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../dashboard"))
                .append_index_html_on_directories(true),
        )
        .nest_service(
            "/extension",
            ServeDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../extension")),
        )
        .route_service(
            "/",
            ServeFile::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../site/templates/index.html"
            )),
        )
        .route_service(
            "/signin.html",
            ServeFile::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../site/templates/signin.html"
            )),
        )
        .route_service(
            "/subscribe",
            ServeFile::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../site/templates/subscribe.html"
            )),
        )
        .fallback_service(
            ServeDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../site"))
                .append_index_html_on_directories(true),
        )
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache, no-store, must-revalidate"),
        ))
        .layer(
            // Origin stays open (Any) since content scripts send requests
            // using the origin of whatever website they're running on
            // (OLX, or any site the domain-check runs on) - not
            // safely.sh itself, so restricting origin here would break
            // the extension's own analyze calls. Methods and headers are
            // narrowed to exactly what this app actually uses, so
            // anything else gets refused automatically rather than
            // reaching a handler at all.
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
                .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]),
        )
        .with_state(app_pool);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Server running on port: 3000");
    axum::serve(listener, app).await.unwrap();
}
