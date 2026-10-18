use crate::handlers::blogs::{blog, blog_lang};
use crate::handlers::tools::{sitemap, tool, tool_lang, tools_list, tools_list_lang};
use axum::{Router, routing::get};
use sqlx::{Pool, Postgres};

/// Public, cacheable pages in every language, and the sitemap.
///
/// - /tools, /pt-br/tools                 the list: free tools, and blogs behind
///   the "Blogs" switch (/tools#blogs)
/// - /tools/{slug}, /pt-br/tools/{slug}   one tool
/// - /blog/{slug}, /pt-br/blog/{slug}     one blog
pub fn tools_routes() -> Router<Pool<Postgres>> {
    Router::new()
        .route("/tools", get(tools_list))
        .route("/tools/", get(tools_list))
        .route("/tools/{slug}", get(tool))
        .route("/{lang}/tools", get(tools_list_lang))
        .route("/{lang}/tools/", get(tools_list_lang))
        .route("/{lang}/tools/{slug}", get(tool_lang))
        .route("/blog/{slug}", get(blog))
        .route("/{lang}/blog/{slug}", get(blog_lang))
        .route("/sitemap.xml", get(sitemap))
}
