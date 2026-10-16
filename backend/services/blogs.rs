//! Blog articles: Markdown files turned into pages, one folder per language.
//!
//! ```text
//! content/blogs/en/<name>.md     ->  safely.sh/blogs/<name>
//! content/blogs/pt-br/<name>.md  ->  safely.sh/pt-br/blogs/<name>
//! ```
//!
//! The same file name in two folders is the same article in two
//! languages. That is how the language menu knows where to go.
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
//!
//! The page text around the article (menu, buttons, footer) is English
//! in ENGLISH below. Other languages take it from site/locales/<code>.json,
//! the same file the rest of the site uses. A missing key falls back
//! to English, so a half-translated language never shows an empty spot.
//!
//! To add a language: make the folder content/blogs/<code>/ and the
//! file site/locales/<code>.json. No code change is needed.

use crate::models::blogs::BlogPost;
use chrono::{Datelike, NaiveDate};
use pulldown_cmark::{Options, Parser, html};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const SITE_URL: &str = "https://safely.sh";
pub const BLOGS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../content/blogs");
pub const LOCALES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../site/locales");
pub const DEFAULT_LANG: &str = "en";
const WORDS_PER_MINUTE: usize = 200;
const MORE_ARTICLES: usize = 3;
/// The list page shows 2 rows of 3 first, then 1 more row per click.
pub const FIRST_CARDS: usize = 6;

const POST_TEMPLATE: &str = include_str!("../../site/templates/blog_post.html");
const LIST_TEMPLATE: &str = include_str!("../../site/templates/blogs.html");

/// Pages that are always in the sitemap, besides the blog pages.
/// The blog list pages and articles are added for every language.
pub const SITEMAP_PAGES: &[&str] = &[
    "/",
    "/partners",
    "/subscribe",
    "/vs-norton",
    "/vs-manual-research",
    "/vs-basic-reviews",
    "/vs-other-extensions",
    "/privacy",
    "/terms",
];

/// The English page text. Keys that already exist on the other pages
/// (nav.*, footer.*, cmp.*, cta.*) use the same English words here.
const ENGLISH: &[(&str, &str)] = &[
    ("nav.home", "Home"),
    ("nav.pricing", "Pricing"),
    ("nav.faq", "FAQ"),
    ("nav.partners", "Partners"),
    ("nav.blog", "Blogs"),
    ("try.nav_btn", "Add to Chrome"),
    ("blog.lang_label", "Choose language"),
    ("blog.list_title_tag", "Safely Blog: Guides for B2B Buyers"),
    (
        "blog.list_description",
        "Guides for B2B buyers: how to check a supplier, spot fake listings and avoid payment scams before you send money.",
    ),
    ("blog.badge", "Blogs"),
    (
        "blog.list_title",
        "Guides for <span class=\"u\">B2B buyers</span>",
    ),
    (
        "blog.list_sub",
        "How to check a supplier, spot a fake listing and avoid payment scams, in plain language.",
    ),
    ("blog.empty", "The first articles are on their way."),
    ("blog.show_more", "Show more articles"),
    ("blog.badge_guide", "Guide"),
    ("blog.min_read", "min read"),
    ("blog.more", "More articles"),
    ("blog.read", "Read the article →"),
    ("blog.not_found", "Article not found"),
    ("blog.see_all", "See all articles"),
    (
        "blog.months",
        "January,February,March,April,May,June,July,August,September,October,November,December",
    ),
    ("blog.date_format", "{month} {day}, {year}"),
    ("blog.cta_title", "Check your next supplier first."),
    (
        "blog.cta_sub",
        "Safely reads the supplier page and shows a 0 to 100 risk score with the reasons in about 10 seconds. The free plan covers 5 scans a month, no card needed.",
    ),
    ("cmp.short_answer", "The short answer"),
    (
        "cmp.disclaimer",
        "Safely gives a risk score from 0 to 100 as guidance. It is not a guarantee, and it does not replace calling the supplier on a phone number you already had.",
    ),
    ("cmp.free_plan", "Free plan"),
    ("cmp.add_free", "Add to Chrome, it is free"),
    ("cta.sticker", "10-second install"),
    (
        "footer.tagline",
        "Fraud detection for any online marketplace. Powered by AI, protected by community. Works anywhere you buy and sell online.",
    ),
    ("footer.product", "Product"),
    ("footer.how_it_works", "How it works"),
    ("footer.features", "Features"),
    ("footer.pricing", "Pricing"),
    ("footer.chrome_extension", "Chrome Extension"),
    ("footer.api", "API"),
    ("footer.company", "Company"),
    ("footer.about", "About"),
    ("footer.partners", "Partners"),
    ("footer.subscribe", "Subscribe"),
    ("footer.blog", "Blogs"),
    ("footer.careers", "Careers"),
    ("footer.contact", "Contact"),
    ("footer.legal", "Legal"),
    ("footer.privacy_policy", "Privacy Policy"),
    ("footer.terms", "Terms of Service"),
    ("footer.cookie_policy", "Cookie Policy"),
    ("footer.report_abuse", "Report Abuse"),
    ("footer.compare", "Compare"),
    ("footer.vs_norton", "vs Norton"),
    ("footer.vs_manual", "vs Manual Research"),
    ("footer.vs_reviews", "vs Basic Reviews"),
    ("footer.vs_extensions", "vs Other Extensions"),
    ("footer.copyright", "© 2026 Safely. All rights reserved."),
    ("footer.privacy", "Privacy"),
    ("footer.terms_short", "Terms"),
    ("footer.contact_short", "Contact"),
];

