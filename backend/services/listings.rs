use crate::models::listings::{Listings, ListingsRequest};
use sqlx::{Error, Pool, Postgres, Row, query, query_as};
use uuid::Uuid;

/// It either creates a brand-new listing, or — if this exact URL has already been seen,
/// updates the existing one with fresh information, without wiping out good data with blanks.
///
/// It generates an ID, ready for a potential new listing, do the actual database operation,
/// binds all eleven real values and returns the real, final row.
pub async fn create_listing(
    pool: &Pool<Postgres>,
    request: &ListingsRequest,
    seller_id: Uuid,
) -> Result<Listings, Error> {
    let id = Uuid::now_v7();

    let listing = query_as::<_, Listings>(
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
        ON CONFLICT (listing_url)
        DO UPDATE SET
            title = COALESCE(EXCLUDED.title, listings.title),
            price = COALESCE(EXCLUDED.price, listings.price),
            description = COALESCE(EXCLUDED.description, listings.description),
            seller_id = COALESCE(EXCLUDED.seller_id, listings.seller_id),
            last_analyzed_at = NOW(),
            updated_at = NOW()
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

/// It counts how many times a seller's listings were analyzed, month by month,
/// over the last year, and returns exactly 12 numbers — one for each month.
///
/// It runs the real database query, sets up a list of 12 zeros, as the starting point,
/// fills in the real numbers, for whichever months actually had activity and
/// returns the completed list.
pub async fn get_monthly_visit_activity(
    pool: &Pool<Postgres>,
    seller_id: Uuid,
) -> Result<Vec<i32>, Error> {
    let rows = query(
        "
        SELECT
            EXTRACT(MONTH FROM a.created_at)::int as month,
            COUNT(*)::int as visits
        FROM analysis a
        JOIN listings l ON a.listing_id = l.id
        WHERE l.seller_id = $1
            AND a.created_at >= DATE_TRUNC('month', NOW()) - INTERVAL '11 months'
        GROUP BY month
        ORDER BY month ASC
        ",
    )
    .bind(seller_id)
    .fetch_all(pool)
    .await?;

    let mut activity = vec![0i32; 12];
    for row in rows {
        let month: i32 = row.get("month");
        let visits: i32 = row.get("visits");
        let index = (month - 1) as usize;
        if index < 12 {
            activity[index] = visits;
        }
    }

    Ok(activity)
}
