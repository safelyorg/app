use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

/// What the dashboard sends when a user writes or edits their review.
#[derive(Debug, Deserialize)]
pub struct ReviewRequest {
    pub display_name: String,
    pub role: Option<String>,
    pub rating: i16,
    pub body: String,
    pub consent: bool,
}

/// A review after its text has been trimmed and checked.
#[derive(Debug, PartialEq)]
pub struct CleanReview {
    pub display_name: String,
    pub role: Option<String>,
    pub rating: i16,
    pub body: String,
}

/// The signed-in user's own review, shown back in the dashboard.
#[derive(Debug, Serialize, FromRow)]
pub struct MyReview {
    pub id: i64,
    pub display_name: String,
    pub role: Option<String>,
    pub rating: i16,
    pub body: String,
    pub status: String,
    pub has_photo: bool,
}

/// An approved review, as read from the database.
#[derive(Debug, FromRow)]
pub struct ApprovedReviewRow {
    pub id: i64,
    pub display_name: String,
    pub role: Option<String>,
    pub rating: i16,
    pub body: String,
}

/// An approved review, as shown on the website.
#[derive(Debug, Serialize)]
pub struct PublicReview {
    pub display_name: String,
    pub role: Option<String>,
    pub rating: i16,
    pub body: String,
    pub photo_url: String,
}

/// A review photo with its image type.
#[derive(Debug, FromRow)]
pub struct ReviewPhoto {
    pub photo_data: Vec<u8>,
    pub photo_content_type: String,
}