/// The page text for one language.
pub struct Texts {
    words: HashMap<String, String>,
}

impl Texts {
    /// English needs no file. Other languages read site/locales/<code>.json.
    pub fn load(lang: &str, locales_dir: &Path) -> Texts {
        let mut words = HashMap::new();
        if lang != DEFAULT_LANG {
            let path = locales_dir.join(format!("{}.json", lang));
            match std::fs::read_to_string(&path) {
                Ok(source) => match serde_json::from_str::<HashMap<String, String>>(&source) {
                    Ok(map) => words = map,
                    Err(e) => eprintln!("Safely: {} is not valid JSON: {}", path.display(), e),
                },
                Err(_) => eprintln!(
                    "Safely: no text file for language {}: {}",
                    lang,
                    path.display()
                ),
            }
        }
        Texts { words }
    }

    /// The text for a key: the language file first, then English.
    /// An unknown key shows as the key itself, so it is easy to spot.
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        if let Some(word) = self.words.get(key) {
            return word;
        }
        ENGLISH
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| *v)
            .unwrap_or(key)
    }

    /// "October 16, 2026" in English, "16 de outubro de 2026" in Portuguese.
    pub fn date(&self, date: NaiveDate) -> String {
        let months: Vec<&str> = self.get("blog.months").split(',').map(str::trim).collect();
        let month = months
            .get(date.month0() as usize)
            .copied()
            .unwrap_or_default();
        self.get("blog.date_format")
            .replace("{month}", month)
            .replace("{day}", &date.day().to_string())
            .replace("{year}", &date.year().to_string())
    }
}

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

/// A language folder name: "en", "pt-br", "es".
pub fn is_valid_lang(lang: &str) -> bool {
    (2..=10).contains(&lang.len())
        && lang.chars().all(|c| c.is_ascii_lowercase() || c == '-')
        && !lang.starts_with('-')
        && !lang.ends_with('-')
}

/// "pt-br" -> "pt-BR", the form Google and browsers expect.
pub fn hreflang(lang: &str) -> String {
    match lang.split_once('-') {
        Some((language, region)) => format!("{}-{}", language, region.to_ascii_uppercase()),
        None => lang.to_string(),
    }
}

/// "pt-br" -> "PT" for the language menu, like the rest of the site.
pub fn short_label(lang: &str) -> String {
    lang.split('-').next().unwrap_or(lang).to_ascii_uppercase()
}

