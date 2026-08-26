use axum::http::{HeaderValue, Method, header};
use axum::serve;
use axum::{Router, response::Redirect, routing::get};
use backend::db::{bootstrap::run_grants, connection::load_pool};
use backend::routes::{analyze, auth, billing, dashboard, fraud_reports, subscribe};
use sqlx::{Pool, Postgres};
use tokio::net::TcpListener;
use tower_http::{
    cors::{Any, CorsLayer},
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
};

async fn setup_database() -> Pool<Postgres> {
    let admin_pool = load_pool("ADMIN_URL").await;
    let app_pool = load_pool("APP_URL").await;

    sqlx::migrate!("../migrations")
        .run(&admin_pool)
        .await
        .expect("migration expected");

    run_grants(&admin_pool).await;

    app_pool
}

/// Builds the CORS rules. It lets the requests come from any site (needed
/// for the browser extension), but only allows the specific methods
/// and headers this app actually uses.
fn build_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
}

fn build_router(app_pool: Pool<Postgres>) -> Router {
    Router::new()
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
        .layer(build_cors_layer())
        .with_state(app_pool)
}

async fn run_server(app: Router) {
    let listener = TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("should have a tcp listener binding");

    println!("Server running on port: 3000");
    serve(listener, app).await.expect("expected to serve");
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let app_pool = setup_database().await;
    let app = build_router(app_pool);
    run_server(app).await;
}
