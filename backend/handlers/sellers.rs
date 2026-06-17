use axum::{Json, extract::State};
use chrono::NaiveDate;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

use crate::models::sellers::{Sellers, SellersRequest, SellersResponse};

pub async fn create_seller(
    State(pool): State<Pool<Postgres>>,
    Json(request): Json<SellersRequest>,
) -> Result<Json<SellersResponse>, String> {
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
        RETURNING *
        ",
    )
    .bind(id)
    .bind(&request.platform)
    .bind(&request.platform_id)
    .bind(&request.name)
    .bind(String::from("ibilalkayy"))
    .bind(&request.phone)
    .bind(&request.profile_url)
    .bind(None::<NaiveDate>)
    .bind("unknown")
    .bind(0_i32)
    .bind(0_i32)
    .bind(None::<f64>)
    .bind(Some(String::from("pakistan")))
    .fetch_one(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(Json(SellersResponse::from(seller)))
}
