use crate::models::reviews::{
    ApprovedReviewRow, CleanReview, MyReview, ReviewPhoto, ReviewRequest,
};
use sqlx::{Error, Pool, Postgres, query_as, query_scalar};
use uuid::Uuid;

pub const MAX_NAME_CHARS: usize = 60;
pub const MAX_ROLE_CHARS: usize = 80;
pub const MIN_BODY_CHARS: usize = 20;
pub const MAX_BODY_CHARS: usize = 600;
/// The dashboard shrinks photos to 400x400 before upload (about 50 KB),
/// so 1 MB leaves plenty of room while stopping huge uploads.
pub const MAX_PHOTO_BYTES: usize = 1_000_000;
/// How many approved reviews the website shows.
pub const MAX_PUBLIC_REVIEWS: i64 = 30;

/// Trims the review and checks every field. The error text is shown
/// to the user as it is.
pub fn validate_review(req: &ReviewRequest) -> Result<CleanReview, String> {
    if !req.consent {
        return Err("Please allow Safely to show your review, name and photo.".to_string());
    }

    let display_name = req.display_name.trim().to_string();
    if display_name.is_empty() {
        return Err("Please enter your name.".to_string());
    }
    if display_name.chars().count() > MAX_NAME_CHARS {
        return Err(format!(
            "Your name can be at most {} characters.",
            MAX_NAME_CHARS
        ));
    }

    let role = req
        .role
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_string);
    if role
        .as_ref()
        .is_some_and(|r| r.chars().count() > MAX_ROLE_CHARS)
    {
        return Err(format!(
            "Your role can be at most {} characters.",
            MAX_ROLE_CHARS
        ));
    }

    if !(1..=5).contains(&req.rating) {
        return Err("Please choose 1 to 5 stars.".to_string());
    }

    let body = req.body.trim().to_string();
    let body_chars = body.chars().count();
    if body_chars < MIN_BODY_CHARS {
        return Err(format!(
            "Please write at least {} characters.",
            MIN_BODY_CHARS
        ));
    }
    if body_chars > MAX_BODY_CHARS {
        return Err(format!(
            "Your review can be at most {} characters.",
            MAX_BODY_CHARS
        ));
    }

    Ok(CleanReview {
        display_name,
        role,
        rating: req.rating,
        body,
    })
}

/// Reads the image type from the file's first bytes, not from what the
/// browser claims. Only JPEG, PNG and WebP are accepted.
pub fn detect_image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    None
}

/// Saves the user's review. Writing again replaces the old one and sends
/// it back to "pending", so every change is approved before it shows.
pub async fn save_review(
    pool: &Pool<Postgres>,
    user_id: Uuid,
    review: &CleanReview,
) -> Result<MyReview, Error> {
    query_as::<_, MyReview>(
        "INSERT INTO reviews (user_id, display_name, role, rating, body, consent_at)
         VALUES ($1, $2, $3, $4, $5, NOW())
         ON CONFLICT (user_id) DO UPDATE SET
             display_name = EXCLUDED.display_name,
             role = EXCLUDED.role,
             rating = EXCLUDED.rating,
             body = EXCLUDED.body,
             consent_at = NOW(),
             status = 'pending',
             approved_at = NULL,
             updated_at = NOW()
         RETURNING id, display_name, role, rating, body, status,
                   (photo_data IS NOT NULL) AS has_photo",
    )
    .bind(user_id)
    .bind(&review.display_name)
    .bind(&review.role)
    .bind(review.rating)
    .bind(&review.body)
    .fetch_one(pool)
    .await
}

/// Saves the photo on the user's review and sends the review back to
/// "pending". Returns false when the user has no review yet.
pub async fn save_review_photo(
    pool: &Pool<Postgres>,
    user_id: Uuid,
    photo: &[u8],
    content_type: &str,
) -> Result<bool, Error> {
    let updated: Option<i64> = query_scalar(
        "UPDATE reviews SET
             photo_data = $2,
             photo_content_type = $3,
             status = 'pending',
             approved_at = NULL,
             updated_at = NOW()
         WHERE user_id = $1
         RETURNING id",
    )
    .bind(user_id)
    .bind(photo)
    .bind(content_type)
    .fetch_optional(pool)
    .await?;
    Ok(updated.is_some())
}

