use sqlx::{Error, Pool, Postgres};
use uuid::Uuid;

use crate::models::listings::{Listings, ListingsRequest};

pub async fn find_listing(
    pool: &Pool<Postgres>,
    listing_url: &str,
) -> Result<Option<Listings>, Error> {
    let listing = sqlx::query_as::<_, Listings>(
        "SELECT * FROM listings
         WHERE listing_url = $1
         LIMIT 1",
    )
    .bind(listing_url)
    .fetch_optional(pool)
    .await?;

    Ok(listing)
}

pub async fn create_listing(
    pool: &Pool<Postgres>,
    request: &ListingsRequest,
    seller_id: Uuid,
) -> Result<Listings, Error> {
    let id = Uuid::now_v7();

    let listing = sqlx::query_as::<_, Listings>(
        "
        INSERT INTO listings (
            id,
            seller_id,
            platform,
            listing_url,
            listing_id,
            title,
            price,
            description,
            category,
            image_urls,
            posted_date,
            first_seen_at,
            last_analyzed_at,
            updated_at
        )
        VALUES (
            $1,  $2,    $3,   $4,   $5,
            $6,  $7,    $8,   $9,   $10,
            $11, NOW(), NULL, NOW()
        )
        RETURNING *
        ",
    )
    .bind(id)
    .bind(seller_id)
    .bind(&request.platform)
    .bind(&request.listing_url)
    .bind(&request.listing_id)
    .bind(&request.title)
    .bind(&request.price)
    .bind(&request.description)
    .bind(&request.category)
    .bind(&request.image_urls)
    .bind(&request.posted_date)
    .fetch_one(pool)
    .await?;

    Ok(listing)
}
