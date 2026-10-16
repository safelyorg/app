use crate::services::blogs::{
    BLOGS_DIR, DEFAULT_LANG, LOCALES_DIR, Library, Texts, is_valid_lang, is_valid_slug,
    render_blog, render_blogs, render_not_found, render_sitemap,
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
    blogs_page(DEFAULT_LANG)
}

/// GET /{lang}/blogs   (for example /pt-br/blogs)
///
/// The list of articles in another language.
pub async fn blogs_list_lang(Path(lang): Path<String>) -> Response {
    if lang == DEFAULT_LANG {
        return Redirect::permanent("/blogs").into_response();
    }
    blogs_page(&lang)
}

/// GET /blog/{slug}
///
/// One English article.
pub async fn blog(Path(slug): Path<String>) -> Response {
    blog_page(DEFAULT_LANG, &slug)
}

/// GET /{lang}/blog/{slug}   (for example /pt-br/blog/how-to-check)
///
/// One article in another language.
pub async fn blog_lang(Path((lang, slug)): Path<(String, String)>) -> Response {
    if lang == DEFAULT_LANG {
        return Redirect::permanent(&format!("/blog/{}", slug)).into_response();
    }
    blog_page(&lang, &slug)
}

/// GET /blogs/{slug}
///
/// The old address of an English article. Sends Google and old links
/// to the new one: /blog/{slug}.
pub async fn old_blog(Path(slug): Path<String>) -> Response {
    if !is_valid_slug(&slug) {
        return not_found(DEFAULT_LANG);
    }
    Redirect::permanent(&format!("/blog/{}", slug)).into_response()
}

/// GET /{lang}/blogs/{slug}
///
/// The old address of an article in another language. Sends it to
/// /{lang}/blog/{slug}.
pub async fn old_blog_lang(Path((lang, slug)): Path<(String, String)>) -> Response {
    if !is_valid_lang(&lang) || !is_valid_slug(&slug) {
        return not_found(DEFAULT_LANG);
    }
    Redirect::permanent(&format!("/{}/blog/{}", lang, slug)).into_response()
}

/// GET /blog
///
/// The address without an article name: show the list instead.
pub async fn blog_without_name() -> Response {
    Redirect::permanent("/blogs").into_response()
}

/// GET /{lang}/blog
///
/// Same, in another language.
pub async fn blog_without_name_lang(Path(lang): Path<String>) -> Response {
    if !is_valid_lang(&lang) {
        return not_found(DEFAULT_LANG);
    }
    Redirect::permanent(&format!("/{}/blogs", lang)).into_response()
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

fn blogs_page(lang: &str) -> Response {
    let library = Library::load(FsPath::new(BLOGS_DIR));
    if !is_valid_lang(lang) || !library.has_language(lang) {
        return not_found(DEFAULT_LANG);
    }
    let texts = Texts::load(lang, FsPath::new(LOCALES_DIR));
    Html(render_blogs(lang, &library, &texts)).into_response()
}

/// Unknown, draft or missing articles get a 404 page with a link back
/// to the list in the same language.
fn blog_page(lang: &str, slug: &str) -> Response {
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
            Html(render_blog(post, lang, &library, &texts)).into_response()
        }
        None => not_found(lang),
    }
}

fn not_found(lang: &str) -> Response {
    let texts = Texts::load(lang, FsPath::new(LOCALES_DIR));
    (StatusCode::NOT_FOUND, Html(render_not_found(lang, &texts))).into_response()
}
