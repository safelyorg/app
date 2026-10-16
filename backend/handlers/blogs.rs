use crate::services::blogs::{
    BLOGS_DIR, DEFAULT_LANG, LOCALES_DIR, Library, Texts, is_valid_lang, is_valid_slug,
    render_list, render_not_found, render_post, render_sitemap,
};
use axum::{
    extract::Path,
    http::{StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use std::path::Path as FsPath;

/// GET /blogs
///
/// The English list of articles, newest first.
pub async fn blogs_list() -> Response {
    list_page(DEFAULT_LANG)
}

/// GET /{lang}/blogs   (for example /pt-br/blogs)
///
/// The list of articles in another language.
pub async fn blogs_list_lang(Path(lang): Path<String>) -> Response {
    if lang == DEFAULT_LANG {
        return Redirect::permanent("/blogs").into_response();
    }
    list_page(&lang)
}

/// GET /blogs/{slug}
///
/// One English article.
pub async fn blog_post(Path(slug): Path<String>) -> Response {
    post_page(DEFAULT_LANG, &slug)
}

/// GET /{lang}/blogs/{slug}   (for example /pt-br/blogs/how-to-check)
///
/// One article in another language.
pub async fn blog_post_lang(Path((lang, slug)): Path<(String, String)>) -> Response {
    if lang == DEFAULT_LANG {
        return Redirect::permanent(&format!("/blogs/{}", slug)).into_response();
    }
    post_page(&lang, &slug)
}

/// GET /sitemap.xml
///
/// Built fresh from the page list and the article files, so a new
/// article is in the sitemap as soon as it is published.
pub async fn sitemap() -> Response {
    let library = Library::load(FsPath::new(BLOGS_DIR));
    (
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        render_sitemap(&library),
    )
        .into_response()
}

fn list_page(lang: &str) -> Response {
    let library = Library::load(FsPath::new(BLOGS_DIR));
    if !is_valid_lang(lang) || !library.has_language(lang) {
        return not_found(DEFAULT_LANG);
    }
    let texts = Texts::load(lang, FsPath::new(LOCALES_DIR));
    Html(render_list(lang, &library, &texts)).into_response()
}

/// Unknown, draft or missing articles get a 404 page with a link back
/// to the list in the same language.
fn post_page(lang: &str, slug: &str) -> Response {
    let library = Library::load(FsPath::new(BLOGS_DIR));
    if !is_valid_lang(lang) || !library.has_language(lang) {
        return not_found(DEFAULT_LANG);
    }
    let found = is_valid_slug(slug)
        .then(|| library.posts(lang).iter().find(|p| p.slug == slug))
        .flatten();
    match found {
        Some(post) => {
            let texts = Texts::load(lang, FsPath::new(LOCALES_DIR));
            Html(render_post(post, lang, &library, &texts)).into_response()
        }
        None => not_found(lang),
    }
}

fn not_found(lang: &str) -> Response {
    let texts = Texts::load(lang, FsPath::new(LOCALES_DIR));
    (StatusCode::NOT_FOUND, Html(render_not_found(lang, &texts))).into_response()
}
