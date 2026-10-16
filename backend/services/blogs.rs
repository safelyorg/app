//! Blog articles: Markdown files in content/blogs/ turned into pages at
//! safely.sh/blogs, plus the list page and the sitemap.
//!
//! An article file looks like this:
//!
//! ```text
//! ---
//! title: How to check if a B2B supplier is legit
//! description: One line for Google results and link previews.
//! date: 2026-10-20
//! short_answer: The 2-3 sentence answer shown at the top.
//! ---
//!
//! ## First section
//! Normal Markdown text...
//! ```
//!
//! Optional: `draft: true` hides an article until it is ready.

use crate::models::blogs::BlogPost;
use chrono::NaiveDate;
use pulldown_cmark::{Options, Parser, html};
use std::path::Path;

pub const SITE_URL: &str = "https://safely.sh";
pub const BLOGS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../content/blogs");
const WORDS_PER_MINUTE: usize = 200;
const MORE_ARTICLES: usize = 3;

const POST_TEMPLATE: &str = include_str!("../../site/templates/blog_post.html");
const LIST_TEMPLATE: &str = include_str!("../../site/templates/blogs.html");

/// Pages that are always in the sitemap, besides the blog articles.
/// Same list as the old site/sitemap.xml, plus /blogs.
pub const SITEMAP_PAGES: &[&str] = &[
    "/",
    "/partners",
    "/subscribe",
    "/blogs",
    "/vs-norton",
    "/vs-manual-research",
    "/vs-basic-reviews",
    "/vs-other-extensions",
    "/privacy",
    "/terms",
];

/// Only lowercase letters, digits and dashes. Stops anyone asking for
/// files outside the articles folder (like "../../.env").
pub fn is_valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 120
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !slug.starts_with('-')
        && !slug.ends_with('-')
}

/// Makes text safe to put inside HTML (and inside attribute quotes).
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Reads one article file's text. Returns an error message that says
/// exactly what is wrong, so a broken file is easy to fix.
pub fn parse_post(slug: &str, source: &str) -> Result<BlogPost, String> {
    let source = source.replace("\r\n", "\n");
    let rest = source
        .strip_prefix("---\n")
        .ok_or("the file must start with a line containing only ---")?;
    let end = rest
        .find("\n---\n")
        .or_else(|| rest.strip_suffix("\n---").map(|r| r.len()))
        .ok_or("the details part must end with a line containing only ---")?;
    let (front, body) = rest.split_at(end);
    let body = body.trim_start_matches("\n---").trim_start_matches('\n');

    let mut title = None;
    let mut description = None;
    let mut date = None;
    let mut short_answer = None;
    let mut draft = false;
    for line in front.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| format!("this line has no \"key: value\": {}", line))?;
        let value = value.trim().to_string();
        match key.trim() {
            "title" => title = Some(value),
            "description" => description = Some(value),
            "date" => {
                date = Some(
                    NaiveDate::parse_from_str(&value, "%Y-%m-%d")
                        .map_err(|_| format!("date must look like 2026-10-20, got: {}", value))?,
                )
            }
            "short_answer" => short_answer = Some(value),
            "draft" => draft = value == "true",
            other => return Err(format!("unknown detail \"{}\"", other)),
        }
    }

    let need = |v: Option<String>, name: &str| {
        v.filter(|s| !s.is_empty())
            .ok_or_else(|| format!("\"{}\" is missing", name))
    };
    let body_html = markdown_to_html(body);
    Ok(BlogPost {
        slug: slug.to_string(),
        title: need(title, "title")?,
        description: need(description, "description")?,
        date: date.ok_or("\"date\" is missing")?,
        short_answer: need(short_answer, "short_answer")?,
        reading_minutes: reading_minutes(body),
        body_html,
        draft,
    })
}

pub fn markdown_to_html(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(markdown, options);
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

pub fn reading_minutes(text: &str) -> usize {
    let words = text.split_whitespace().count();
    words.div_ceil(WORDS_PER_MINUTE).max(1)
}

/// All published articles, newest first. A file with a mistake is
/// skipped (and logged) so one bad file never breaks the whole blog.
pub fn load_posts(dir: &Path) -> Vec<BlogPost> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        eprintln!("Safely: blog folder not found: {}", dir.display());
        return Vec::new();
    };
    let mut posts: Vec<BlogPost> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                return None;
            }
            let slug = path.file_stem()?.to_str()?.to_string();
            if !is_valid_slug(&slug) {
                eprintln!(
                    "Safely: blog file name must be lowercase-with-dashes: {}",
                    path.display()
                );
                return None;
            }
            let source = std::fs::read_to_string(&path).ok()?;
            match parse_post(&slug, &source) {
                Ok(post) if !post.draft => Some(post),
                Ok(_) => None,
                Err(e) => {
                    eprintln!("Safely: blog file {} has a problem: {}", path.display(), e);
                    None
                }
            }
        })
        .collect();
    posts.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.slug.cmp(&b.slug)));
    posts
}

