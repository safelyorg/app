use crate::services::features::tools::{
    CONTENT_DIR, DEFAULT_LANG, LOCALES_DIR, Library, NOT_FOUND_PAGE, Texts, WIDGETS, is_valid_lang,
    is_valid_slug, render_sitemap, render_tool, render_tools, widget_for,
};
use axum::{
    extract::Path,
    http::{StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use std::path::Path as FsPath;

/// GET /tools
///
/// The English "Free Tools" page: the tools, and the blogs behind the
/// "Blogs" switch, newest first.
pub async fn tools_list() -> Response {
    tools_page(DEFAULT_LANG)
}

/// GET /{lang}/tools   (for example /pt-br/tools)
///
/// The "Free Tools" page in another language.
pub async fn tools_list_lang(Path(lang): Path<String>) -> Response {
    if lang == DEFAULT_LANG {
        return Redirect::permanent("/tools").into_response();
    }
    tools_page(&lang)
}

/// GET /tools/{slug}
///
/// One English tool.
pub async fn tool(Path(slug): Path<String>) -> Response {
    tool_page(DEFAULT_LANG, &slug)
}

/// GET /{lang}/tools/{slug}   (for example /pt-br/tools/domain-age-checker)
///
/// One tool in another language.
pub async fn tool_lang(Path((lang, slug)): Path<(String, String)>) -> Response {
    if lang == DEFAULT_LANG {
        return Redirect::permanent(&format!("/tools/{}", slug)).into_response();
    }
    tool_page(&lang, &slug)
}

/// GET /sitemap.xml
///
/// Built fresh from the page list and the tool and blog files, so a
/// new tool or blog is in the sitemap as soon as it is published.
pub async fn sitemap() -> Response {
    let library = Library::load(FsPath::new(CONTENT_DIR));
    (
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        render_sitemap(&library),
    )
        .into_response()
}

fn tools_page(lang: &str) -> Response {
    let library = Library::load(FsPath::new(CONTENT_DIR));
    if !is_valid_lang(lang) || !library.has_language(lang) {
        return not_found();
    }
    let texts = Texts::load(lang, FsPath::new(LOCALES_DIR));
    Html(render_tools(lang, &library, &texts)).into_response()
}

/// Unknown, draft or missing tools get the site's 404 page.
fn tool_page(lang: &str, slug: &str) -> Response {
    let library = Library::load(FsPath::new(CONTENT_DIR));
    if !is_valid_lang(lang) || !library.has_language(lang) {
        return not_found();
    }
    let found = is_valid_slug(slug)
        .then(|| library.tools(lang).iter().find(|t| t.slug == slug))
        .flatten();
    let widget = widget_for(WIDGETS, slug);
    match (found, widget) {
        (Some(tool), Some(widget)) => {
            let texts = Texts::load(lang, FsPath::new(LOCALES_DIR));
            Html(render_tool(tool, widget, lang, &library, &texts)).into_response()
        }
        _ => not_found(),
    }
}

/// The same "page not found" page as the rest of the site.
fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Html(NOT_FOUND_PAGE)).into_response()
}
