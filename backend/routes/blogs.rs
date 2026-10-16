use crate::handlers::blogs::{
    blog, blog_lang, blog_without_name, blog_without_name_lang, blogs_list, blogs_list_lang,
    old_blog, old_blog_lang, sitemap,
};
use axum::{Router, routing::get};
use sqlx::{Pool, Postgres};

/// Public, cacheable pages in every language, and the sitemap.
///
/// - /blogs, /pt-br/blogs               the list of articles
/// - /blog/{slug}, /pt-br/blog/{slug}   one article
/// - /blogs/{slug}, /pt-br/blogs/{slug} old article addresses, sent to /blog/...
pub fn blogs_routes() -> Router<Pool<Postgres>> {
    Router::new()
        .route("/blogs", get(blogs_list))
        .route("/blogs/", get(blogs_list))
        .route("/{lang}/blogs", get(blogs_list_lang))
        .route("/{lang}/blogs/", get(blogs_list_lang))
        .route("/blog", get(blog_without_name))
        .route("/blog/", get(blog_without_name))
        .route("/blog/{slug}", get(blog))
        .route("/{lang}/blog", get(blog_without_name_lang))
        .route("/{lang}/blog/", get(blog_without_name_lang))
        .route("/{lang}/blog/{slug}", get(blog_lang))
        .route("/blogs/{slug}", get(old_blog))
        .route("/{lang}/blogs/{slug}", get(old_blog_lang))
        .route("/sitemap.xml", get(sitemap))
}
