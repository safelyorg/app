use sqlx::{Pool, Postgres, postgres::PgPoolOptions};

pub async fn load_pool(env_key: &str) -> Pool<Postgres> {
    let database_url = std::env::var(env_key)
        .unwrap_or_else(|_| panic!("{} needs to be present in the .env file", env_key));

    // APP_URL serves every real request from real users, so it gets the
    // larger share - sized for a Hetzner CX23 (2 vCPU, 4GB RAM): enough
    // headroom for meaningful early growth without risking the server's
    // memory or approaching Postgres's own default 100-connection
    // server-wide ceiling. ADMIN_URL only ever does brief work at
    // startup (migrations, grants), so it stays small on purpose.
    let max_connections = if env_key == "APP_URL" { 15 } else { 3 };

    let db_pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(&database_url)
        .await
        .expect("Database connection needs to be established");

    db_pool
}
