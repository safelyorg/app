use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use std::env::var;

pub async fn load_pool(env_key: &str) -> Pool<Postgres> {
    let database_url =
        var(env_key).unwrap_or_else(|_| panic!("{} needs to be present in the .env file", env_key));

    let max_connections = if env_key == "APP_URL" { 15 } else { 3 };
    let db_pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(&database_url)
        .await
        .expect("database connection needs to be established");

    db_pool
}
