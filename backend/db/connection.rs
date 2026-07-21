use sqlx::{Pool, Postgres, postgres::PgPoolOptions};

pub async fn load_pool(env_key: &str) -> Pool<Postgres> {
    let database_url = std::env::var(env_key)
        .unwrap_or_else(|_| panic!("{} needs to be present in the .env file", env_key));

    let db_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Database connection needs to be established");

    db_pool
}
