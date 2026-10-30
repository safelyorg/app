//! Blogs: Markdown files turned into pages, one folder per language.
//!
//! They are listed on the "Free Tools" page, under the "Blogs" switch
//! (see services/tools.rs). Each blog has its own page:
//!
//! ```text
//! content/blogs/en/<name>.md     ->  safely.sh/blog/<name>
//! content/blogs/pt-br/<name>.md  ->  safely.sh/pt-br/blog/<name>
//! ```
//!
//! A blog file looks like this:
//!
//! ```text
//! ---
//! title: How to check if a B2B supplier is legit
//! description: One line for Google results, link previews and the card.
//! date: 2026-10-20
//! short_answer: The 2-3 sentence answer shown at the top.
//! ---
//!
//! ## First section
//! Normal Markdown text...
//! ```
//!
//! Optional: `draft: true` hides a blog until it is ready.

use crate::models::blogs::BlogPost;
use crate::services::features::tools::{
    BLOGS_TAB, DEFAULT_LANG, Library, MORE_ITEMS, SITE_URL, Texts, Version, alternate_links,
    blog_cards, escape_html, fill_template, hreflang, lang_options, markdown_files,
    markdown_to_html, parse_date, related, required, split_front_matter, tools_base,
};
use std::path::Path;

const WORDS_PER_MINUTE: usize = 200;
const BLOG_TEMPLATE: &str = include_str!("../../../site/templates/blog.html");

/// Where one blog lives: /blog/<name> for English,
/// /pt-br/blog/<name> for Portuguese.
pub fn blog_path(lang: &str, slug: &str) -> String {
    if lang == DEFAULT_LANG {
        format!("/blog/{}", slug)
    } else {
        format!("/{}/blog/{}", lang, slug)
    }
}

/// Reads one blog file's text. Returns an error message that says
/// exactly what is wrong, so a broken file is easy to fix.
pub fn parse_post(slug: &str, source: &str) -> Result<BlogPost, String> {
    let (details, body) = split_front_matter(source)?;
    let mut title = None;
    let mut description = None;
    let mut date = None;
    let mut short_answer = None;
    let mut draft = false;
    for (key, value) in details {
        match key.as_str() {
            "title" => title = Some(value),
            "description" => description = Some(value),
            "date" => date = Some(parse_date(&value)?),
            "short_answer" => short_answer = Some(value),
            "draft" => draft = value == "true",
            other => return Err(format!("unknown detail \"{}\"", other)),
        }
    }
    Ok(BlogPost {
        slug: slug.to_string(),
        title: required(title, "title")?,
        description: required(description, "description")?,
        date: date.ok_or("\"date\" is missing")?,
        short_answer: required(short_answer, "short_answer")?,
        reading_minutes: reading_minutes(&body),
        body_html: markdown_to_html(&body),
        draft,
    })
}

pub fn reading_minutes(text: &str) -> usize {
    let words = text.split_whitespace().count();
    words.div_ceil(WORDS_PER_MINUTE).max(1)
}

/// All published blogs in one folder, newest first. A file with a
/// mistake is skipped (and logged) so one bad file never breaks the page.
pub fn load_posts(dir: &Path) -> Vec<BlogPost> {
    let mut posts: Vec<BlogPost> = markdown_files(dir)
        .into_iter()
        .filter_map(|(slug, source, path)| match parse_post(&slug, &source) {
            Ok(post) if post.draft => None,
            Ok(post) => Some(post),
            Err(e) => {
                eprintln!("Safely: blog file {} has a problem: {}", path.display(), e);
                None
            }
        })
        .collect();
    posts.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.slug.cmp(&b.slug)));
    posts
}

/// The same blog in every language. A language without this blog
/// points to its list, on the "Blogs" side.
fn blog_versions(library: &Library, slug: &str) -> Vec<Version> {
    library
        .languages
        .iter()
        .map(|c| {
            let exact = library.has_blog(&c.lang, slug);
            Version {
                lang: c.lang.clone(),
                path: if exact {
                    blog_path(&c.lang, slug)
                } else {
                    format!("{}{}", tools_base(&c.lang), BLOGS_TAB)
                },
                exact,
            }
        })
        .collect()
}

/// Article data for Google (structured data), safe inside a <script>.
fn article_json_ld(post: &BlogPost, lang: &str) -> String {
    let url = format!("{}{}", SITE_URL, blog_path(lang, &post.slug));
    let data = serde_json::json!({
        "@context": "https://schema.org",
        "@type": "Article",
        "headline": post.title,
        "description": post.description,
        "inLanguage": hreflang(lang),
        "datePublished": post.date.to_string(),
        "dateModified": post.date.to_string(),
        "mainEntityOfPage": url,
        "image": format!("{}/images/og-image.png", SITE_URL),
        "author": { "@type": "Organization", "name": "Safely", "url": SITE_URL },
        "publisher": { "@type": "Organization", "name": "Safely", "url": SITE_URL }
    });
    data.to_string().replace("</", "<\\/")
}

