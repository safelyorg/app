use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Type, prelude::FromRow};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "listing_category", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ListingCategory {
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
    pub seller_id: Option<Uuid>,
    pub platform: String,
    pub listing_url: String,
    pub listing_id: Option<String>,
    pub title: Option<String>,
    pub price: Option<i64>,
    pub description: Option<String>,
    pub category: Option<ListingCategory>,
    pub image_urls: Option<Vec<String>>,
    pub posted_date: Option<NaiveDate>,
    pub first_seen_at: DateTime<Utc>,
    pub last_analyzed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ListingsRequest {
    pub seller_id: Option<Uuid>,
    pub platform: String,
    pub listing_url: String,
    pub listing_id: Option<String>,
    pub title: Option<String>,
    pub price: Option<i64>,
    pub description: Option<String>,
    pub category: Option<ListingCategory>,
    pub image_urls: Option<Vec<String>>,
    pub posted_date: Option<NaiveDate>,
}