/// The signed-in user's own review, if they wrote one.
pub async fn find_my_review(
    pool: &Pool<Postgres>,
    user_id: Uuid,
) -> Result<Option<MyReview>, Error> {
    query_as::<_, MyReview>(
        "SELECT id, display_name, role, rating, body, status,
                (photo_data IS NOT NULL) AS has_photo
         FROM reviews WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// Approved reviews with a photo, newest approval first.
pub async fn list_approved_reviews(pool: &Pool<Postgres>) -> Result<Vec<ApprovedReviewRow>, Error> {
    query_as::<_, ApprovedReviewRow>(
        "SELECT id, display_name, role, rating, body
         FROM reviews
         WHERE status = 'approved' AND photo_data IS NOT NULL
         ORDER BY approved_at DESC
         LIMIT $1",
    )
    .bind(MAX_PUBLIC_REVIEWS)
    .fetch_all(pool)
    .await
}

/// The photo of an approved review. Pending and rejected photos are
/// never served.
pub async fn find_approved_photo(
    pool: &Pool<Postgres>,
    review_id: i64,
) -> Result<Option<ReviewPhoto>, Error> {
    query_as::<_, ReviewPhoto>(
        "SELECT photo_data, photo_content_type
         FROM reviews
         WHERE id = $1 AND status = 'approved'
           AND photo_data IS NOT NULL AND photo_content_type IS NOT NULL",
    )
    .bind(review_id)
    .fetch_optional(pool)
    .await
}

/// The signed-in user's own review photo, whatever its status.
pub async fn find_my_photo(
    pool: &Pool<Postgres>,
    user_id: Uuid,
) -> Result<Option<ReviewPhoto>, Error> {
    query_as::<_, ReviewPhoto>(
        "SELECT photo_data, photo_content_type
         FROM reviews
         WHERE user_id = $1
           AND photo_data IS NOT NULL AND photo_content_type IS NOT NULL",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(
        name: &str,
        role: Option<&str>,
        rating: i16,
        body: &str,
        consent: bool,
    ) -> ReviewRequest {
        ReviewRequest {
            display_name: name.to_string(),
            role: role.map(str::to_string),
            rating,
            body: body.to_string(),
            consent,
        }
    }

    const GOOD_BODY: &str = "Safely showed me the supplier had no real address.";

    #[test]
    fn accepts_a_good_review_and_trims_it() {
        let clean = validate_review(&request(
            "  Ana  ",
            Some("  Buyer · São Paulo "),
            5,
            "  Safely showed me the supplier had no real address.  ",
            true,
        ))
        .unwrap();
        assert_eq!(
            clean,
            CleanReview {
                display_name: "Ana".to_string(),
                role: Some("Buyer · São Paulo".to_string()),
                rating: 5,
                body: GOOD_BODY.to_string(),
            }
        );
    }

    #[test]
    fn empty_role_becomes_none() {
        let clean = validate_review(&request("Ana", Some("   "), 4, GOOD_BODY, true)).unwrap();
        assert_eq!(clean.role, None);
    }

    #[test]
    fn needs_consent() {
        assert!(validate_review(&request("Ana", None, 5, GOOD_BODY, false)).is_err());
    }

    #[test]
    fn needs_a_name() {
        assert!(validate_review(&request("   ", None, 5, GOOD_BODY, true)).is_err());
    }

    #[test]
    fn name_too_long() {
        let name = "a".repeat(MAX_NAME_CHARS + 1);
        assert!(validate_review(&request(&name, None, 5, GOOD_BODY, true)).is_err());
    }

    #[test]
    fn role_too_long() {
        let role = "a".repeat(MAX_ROLE_CHARS + 1);
        assert!(validate_review(&request("Ana", Some(&role), 5, GOOD_BODY, true)).is_err());
    }

    #[test]
    fn rating_must_be_one_to_five() {
        assert!(validate_review(&request("Ana", None, 0, GOOD_BODY, true)).is_err());
        assert!(validate_review(&request("Ana", None, 6, GOOD_BODY, true)).is_err());
        assert!(validate_review(&request("Ana", None, 1, GOOD_BODY, true)).is_ok());
    }

    #[test]
    fn body_length_limits() {
        assert!(validate_review(&request("Ana", None, 5, "Too short", true)).is_err());
        let long = "a".repeat(MAX_BODY_CHARS + 1);
        assert!(validate_review(&request("Ana", None, 5, &long, true)).is_err());
        let max = "a".repeat(MAX_BODY_CHARS);
        assert!(validate_review(&request("Ana", None, 5, &max, true)).is_ok());
    }

    #[test]
    fn body_counts_characters_not_bytes() {
        // 300 "ã" = 600 bytes but 300 characters: allowed.
        let body = "ã".repeat(300);
        assert!(validate_review(&request("Ana", None, 5, &body, true)).is_ok());
    }

    #[test]
    fn detects_image_types() {
        assert_eq!(
            detect_image_type(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]),
            Some("image/jpeg")
        );
        assert_eq!(
            detect_image_type(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00]),
            Some("image/png")
        );
        assert_eq!(
            detect_image_type(b"RIFF\x00\x00\x00\x00WEBPVP8 "),
            Some("image/webp")
        );
    }

    #[test]
    fn rejects_other_files() {
        assert_eq!(
            detect_image_type(b"<svg xmlns='http://www.w3.org/2000/svg'>"),
            None
        );
        assert_eq!(detect_image_type(b"GIF89a"), None);
        assert_eq!(detect_image_type(b"%PDF-1.7"), None);
        assert_eq!(detect_image_type(b"RIFF"), None);
        assert_eq!(detect_image_type(&[]), None);
    }
}
