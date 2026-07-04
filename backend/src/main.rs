use crate::db::{bootstrap::run_grants, connection::load_pool};
use axum::{Router, response::Redirect, routing::get};
use tower_http::{cors::CorsLayer, services::ServeDir};

#[path = "../db/mod.rs"]
pub mod db;
#[path = "../handlers/mod.rs"]
pub mod handlers;
#[path = "../models/mod.rs"]
pub mod models;
#[path = "../routes/mod.rs"]
pub mod routes;
#[path = "../services/mod.rs"]
pub mod services;

#[tokio::main]
async fn main() {
    let admin_pool = load_pool("ADMIN_URL").await;
    let app_pool = load_pool("APP_URL").await;

    sqlx::migrate!("../migrations")
        .run(&admin_pool)
        .await
        .expect("migration expected");
    run_grants(&admin_pool).await;

    let app = Router::new()
        .merge(routes::analyze::analyze_routes())
        .merge(routes::fraud_reports::fraud_reports_routes())
        .merge(routes::auth::auth_routes())
        .merge(routes::dashboard::dashboard_routes())
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
        .fallback_service(
            ServeDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../site"))
                .append_index_html_on_directories(true),
        )
        .layer(CorsLayer::permissive())
        .with_state(app_pool);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Server running on port: 3000");
    axum::serve(listener, app).await.unwrap();
}