/// Where a language's blog lives: /blogs for English, /pt-br/blogs for Portuguese.
pub fn blog_base(lang: &str) -> String {
    if lang == DEFAULT_LANG {
        "/blogs".to_string()
    } else {
        format!("/{}/blogs", lang)
    }
}

pub fn post_path(lang: &str, slug: &str) -> String {
    format!("{}/{}", blog_base(lang), slug)
}

/// Every language that has a folder in content/blogs. English first.
pub fn available_languages(blogs_dir: &Path) -> Vec<String> {
    let mut langs: Vec<String> = std::fs::read_dir(blogs_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .filter(|name| is_valid_lang(name))
                .collect()
        })
        .unwrap_or_default();
    langs.sort_by(|a, b| (a != DEFAULT_LANG).cmp(&(b != DEFAULT_LANG)).then(a.cmp(b)));
    if !langs.iter().any(|l| l == DEFAULT_LANG) {
        langs.insert(0, DEFAULT_LANG.to_string());
    }
    langs
}

pub fn lang_dir(blogs_dir: &Path, lang: &str) -> PathBuf {
    blogs_dir.join(lang)
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
    let source = source.trim_start_matches('\u{feff}');
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

/// All published articles in one folder, newest first. A file with a
/// mistake is skipped (and logged) so one bad file never breaks the blog.
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

/// Every language with its published articles. Used to build the
/// language menu and the sitemap.
pub struct Library {
    pub languages: Vec<(String, Vec<BlogPost>)>,
}

impl Library {
    pub fn load(blogs_dir: &Path) -> Library {
        Library {
            languages: available_languages(blogs_dir)
                .into_iter()
                .map(|lang| {
                    let posts = load_posts(&lang_dir(blogs_dir, &lang));
                    (lang, posts)
                })
                .collect(),
        }
    }

    pub fn has_language(&self, lang: &str) -> bool {
        self.languages.iter().any(|(l, _)| l == lang)
    }

    pub fn posts(&self, lang: &str) -> &[BlogPost] {
        self.languages
            .iter()
            .find(|(l, _)| l == lang)
            .map(|(_, p)| p.as_slice())
            .unwrap_or(&[])
    }

    fn has_post(&self, lang: &str, slug: &str) -> bool {
        self.posts(lang).iter().any(|p| p.slug == slug)
    }
}

/// One language version of a page: where it lives, and whether it is
/// the same article (true) or only that language's blog list (false).
struct Version {
    lang: String,
    path: String,
    exact: bool,
}

fn list_versions(library: &Library) -> Vec<Version> {
    library
        .languages
        .iter()
        .map(|(lang, _)| Version {
            lang: lang.clone(),
            path: blog_base(lang),
            exact: true,
        })
        .collect()
}

fn post_versions(library: &Library, slug: &str) -> Vec<Version> {
    library
        .languages
        .iter()
        .map(|(lang, _)| {
            let exact = library.has_post(lang, slug);
            Version {
                lang: lang.clone(),
                path: if exact {
                    post_path(lang, slug)
                } else {
                    blog_base(lang)
                },
                exact,
            }
        })
        .collect()
}

/// <option>s for the language menu. Picking one opens that page.
fn lang_options(versions: &[Version], current: &str) -> String {
    versions
        .iter()
        .map(|v| {
            format!(
                "<option value=\"{path}\" data-lang=\"{lang}\" data-exact=\"{exact}\"{selected}>{label}</option>",
                path = v.path,
                lang = v.lang,
                exact = v.exact,
                selected = if v.lang == current { " selected" } else { "" },
                label = short_label(&v.lang),
            )
        })
        .collect()
}

/// <link rel="alternate"> tags that tell Google about the other
/// languages of the same page.
fn alternate_links(versions: &[Version]) -> String {
    let exact: Vec<&Version> = versions.iter().filter(|v| v.exact).collect();
    if exact.len() < 2 {
        return String::new();
    }
    let mut out: String = exact
        .iter()
        .map(|v| {
            format!(
                "<link rel=\"alternate\" hreflang=\"{}\" href=\"{}{}\" />\n        ",
                hreflang(&v.lang),
                SITE_URL,
                v.path
            )
        })
        .collect();
    if let Some(default) = exact.iter().find(|v| v.lang == DEFAULT_LANG) {
        out.push_str(&format!(
            "<link rel=\"alternate\" hreflang=\"x-default\" href=\"{}{}\" />",
            SITE_URL, default.path
        ));
    }
    out
}

/// Fills spots in a template in one pass, so text inside a value is
/// never treated as another spot.
///
/// - `{{name}}`   a value given by the code
/// - `{{t:key}}`  page text for the language, as HTML (may hold <br>, <span>)
/// - `{{a:key}}`  page text made safe for an attribute or the <title>
pub fn fill_template(template: &str, values: &[(&str, &str)], texts: &Texts) -> String {
    let mut out = String::with_capacity(template.len() + 4096);
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let name = &after[..end];
                if let Some(key) = name.strip_prefix("t:") {
                    out.push_str(texts.get(key));
                } else if let Some(key) = name.strip_prefix("a:") {
                    out.push_str(&escape_html(texts.get(key)));
                } else {
                    match values.iter().find(|(k, _)| *k == name) {
                        Some((_, v)) => out.push_str(v),
                        None => out.push_str(&rest[start..start + 2 + end + 2]),
                    }
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

/// Article data for Google (structured data), safe inside a <script>.
fn article_json_ld(post: &BlogPost, lang: &str) -> String {
    let url = format!("{}{}", SITE_URL, post_path(lang, &post.slug));
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

/// Cards linking to articles. Cards from `visible` onwards start hidden
/// and are shown by the "Show more" button.
fn post_cards(posts: &[&BlogPost], lang: &str, texts: &Texts, visible: usize) -> String {
    posts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            format!(
                "<li{hidden}><a href=\"{href}\"><span class=\"blog-card-date\">{date}</span>\
                 <span class=\"blog-card-title\">{title}</span>\
                 <span class=\"blog-card-desc\">{desc}</span>\
                 <span class=\"blog-card-more\">{read}</span></a></li>",
                hidden = if i >= visible { " hidden" } else { "" },
                href = post_path(lang, &p.slug),
                date = escape_html(&texts.date(p.date)),
                title = escape_html(&p.title),
                desc = escape_html(&p.description),
                read = texts.get("blog.read"),
            )
        })
        .collect()
}

pub fn render_post(post: &BlogPost, lang: &str, library: &Library, texts: &Texts) -> String {
    let others: Vec<&BlogPost> = library
        .posts(lang)
        .iter()
        .filter(|p| p.slug != post.slug)
        .take(MORE_ARTICLES)
        .collect();
    let more = if others.is_empty() {
        String::new()
    } else {
        format!(
            "<div class=\"cmp-block related\"><h2>{}</h2><ul class=\"blog-cards\">{}</ul></div>",
            texts.get("blog.more"),
            post_cards(&others, lang, texts, usize::MAX)
        )
    };
    let versions = post_versions(library, &post.slug);
    let canonical = format!("{}{}", SITE_URL, post_path(lang, &post.slug));
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
    let base = blog_base(lang);
    fill_template(
        POST_TEMPLATE,
        &[
            ("lang", lang),
            ("html_lang", &html_lang),
            ("title", &title),
            ("description", &description),
            ("canonical", &canonical),
            ("alternates", &alternates),
            ("lang_options", &options),
            ("blog_base", &base),
            ("date_iso", &date_iso),
            ("date_human", &date_human),
            ("reading_minutes", &minutes),
            ("short_answer", &short_answer),
            ("body", &post.body_html),
            ("more_articles", &more),
            ("json_ld", &json_ld),
        ],
        texts,
    )
}

pub fn render_list(lang: &str, library: &Library, texts: &Texts) -> String {
    let posts = library.posts(lang);
    let cards = if posts.is_empty() {
        format!("<p class=\"cmp-note\">{}</p>", texts.get("blog.empty"))
    } else {
        let all: Vec<&BlogPost> = posts.iter().collect();
        let button = if posts.len() > FIRST_CARDS {
            format!(
                "<div class=\"blog-more-row\"><button type=\"button\" class=\"btn btn-lg blog-more\" data-blog-more hidden>{}</button></div>",
                texts.get("blog.show_more")
            )
        } else {
            String::new()
        };
        format!(
            "<ul class=\"blog-cards\">{}</ul>{}",
            post_cards(&all, lang, texts, FIRST_CARDS),
            button
        )
    };
    let versions = list_versions(library);
    let canonical = format!("{}{}", SITE_URL, blog_base(lang));
    let html_lang = hreflang(lang);
    let options = lang_options(&versions, lang);
    let alternates = alternate_links(&versions);
    let base = blog_base(lang);
    fill_template(
        LIST_TEMPLATE,
        &[
            ("lang", lang),
            ("html_lang", &html_lang),
            ("canonical", &canonical),
            ("alternates", &alternates),
            ("lang_options", &options),
            ("blog_base", &base),
            ("cards", &cards),
        ],
        texts,
    )
}

/// The page shown for an article that does not exist, in the right language.
pub fn render_not_found(lang: &str, texts: &Texts) -> String {
    format!(
        "<!doctype html><html lang=\"{lang}\"><head><meta charset=\"utf-8\"><title>{title} | Safely</title>\
         <meta name=\"robots\" content=\"noindex\"></head><body style=\"font-family:sans-serif;padding:60px;text-align:center\">\
         <h1>{title}</h1><p><a href=\"{base}\">{see_all}</a></p></body></html>",
        lang = hreflang(lang),
        title = escape_html(texts.get("blog.not_found")),
        base = blog_base(lang),
        see_all = escape_html(texts.get("blog.see_all")),
    )
}

/// sitemap.xml: the fixed pages, then each language's blog list and articles.
pub fn render_sitemap(library: &Library) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for page in SITEMAP_PAGES {
        xml.push_str(&format!("  <url><loc>{}{}</loc></url>\n", SITE_URL, page));
    }
    for (lang, posts) in &library.languages {
        xml.push_str(&format!(
            "  <url><loc>{}{}</loc></url>\n",
            SITE_URL,
            blog_base(lang)
        ));
        for post in posts {
            xml.push_str(&format!(
                "  <url><loc>{}{}</loc><lastmod>{}</lastmod></url>\n",
                SITE_URL,
                post_path(lang, &post.slug),
                post.date
            ));
        }
    }
    xml.push_str("</urlset>\n");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;

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
            languages: langs.into_iter().map(|(l, p)| (l.to_string(), p)).collect(),
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
    fn windows_line_endings_and_bom_work() {
        let p = parse_post("x", &GOOD.replace('\n', "\r\n")).unwrap();
        assert_eq!(p.title, "How to check a supplier");
        let p = parse_post("x", &format!("\u{feff}{}", GOOD)).unwrap();
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
    fn slugs_and_languages() {
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
        assert!(is_valid_lang("en") && is_valid_lang("pt-br") && is_valid_lang("es"));
        for bad in [
            "",
            "e",
            "..",
            "PT",
            "pt_br",
            "-pt",
            "api/x",
            "verylonglanguage",
        ] {
            assert!(!is_valid_lang(bad), "{bad}");
        }
        assert_eq!(hreflang("pt-br"), "pt-BR");
        assert_eq!(hreflang("en"), "en");
        assert_eq!(short_label("pt-br"), "PT");
        assert_eq!(blog_base("en"), "/blogs");
        assert_eq!(post_path("pt-br", "x"), "/pt-br/blogs/x");
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
        let t = english();
        let out = fill_template(
            "<h1>{{title}}</h1>{{body}}{{unknown}}<p>{{t:nav.home}}</p><i title=\"{{a:blog.list_title}}\">",
            &[("title", "{{body}}"), ("body", "B")],
            &t,
        );
        assert_eq!(
            out,
            "<h1>{{body}}</h1>B{{unknown}}<p>Home</p><i title=\"Guides for &lt;span class=&quot;u&quot;&gt;B2B buyers&lt;/span&gt;\">"
        );
    }

    #[test]
    fn texts_fall_back_to_english() {
        let dir = std::env::temp_dir().join(format!("safely-locales-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("pt-br.json"),
            r#"{"nav.home": "Início", "blog.months": "janeiro,fevereiro,março,abril,maio,junho,julho,agosto,setembro,outubro,novembro,dezembro", "blog.date_format": "{day} de {month} de {year}"}"#,
        )
        .unwrap();
        let pt = Texts::load("pt-br", &dir);
        let missing = Texts::load("xx", &dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(pt.get("nav.home"), "Início");
        assert_eq!(pt.get("nav.faq"), "FAQ", "missing key uses English");
        let day = NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
        assert_eq!(pt.date(day), "16 de outubro de 2026");
        assert_eq!(english().date(day), "October 16, 2026");
        assert_eq!(missing.get("nav.home"), "Home", "no file means English");
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
        let lib = library(vec![("en", vec![p.clone()])]);
        let html = render_post(&p, "en", &lib, &english());
        assert!(html.contains("<title>How to check a supplier | Safely</title>"));
        assert!(html.contains("https://safely.sh/blogs/how-to-check"));
        assert!(html.contains("Check the registration &amp; the contacts."));
        assert!(html.contains("October 20, 2026"));
        assert!(html.contains("\"@type\":\"Article\""));
        assert!(html.contains("<html lang=\"en\""));
        assert!(!html.contains("{{"), "a {{spot}} was left empty");
        assert!(!html.contains("More articles"), "no other articles yet");
        assert!(!html.contains("hreflang"), "only one language");
    }

    #[test]
    fn language_menu_and_google_links() {
        let en = vec![post("both", 1), post("only-english", 2)];
        let pt = vec![post("both", 1)];
        let lib = library(vec![("en", en.clone()), ("pt-br", pt)]);
        let t = english();

        // Article in both languages: menu goes to the same article, Google is told.
        let html = render_post(&en[0], "en", &lib, &t);
        assert!(html.contains("<option value=\"/blogs/both\" data-lang=\"en\" data-exact=\"true\" selected>EN</option>"));
        assert!(html.contains("<option value=\"/pt-br/blogs/both\" data-lang=\"pt-br\" data-exact=\"true\">PT</option>"));
        assert!(html.contains("hreflang=\"pt-BR\" href=\"https://safely.sh/pt-br/blogs/both\""));
        assert!(html.contains("hreflang=\"x-default\" href=\"https://safely.sh/blogs/both\""));

        // English-only article: PT goes to the Portuguese list, no Google link.
        let html = render_post(&en[1], "en", &lib, &t);
        assert!(html.contains(
            "<option value=\"/pt-br/blogs\" data-lang=\"pt-br\" data-exact=\"false\">PT</option>"
        ));
        assert!(!html.contains("hreflang"));

        // List pages link to each other.
        let list = render_list("pt-br", &lib, &t);
        assert!(list.contains("<html lang=\"pt-BR\""));
        assert!(list.contains("<link rel=\"canonical\" href=\"https://safely.sh/pt-br/blogs\""));
        assert!(list.contains("href=\"/pt-br/blogs/both\""));
        assert!(list.contains("hreflang=\"en\" href=\"https://safely.sh/blogs\""));
    }

    #[test]
    fn list_shows_six_then_a_button() {
        let t = english();
        let five: Vec<BlogPost> = (1..=5).rev().map(|d| post(&format!("a{d}"), d)).collect();
        let html = render_list("en", &library(vec![("en", five)]), &t);
        assert!(!html.contains("<li hidden>"));
        assert!(!html.contains("data-blog-more"), "no button for 6 or fewer");

        let eight: Vec<BlogPost> = (1..=8).rev().map(|d| post(&format!("a{d}"), d)).collect();
        let html = render_list("en", &library(vec![("en", eight)]), &t);
        assert_eq!(html.matches("<li><a").count(), FIRST_CARDS);
        assert_eq!(html.matches("<li hidden><a").count(), 2);
        assert!(html.contains("data-blog-more hidden>Show more articles</button>"));
        // newest first
        assert!(html.find("/blogs/a8").unwrap() < html.find("/blogs/a1").unwrap());
        assert!(!html.contains("{{"));
    }

    #[test]
    fn empty_language_and_not_found() {
        let t = english();
        let html = render_list("en", &library(vec![("en", vec![])]), &t);
        assert!(html.contains("The first articles are on their way."));
        let nf = render_not_found("pt-br", &t);
        assert!(nf.contains("href=\"/pt-br/blogs\""));
        assert!(nf.contains("noindex"));
    }

    #[test]
    fn sitemap_has_every_language() {
        let lib = library(vec![
            ("en", vec![post("newer", 20), post("older", 1)]),
            ("pt-br", vec![post("newer", 20)]),
        ]);
        let xml = render_sitemap(&lib);
        assert!(xml.contains("<loc>https://safely.sh/</loc>"));
        assert!(xml.contains("<loc>https://safely.sh/blogs</loc>"));
        assert!(xml.contains("<loc>https://safely.sh/pt-br/blogs</loc>"));
        assert!(
            xml.contains("<loc>https://safely.sh/blogs/older</loc><lastmod>2026-10-01</lastmod>")
        );
        assert!(xml.contains(
            "<loc>https://safely.sh/pt-br/blogs/newer</loc><lastmod>2026-10-20</lastmod>"
        ));
        assert!(!xml.contains("/try"));
    }

    #[test]
    fn loads_folders_and_skips_bad_files() {
        let dir = std::env::temp_dir().join(format!("safely-blog-test-{}", std::process::id()));
        for lang in ["en", "pt-br", "Bad_Folder"] {
            std::fs::create_dir_all(dir.join(lang)).unwrap();
        }
        let en = dir.join("en");
        std::fs::write(en.join("good-one.md"), GOOD).unwrap();
        std::fs::write(en.join("broken.md"), "no front matter").unwrap();
        std::fs::write(en.join("Bad Name.md"), GOOD).unwrap();
        std::fs::write(
            en.join("hidden.md"),
            GOOD.replace("---\n\n##", "draft: true\n---\n\n##"),
        )
        .unwrap();
        std::fs::write(en.join("notes.txt"), "ignore me").unwrap();
        std::fs::write(dir.join("pt-br").join("good-one.md"), GOOD).unwrap();
        std::fs::write(dir.join("stray.md"), GOOD).unwrap();
        let lib = Library::load(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        let langs: Vec<&str> = lib.languages.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(langs, vec!["en", "pt-br"]);
        let slugs: Vec<&str> = lib.posts("en").iter().map(|p| p.slug.as_str()).collect();
        assert_eq!(slugs, vec!["good-one"]);
        assert!(lib.has_language("pt-br") && !lib.has_language("es"));
    }

    #[test]
    fn the_shipped_articles_and_texts_are_valid() {
        let langs = available_languages(Path::new(BLOGS_DIR));
        assert!(langs.contains(&"en".to_string()));
        for lang in &langs {
            let dir = lang_dir(Path::new(BLOGS_DIR), lang);
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                let slug = path.file_stem().unwrap().to_str().unwrap();
                assert!(is_valid_slug(slug), "{slug}");
                let source = std::fs::read_to_string(&path).unwrap();
                parse_post(slug, &source).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            }
            if lang != DEFAULT_LANG {
                let source =
                    std::fs::read_to_string(Path::new(LOCALES_DIR).join(format!("{lang}.json")))
                        .unwrap_or_else(|_| panic!("site/locales/{lang}.json is missing"));
                let map: HashMap<String, String> = serde_json::from_str(&source).unwrap();
                for (key, _) in ENGLISH {
                    assert!(map.contains_key(*key), "{lang}.json has no \"{key}\"");
                }
            }
        }
    }
}
