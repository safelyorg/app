use crate::db::{bootstrap::run_grants, connection::load_pool};
use axum::Router;
use tower_http::services::ServeDir;

#[path = "../db/mod.rs"]
pub mod db;

#[path = "../handlers/mod.rs"]
pub mod handlers;

#[path = "../models/mod.rs"]
pub mod models;

#[path = "../routes/mod.rs"]
pub mod routes;

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
        .merge(routes::sellers::seller_routes())
        .nest_service(
            "/static/",
            ServeDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/static")),
        )
        .with_state(app_pool);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    println!("Server running on port: 3000");

    axum::serve(listener, app).await.unwrap();
}
