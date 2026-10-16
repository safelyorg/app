use crate::handlers::blogs::{blog_post, blog_post_lang, blogs_list, blogs_list_lang, sitemap};
use axum::{Router, routing::get};
use sqlx::{Pool, Postgres};

/// Public, cacheable pages: the blog in every language, and the sitemap.
pub fn blogs_routes() -> Router<Pool<Postgres>> {
    Router::new()
        .route("/blogs", get(blogs_list))
        .route("/blogs/", get(blogs_list))
        .route("/blogs/{slug}", get(blog_post))
        .route("/{lang}/blogs", get(blogs_list_lang))
        .route("/{lang}/blogs/", get(blogs_list_lang))
        .route("/{lang}/blogs/{slug}", get(blog_post_lang))
        .route("/sitemap.xml", get(sitemap))
}