/// Fills {{name}} spots in a template in one pass, so text inside a
/// value is never treated as another {{spot}}.
pub fn fill_template(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + 4096);
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let name = &after[..end];
                match values.iter().find(|(k, _)| *k == name) {
                    Some((_, v)) => out.push_str(v),
                    None => out.push_str(&rest[start..start + 2 + end + 2]),
                }
                rest = &after[end + 2..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn human_date(date: NaiveDate) -> String {
    date.format("%B %-d, %Y").to_string()
}

/// Article data for Google (structured data), safe inside a <script>.
fn article_json_ld(post: &BlogPost) -> String {
    let url = format!("{}/blogs/{}", SITE_URL, post.slug);
    let data = serde_json::json!({
        "@context": "https://schema.org",
        "@type": "Article",
        "headline": post.title,
        "description": post.description,
        "datePublished": post.date.to_string(),
        "dateModified": post.date.to_string(),
        "mainEntityOfPage": url,
        "image": format!("{}/images/og-image.png", SITE_URL),
        "author": { "@type": "Organization", "name": "Safely", "url": SITE_URL },
        "publisher": { "@type": "Organization", "name": "Safely", "url": SITE_URL }
    });
    data.to_string().replace("</", "<\\/")
}

/// Cards linking to other articles, for the end of an article and the list.
fn post_cards(posts: &[&BlogPost]) -> String {
    posts
        .iter()
        .map(|p| {
            format!(
                "<li><a href=\"/blogs/{slug}\"><span class=\"blog-card-date\">{date}</span>\
                 <span class=\"blog-card-title\">{title}</span>\
                 <span class=\"blog-card-desc\">{desc}</span>\
                 <span class=\"blog-card-more\">Read the article →</span></a></li>",
                slug = p.slug,
                date = escape_html(&human_date(p.date)),
                title = escape_html(&p.title),
                desc = escape_html(&p.description),
            )
        })
        .collect()
}

pub fn render_post(post: &BlogPost, all_posts: &[BlogPost]) -> String {
    let others: Vec<&BlogPost> = all_posts
        .iter()
        .filter(|p| p.slug != post.slug)
        .take(MORE_ARTICLES)
        .collect();
    let more = if others.is_empty() {
        String::new()
    } else {
        format!(
            "<div class=\"cmp-block related\"><h2>More articles</h2><ul class=\"blog-cards\">{}</ul></div>",
            post_cards(&others)
        )
    };
    let canonical = format!("{}/blogs/{}", SITE_URL, post.slug);
    let title = escape_html(&post.title);
    let description = escape_html(&post.description);
    let date_iso = post.date.to_string();
    let date_human = escape_html(&human_date(post.date));
    let minutes = post.reading_minutes.to_string();
    let short_answer = escape_html(&post.short_answer);
    let json_ld = article_json_ld(post);
    fill_template(
        POST_TEMPLATE,
        &[
            ("title", &title),
            ("description", &description),
            ("canonical", &canonical),
            ("date_iso", &date_iso),
            ("date_human", &date_human),
            ("reading_minutes", &minutes),
            ("short_answer", &short_answer),
            ("body", &post.body_html),
            ("more_articles", &more),
            ("json_ld", &json_ld),
        ],
    )
}

pub fn render_list(posts: &[BlogPost]) -> String {
    let cards = if posts.is_empty() {
        "<p class=\"cmp-note\">The first articles are on their way.</p>".to_string()
    } else {
        let all: Vec<&BlogPost> = posts.iter().collect();
        format!("<ul class=\"blog-cards\">{}</ul>", post_cards(&all))
    };
    let canonical = format!("{}/blogs", SITE_URL);
    fill_template(
        LIST_TEMPLATE,
        &[("canonical", &canonical), ("cards", &cards)],
    )
}

/// sitemap.xml: the fixed pages plus every published article.
pub fn render_sitemap(posts: &[BlogPost]) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for page in SITEMAP_PAGES {
        xml.push_str(&format!("  <url><loc>{}{}</loc></url>\n", SITE_URL, page));
    }
    for post in posts {
        xml.push_str(&format!(
            "  <url><loc>{}/blogs/{}</loc><lastmod>{}</lastmod></url>\n",
            SITE_URL, post.slug, post.date
        ));
    }
    xml.push_str("</urlset>\n");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "---\ntitle: How to check a supplier\ndescription: Seven checks.\ndate: 2026-10-20\nshort_answer: Check the registration & the contacts.\n---\n\n## First\nSome **bold** text.\n";

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
    fn windows_line_endings_work() {
        let p = parse_post("x", &GOOD.replace('\n', "\r\n")).unwrap();
        assert_eq!(p.title, "How to check a supplier");
    }

    #[test]
    fn clear_errors_for_broken_files() {
        assert!(
            parse_post("x", "title: no dashes")
                .unwrap_err()
                .contains("---")
        );
        assert!(parse_post("x", "---\ntitle: a\n").is_err());
        let no_date = GOOD.replace("date: 2026-10-20\n", "");
        assert!(parse_post("x", &no_date).unwrap_err().contains("date"));
        let bad_date = GOOD.replace("2026-10-20", "20/10/2026");
        assert!(
            parse_post("x", &bad_date)
                .unwrap_err()
                .contains("2026-10-20")
        );
        let typo = GOOD.replace("title:", "titel:");
        assert!(parse_post("x", &typo).unwrap_err().contains("titel"));
    }

    #[test]
    fn colon_inside_a_value_is_kept() {
        let s = GOOD.replace("Seven checks.", "Tip: seven checks.");
        assert_eq!(
            parse_post("x", &s).unwrap().description,
            "Tip: seven checks."
        );
    }

    #[test]
    fn draft_is_read() {
        let s = GOOD.replace("---\n\n##", "draft: true\n---\n\n##");
        assert!(parse_post("x", &s).unwrap().draft);
    }

    #[test]
    fn slugs() {
        assert!(is_valid_slug("how-to-check-a-supplier-2026"));
        for bad in [
            "",
            "../.env",
            "Has-Caps",
            "with space",
            "-start",
            "end-",
            "a/b",
            "a.md",
        ] {
            assert!(!is_valid_slug(bad), "{bad}");
        }
    }

    #[test]
    fn escaping() {
        assert_eq!(
            escape_html("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }

    #[test]
    fn template_fills_in_one_pass() {
        let out = fill_template(
            "<h1>{{title}}</h1>{{body}}{{unknown}}",
            &[("title", "{{body}}"), ("body", "B")],
        );
        assert_eq!(out, "<h1>{{body}}</h1>B{{unknown}}");
    }

    #[test]
    fn reading_time() {
        assert_eq!(reading_minutes(""), 1);
        assert_eq!(reading_minutes(&"word ".repeat(200)), 1);
        assert_eq!(reading_minutes(&"word ".repeat(201)), 2);
    }

    #[test]
    fn rendered_page_has_everything_and_is_escaped() {
        let p = parse_post("how-to-check", GOOD).unwrap();
        let html = render_post(&p, std::slice::from_ref(&p));
        assert!(html.contains("<title>How to check a supplier | Safely</title>"));
        assert!(html.contains("https://safely.sh/blogs/how-to-check"));
        assert!(html.contains("Check the registration &amp; the contacts."));
        assert!(html.contains("October 20, 2026"));
        assert!(html.contains("\"@type\":\"Article\""));
        assert!(!html.contains("{{"), "a {{spot}} was left empty");
        assert!(!html.contains("More articles"), "no other articles yet");
    }

    #[test]
    fn list_and_sitemap() {
        let a = parse_post("older", GOOD).unwrap();
        let mut b = parse_post("newer", GOOD).unwrap();
        b.date = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        let list = render_list(&[b.clone(), a.clone()]);
        assert!(list.find("/blogs/newer").unwrap() < list.find("/blogs/older").unwrap());
        assert!(!list.contains("{{"));
        let xml = render_sitemap(&[b, a]);
        assert!(xml.contains("<loc>https://safely.sh/</loc>"));
        assert!(!xml.contains("/try"));
        assert!(xml.contains("<loc>https://safely.sh/blogs</loc>"));
        assert!(
            xml.contains("<loc>https://safely.sh/blogs/newer</loc><lastmod>2026-11-01</lastmod>")
        );
    }

    #[test]
    fn loads_real_folder_and_skips_bad_files() {
        let dir = std::env::temp_dir().join(format!("safely-blog-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("good-one.md"), GOOD).unwrap();
        std::fs::write(dir.join("broken.md"), "no front matter").unwrap();
        std::fs::write(dir.join("Bad Name.md"), GOOD).unwrap();
        std::fs::write(
            dir.join("hidden.md"),
            GOOD.replace("---\n\n##", "draft: true\n---\n\n##"),
        )
        .unwrap();
        std::fs::write(dir.join("notes.txt"), "ignore me").unwrap();
        let posts = load_posts(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            posts.iter().map(|p| p.slug.as_str()).collect::<Vec<_>>(),
            vec!["good-one"]
        );
    }

    #[test]
    fn the_shipped_articles_are_valid() {
        let dir = Path::new(BLOGS_DIR);
        let files = std::fs::read_dir(dir).unwrap().count();
        assert!(files > 0);
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let slug = path.file_stem().unwrap().to_str().unwrap();
            assert!(is_valid_slug(slug), "{slug}");
            let source = std::fs::read_to_string(&path).unwrap();
            parse_post(slug, &source).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        }
    }
}
