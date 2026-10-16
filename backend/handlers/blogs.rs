use crate::services::blogs::{
    BLOGS_DIR, is_valid_slug, load_posts, render_list, render_post, render_sitemap,
};
use axum::{
    extract::Path,
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use std::path::Path as FsPath;

/// GET /blogs
///
/// The list of all published articles, newest first.
pub async fn blogs_list() -> Html<String> {
    let posts = load_posts(FsPath::new(BLOGS_DIR));
    Html(render_list(&posts))
}

/// GET /blogs/{slug}
///
/// One article. Unknown or draft articles get a 404 page with a link
/// back to the list.
pub async fn blog_post(Path(slug): Path<String>) -> Response {
    if is_valid_slug(&slug) {
        let posts = load_posts(FsPath::new(BLOGS_DIR));
        if let Some(post) = posts.iter().find(|p| p.slug == slug) {
            return Html(render_post(post, &posts)).into_response();
        }
    }
    (
        StatusCode::NOT_FOUND,
        Html(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Article not found | Safely</title>\
             <meta name=\"robots\" content=\"noindex\"></head><body style=\"font-family:sans-serif;padding:60px;text-align:center\">\
             <h1>Article not found</h1><p><a href=\"/blogs\">See all articles</a></p></body></html>"
                .to_string(),
        ),
    )
        .into_response()
}

/// GET /sitemap.xml
///
/// Built fresh from the page list and the article files, so a new
/// article is in the sitemap as soon as it is published.
pub async fn sitemap() -> Response {
    let posts = load_posts(FsPath::new(BLOGS_DIR));
    (
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        render_sitemap(&posts),
    )
        .into_response()
}
