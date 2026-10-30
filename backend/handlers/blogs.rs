use crate::services::features::blogs::render_blog;
use crate::services::features::tools::{
    CONTENT_DIR, DEFAULT_LANG, LOCALES_DIR, Library, NOT_FOUND_PAGE, Texts, is_valid_lang,
    is_valid_slug,
};
use axum::{
    extract::Path,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use std::path::Path as FsPath;

/// GET /blog/{slug}
///
/// One English blog.
pub async fn blog(Path(slug): Path<String>) -> Response {
    blog_page(DEFAULT_LANG, &slug)
}

/// GET /{lang}/blog/{slug}   (for example /pt-br/blog/how-to-check)
///
/// One blog in another language.
pub async fn blog_lang(Path((lang, slug)): Path<(String, String)>) -> Response {
    if lang == DEFAULT_LANG {
        return Redirect::permanent(&format!("/blog/{}", slug)).into_response();
    }
    blog_page(&lang, &slug)
}

/// Unknown, draft or missing blogs get the site's 404 page.
fn blog_page(lang: &str, slug: &str) -> Response {
    let library = Library::load(FsPath::new(CONTENT_DIR));
    if !is_valid_lang(lang) || !library.has_language(lang) {
        return not_found();
    }
    let found = is_valid_slug(slug)
        .then(|| library.blogs(lang).iter().find(|p| p.slug == slug))
        .flatten();
    match found {
        Some(post) => {
            let texts = Texts::load(lang, FsPath::new(LOCALES_DIR));
            Html(render_blog(post, lang, &library, &texts)).into_response()
        }
        None => not_found(),
    }
}

/// The same "page not found" page as the rest of the site.
fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Html(NOT_FOUND_PAGE)).into_response()
}
