use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "listing_categories", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ListingCategories {
    MobilePhones,
    Tablets,
    Laptops,
    Computers,
    Accessories,
    Cameras,
    TvAndVideo,
    Audio,
    Gaming,
    Appliances,
    Furniture,
    Clothing,
    Shoes,
    Watches,
    Jewellery,
    Vehicles,
    VehicleParts,
    Property,
    Books,
    Sports,
    Toys,
    Tools,
    Services,
    Other,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Listings {
    pub id: Uuid,
    pub user_id: Uuid,
    pub platform: String,
    pub listing_url: String,
    pub listing_id: String,
    pub title: Option<String>,
    pub price: Option<i64>,
    pub description: Option<String>,
    pub category: ListingCategories,
    pub image_urls: Vec<String>,
    pub posted_date: Option<NaiveDate>,
    pub first_seen_at: DateTime<Utc>,
    pub last_analyzed_at: DateTime<Utc>,
}
