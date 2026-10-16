use crate::handlers::blogs::{blog_post, blogs_list, sitemap};
use axum::{Router, routing::get};
use sqlx::{Pool, Postgres};

/// Public, cacheable pages: the blog and the sitemap.
pub fn blogs_routes() -> Router<Pool<Postgres>> {
    Router::new()
        .route("/blogs", get(blogs_list))
        .route("/blogs/", get(blogs_list))
        .route("/blogs/{slug}", get(blog_post))
        .route("/sitemap.xml", get(sitemap))
}