/// One blog page, with up to 3 other blogs at the bottom.
pub fn render_blog(post: &BlogPost, lang: &str, library: &Library, texts: &Texts) -> String {
    let others: Vec<&BlogPost> = library
        .blogs(lang)
        .iter()
        .filter(|p| p.slug != post.slug)
        .take(MORE_ITEMS)
        .collect();
    let more = related(
        texts.get("blogs.more"),
        blog_cards(&others, lang, texts, usize::MAX),
    );
    let versions = blog_versions(library, &post.slug);
    let canonical = format!("{}{}", SITE_URL, blog_path(lang, &post.slug));
    let title = escape_html(&post.title);
    let description = escape_html(&post.description);
    let date_iso = post.date.to_string();
    let date_human = escape_html(&texts.date(post.date));
    let minutes = post.reading_minutes.to_string();
    let short_answer = escape_html(&post.short_answer);
    let json_ld = article_json_ld(post, lang);
    let html_lang = hreflang(lang);
    let options = lang_options(&versions, lang);
    let alternates = alternate_links(&versions);
    let base = tools_base(lang);
    fill_template(
        BLOG_TEMPLATE,
        &[
            ("lang", lang),
            ("html_lang", &html_lang),
            ("title", &title),
            ("description", &description),
            ("canonical", &canonical),
            ("alternates", &alternates),
            ("lang_options", &options),
            ("tools_base", &base),
            ("date_iso", &date_iso),
            ("date_human", &date_human),
            ("reading_minutes", &minutes),
            ("short_answer", &short_answer),
            ("body", &post.body_html),
            ("more_blogs", &more),
            ("json_ld", &json_ld),
        ],
        texts,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::features::tools::{LOCALES_DIR, LangContent};
    use chrono::NaiveDate;

    const GOOD: &str = "---\ntitle: How to check a supplier\ndescription: Seven checks.\ndate: 2026-10-20\nshort_answer: Check the registration & the contacts.\n---\n\n## First\nSome **bold** text.\n";

    fn english() -> Texts {
        Texts::load("en", Path::new(LOCALES_DIR))
    }

    fn post(slug: &str, day: u32) -> BlogPost {
        let mut p = parse_post(slug, GOOD).unwrap();
        p.date = NaiveDate::from_ymd_opt(2026, 10, day).unwrap();
        p
    }

    fn library(langs: Vec<(&str, Vec<BlogPost>)>) -> Library {
        Library {
            languages: langs
                .into_iter()
                .map(|(l, blogs)| LangContent {
                    lang: l.to_string(),
                    tools: vec![],
                    blogs,
                })
                .collect(),
        }
    }

    #[test]
    fn parses_a_good_file() {
        let p = parse_post("how-to-check", GOOD).unwrap();
        assert_eq!(p.title, "How to check a supplier");
        assert_eq!(p.date, NaiveDate::from_ymd_opt(2026, 10, 20).unwrap());
        assert!(p.body_html.contains("<h2>First</h2>"));
        assert!(p.body_html.contains("<strong>bold</strong>"));
        assert!(!p.draft);
        assert_eq!(p.reading_minutes, 1);
    }

    #[test]
    fn short_answer_is_needed_and_typos_are_caught() {
        let none = GOOD.replace("short_answer: Check the registration & the contacts.\n", "");
        assert!(parse_post("x", &none).unwrap_err().contains("short_answer"));
        let typo = GOOD.replace("title:", "titel:");
        assert!(parse_post("x", &typo).unwrap_err().contains("titel"));
        let draft = GOOD.replace("---\n\n##", "draft: true\n---\n\n##");
        assert!(parse_post("x", &draft).unwrap().draft);
    }

    #[test]
    fn addresses_and_reading_time() {
        assert_eq!(blog_path("en", "x"), "/blog/x");
        assert_eq!(blog_path("pt-br", "x"), "/pt-br/blog/x");
        assert_eq!(reading_minutes(""), 1);
        assert_eq!(reading_minutes(&"word ".repeat(200)), 1);
        assert_eq!(reading_minutes(&"word ".repeat(201)), 2);
    }

    #[test]
    fn rendered_page_has_everything_and_is_escaped() {
        let p = parse_post("how-to-check", GOOD).unwrap();
        let lib = library(vec![("en", vec![p.clone()])]);
        let html = render_blog(&p, "en", &lib, &english());
        assert!(html.contains("<title>How to check a supplier | Safely</title>"));
        assert!(html.contains("https://safely.sh/blog/how-to-check"));
        assert!(html.contains("Check the registration &amp; the contacts."));
        assert!(html.contains("October 20, 2026"));
        assert!(html.contains("\"@type\":\"Article\""));
        assert!(html.contains("<html lang=\"en\""));
        assert!(
            html.contains("href=\"/tools\""),
            "the menu links to Free Tools"
        );
        assert!(!html.contains("{{"), "a {{spot}} was left empty");
        assert!(!html.contains("More blogs"), "no other blogs yet");
        assert!(!html.contains("hreflang"), "only one language");
    }

    #[test]
    fn language_menu_and_more_blogs() {
        let en = vec![post("both", 3), post("only-english", 2), post("third", 1)];
        let pt = vec![post("both", 3)];
        let lib = library(vec![("en", en.clone()), ("pt-br", pt)]);
        let t = english();

        let html = render_blog(&en[0], "en", &lib, &t);
        assert!(html.contains(
            "<option value=\"/pt-br/blog/both\" data-lang=\"pt-br\" data-exact=\"true\">PT</option>"
        ));
        assert!(html.contains("hreflang=\"pt-BR\" href=\"https://safely.sh/pt-br/blog/both\""));
        assert!(html.contains("More blogs"));
        assert!(html.contains("href=\"/blog/third\""));

        // English-only blog: PT goes to the Portuguese list, Blogs side.
        let html = render_blog(&en[1], "en", &lib, &t);
        assert!(html.contains(
            "<option value=\"/pt-br/tools#blogs\" data-lang=\"pt-br\" data-exact=\"false\">PT</option>"
        ));
        assert!(!html.contains("hreflang"));
    }
}
