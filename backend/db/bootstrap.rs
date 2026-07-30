use sqlx::{PgPool, query};

pub async fn run_grants(pool: &PgPool) {
    query(
        r#"
            GRANT USAGE ON SCHEMA public TO safely;
        "#,
    )
    .execute(pool)
    .await
    .expect("public schema access should be granted to safely role");

    query(
        r#"
            GRANT CREATE ON SCHEMA public TO safely;
        "#,
    )
    .execute(pool)
    .await
    .expect("safely role should be allowed to create new objects in the public schema");

    query(
        r#"
            GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO safely;
        "#,
    )
    .execute(pool)
    .await
    .expect("safely role should have all privileges on existing tables in the public schema");

    query(
        r#"
            ALTER DEFAULT PRIVILEGES IN SCHEMA public
            GRANT ALL ON TABLES TO safely;
        "#,
    )
    .execute(pool)
    .await
    .expect("new tables should automatically grant privileges to safely role");
}
