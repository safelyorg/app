use crate::models::sellers::{SellerVerification, Sellers, SellersRequest};
use chrono::NaiveDate;
use sqlx::{Error, Pool, Postgres};
use uuid::Uuid;

pub async fn find_seller(
    pool: &Pool<Postgres>,
    platform: &str,
    platform_id: &str,
) -> Result<Option<Sellers>, Error> {
    let seller = sqlx::query_as::<_, Sellers>(
        "SELECT * FROM sellers
         WHERE platform = $1 AND platform_id = $2
         LIMIT 1",
    )
    .bind(platform)
    .bind(platform_id)
    .fetch_optional(pool)
    .await?;

    Ok(seller)
}

pub async fn create_seller(
    pool: &Pool<Postgres>,
    request: &SellersRequest,
) -> Result<Sellers, Error> {
    let id = Uuid::now_v7();

    let seller = sqlx::query_as::<_, Sellers>(
        "
        INSERT INTO sellers (
            id,
            platform,
            platform_id,
            name,
            handle,
            phone,
            profile_url,
            join_date,
            verification,
            total_deals,
            disputes,
            completion_rate,
            location,
            created_at,
            updated_at
        )
        VALUES (
            $1, $2, $3, $4, $5,
            $6, $7, $8, $9, $10,
            $11, $12, $13, NOW(), NOW()
        )
        ON CONFLICT (platform, platform_id)
        DO UPDATE SET
            name = COALESCE(EXCLUDED.name, sellers.name),
            updated_at = NOW()
        RETURNING *
        ",
    )
    .bind(id)
    .bind(&request.platform)
    .bind(request.platform_id.as_deref().unwrap_or("unknown"))
    .bind(&request.name)
    .bind(&request.handle)
    .bind(&request.phone)
    .bind(&request.profile_url)
    .bind(None::<NaiveDate>)
    .bind(SellerVerification::Unknown)
    .bind(0_i32)
    .bind(0_i32)
    .bind(None::<i64>)
    .bind(&request.location)
    .fetch_one(pool)
    .await?;

    Ok(seller)
}
