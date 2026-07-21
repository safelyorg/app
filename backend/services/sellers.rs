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
    verification: SellerVerification,
) -> Result<Sellers, Error> {
    let id = Uuid::now_v7();
    let join_date = request.join_date.as_deref().and_then(|s| {
        let year_str = s.split_whitespace().last()?;
        let year: i32 = year_str.parse().ok()?;
        NaiveDate::from_ymd_opt(year, 1, 1)
    });
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
            location,
            last_active_text,
            created_at,
            updated_at
        )
        VALUES (
            $1, $2, $3, $4, $5,
            $6, $7, $8, $9, $10,
            $11, NOW(), NOW()
        )
        ON CONFLICT (platform, platform_id)
        DO UPDATE SET
            name = COALESCE(EXCLUDED.name, sellers.name),
            join_date = COALESCE(EXCLUDED.join_date, sellers.join_date),
            location = COALESCE(EXCLUDED.location, sellers.location),
            last_active_text = COALESCE(EXCLUDED.last_active_text, sellers.last_active_text),
            verification = EXCLUDED.verification,
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
    .bind(join_date)
    .bind(verification)
    .bind(&request.location)
    .bind(&request.last_active)
    .fetch_one(pool)
    .await?;
    Ok(seller)
}
