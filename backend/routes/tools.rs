use crate::handlers::tools::{sitemap, tool, tool_lang, tools_list, tools_list_lang};
use axum::{Router, routing::get};
use sqlx::{Pool, Postgres};

/// Public, cacheable pages in every language, and the sitemap.
///
/// - /tools, /pt-br/tools                 the list of free tools
/// - /tools/{slug}, /pt-br/tools/{slug}   one tool
pub fn tools_routes() -> Router<Pool<Postgres>> {
    Router::new()
        .route("/tools", get(tools_list))
        .route("/tools/", get(tools_list))
        .route("/tools/{slug}", get(tool))
        .route("/{lang}/tools", get(tools_list_lang))
        .route("/{lang}/tools/", get(tools_list_lang))
        .route("/{lang}/tools/{slug}", get(tool_lang))
        .route("/sitemap.xml", get(sitemap))
}
