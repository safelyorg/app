//! Free tools: one page per tool, one folder per language.
//!
//! "tools" is the list of tools, "tools/<name>" is one tool:
//!
//! ```text
//! the list                       ->  safely.sh/tools        safely.sh/pt-br/tools
//! content/tools/en/<name>.md     ->  safely.sh/tools/<name>
//! content/tools/pt-br/<name>.md  ->  safely.sh/pt-br/tools/<name>
//! ```
//!
//! The same file name in two folders is the same tool in two
//! languages. That is how the language menu knows where to go.
//!
//! A tool page has three parts:
//!
//! 1. The heading and the line under it: `title` and `description`
//!    in the tool's Markdown file.
//! 2. The tool itself (the box, the button and the result): an HTML
//!    file in site/templates/tool-widgets/<name>.html, listed in
//!    WIDGETS below. Its words are page text keys ({{t:key}}), so the
//!    box is translated like the rest of the page.
//! 3. The text under the tool (what it checks, how to read the result,
//!    questions): the body of the Markdown file.
//!
//! A tool's Markdown file looks like this:
//!
//! ```text
//! ---
//! title: Domain Age Checker
//! description: One line for the heading, Google results and the card.
//! date: 2026-10-20
//! ---
//!
//! ## What this tool checks
//! Normal Markdown text...
//! ```
//!
//! Optional: `draft: true` hides a tool until it is ready. A Markdown
//! file without a box in WIDGETS is also left out, so a page never
//! shows up with no tool on it.
//!
//! The page text around the tool (menu, buttons, footer) is English
//! in ENGLISH below. Other languages take it from site/locales/<code>.json,
//! the same file the rest of the site uses. A missing key falls back
//! to English, so a half-translated language never shows an empty spot.
//!
//! To add a language: make the folder content/tools/<code>/ and the
//! file site/locales/<code>.json. No code change is needed.

use crate::models::tools::Tool;
use chrono::NaiveDate;
use pulldown_cmark::{Options, Parser, html};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const SITE_URL: &str = "https://safely.sh";
pub const TOOLS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../content/tools");
pub const LOCALES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../site/locales");
pub const DEFAULT_LANG: &str = "en";
const MORE_TOOLS: usize = 3;
/// The list page shows 2 rows of 3 first, then 1 more row per click.
pub const FIRST_CARDS: usize = 6;

const TOOL_TEMPLATE: &str = include_str!("../../site/templates/tool.html");
const TOOLS_TEMPLATE: &str = include_str!("../../site/templates/tools.html");

/// The box of each tool, by its name (the Markdown file name).
/// A new tool adds one line here, for example:
/// ("domain-age-checker", include_str!("../../site/templates/tool-widgets/domain-age-checker.html")),
pub const WIDGETS: &[(&str, &str)] = &[];

/// Pages that are always in the sitemap, besides the tool pages.
/// The tool list pages and tools are added for every language.
pub const SITEMAP_PAGES: &[&str] = &["/", "/partners", "/subscribe", "/privacy", "/terms"];

/// The English page text. Keys that already exist on the other pages
/// (nav.*, footer.*, cmp.*, cta.*, modal.*) use the same English words here.
const ENGLISH: &[(&str, &str)] = &[
    ("nav.home", "Home"),
    ("nav.pricing", "Pricing"),
    ("nav.faq", "FAQ"),
    ("nav.partners", "Partners"),
    ("nav.tools", "Free Tools"),
    ("try.nav_btn", "Add to Chrome"),
    ("tools.lang_label", "Choose language"),
    ("tools.list_title_tag", "Free Supplier Check Tools | Safely"),
    (
        "tools.list_description",
        "Free tools for B2B buyers: check a supplier's website, email, phone number or bank details in seconds, before you send money.",
    ),
    ("tools.badge", "Free tools"),
    (
        "tools.list_title",
        "Free tools to <span class=\"u\">check suppliers</span>",
    ),
    (
        "tools.list_sub",
        "Check a website, an email, a phone number or bank details in seconds. Free, with no sign-up.",
    ),
    ("tools.empty", "The first free tools are on their way."),
    ("tools.show_more", "Show more tools"),
    ("tools.card_badge", "Free"),
    ("tools.use", "Use the tool →"),
    ("tools.badge_tool", "Free tool"),
    ("tools.hero_meta", "Free · No sign-up · Results in seconds"),
    ("tools.more", "More free tools"),
    ("tools.not_found", "Tool not found"),
    ("tools.see_all", "See all free tools"),
    (
        "tools.disclaimer",
        "This tool gives guidance, not a guarantee. Before you pay a new supplier, also call them on a phone number you found yourself.",
    ),
    (
        "tools.cta_title",
        "Get these checks on every supplier page.",
    ),
    (
        "tools.cta_sub",
        "Safely checks 15 signals on any supplier page and shows a 0 to 100 risk score with the reasons in about 10 seconds. The free plan covers 5 scans a month, no card needed.",
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
    ("footer.subscribe", "Free Scorecard 🎁"),
    ("footer.tools", "Free Tools"),
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
    // Pop-ups at the bottom of the page (same text as the home page)
    ("modal.about.title", "About Safely"),
    (
        "modal.about.body1",
        "Safely started after one too many friends lost money buying second-hand online. We build AI-powered fraud detection that runs quietly in your browser and warns you before money ever leaves your hands.",
    ),
    (
        "modal.about.body2",
        "Our mission is simple: make every online marketplace safe to buy from.",
    ),
    ("modal.got_it", "Got it"),
    ("modal.careers.title", "Careers"),
    (
        "modal.careers.body1",
        "We're a small, fully-remote team building trust infrastructure for online commerce, and we care a lot about protecting buyers.",
    ),
    (
        "modal.careers.body2",
        "There are no open roles listed right now, but we're always happy to hear from thoughtful people — reach out at <b>help@safely.sh</b>.",
    ),
    ("modal.contact.title", "Contact"),
    (
        "modal.contact.body1",
        "Questions, partnership ideas, or a scam you'd like us to look into? Email the team at <b>help@safely.sh</b> and we'll get back to you within one business day.",
    ),
    (
        "modal.contact.body2",
        "Pro and Business customers receive priority support.",
    ),
    ("modal.privacy.title", "Privacy Policy"),
    (
        "modal.privacy.body1",
        "We only read listing and seller data from pages you actively open on supported marketplaces. We never track your general browsing history and never sell your data to third parties.",
    ),
    (
        "modal.privacy.body2",
        "You can export or delete your account and all associated data at any time. This is a plain-language summary of our full policy.",
    ),
    ("modal.privacy.full", "Read the full privacy policy &rarr;"),
    ("modal.terms.title", "Terms of Service"),
    (
        "modal.terms.body1",
        "By using Safely you agree to use it for lawful buying and selling only. Risk scores are guidance to help you decide — not a guarantee — so always use your own judgement before paying.",
    ),
    (
        "modal.terms.body2",
        "The service is provided as-is. This is an overview of the full terms.",
    ),
    ("modal.terms.full", "Read the full terms &rarr;"),
    ("modal.cookie.title", "Cookie Policy"),
    (
        "modal.cookie.body1",
        "We use a small number of essential cookies to keep you signed in and remember your settings. We also use the Meta Pixel, which sets cookies so we can measure our Facebook and Instagram ads (for example, how many people install Safely after seeing an ad).",
    ),
    (
        "modal.cookie.body2",
        "You can clear cookies any time from your browser settings.",
    ),
    ("modal.report.title", "Report Abuse"),
    (
        "modal.report.body1",
        "Spotted a scammer, an abusive seller, or misuse of Safely? Report it directly from the extension on the seller's page, or email <b>help@safely.sh</b>.",
    ),
    (
        "modal.report.body2",
        "Reports are reviewed quickly and help protect the whole community.",
    ),
    ("modal.chrome.title", "Chrome Extension"),
    (
        "modal.chrome.body1",
        "Safely is a lightweight Chrome extension. Install it in one click and it activates automatically on any supported marketplace — no account needed to start.",
    ),
    (
        "modal.chrome.body2",
        "Support for Edge and Firefox is on the way.",
    ),
    ("modal.api.title", "API"),
    (
        "modal.api.body1",
        "The Safely API lets Business and Enterprise customers score listings and sellers programmatically, run bulk analyses, and pull fraud reports into their own tools.",
    ),
    (
        "modal.api.body2",
        "API access is included on the Enterprise plan.",
    ),
    ("modal.vs_norton.title", "Safely vs Norton"),
    (
        "modal.vs_norton.body1",
        "Norton and similar antivirus tools protect your computer — they catch malware, viruses, and malicious files trying to attack your device.",
    ),
    (
        "modal.vs_norton.body2",
        "Safely protects you from a different kind of risk: a supplier or listing that looks real but is not. Norton looks after your device and warns about scam messages and websites. Safely checks the supplier behind a B2B listing, including their registration, the listing itself and community fraud reports. They cover different things, and many people use both.",
    ),
    ("modal.vs_manual.title", "Safely vs manual research"),
    (
        "modal.vs_manual.body1",
        "Most buyers today check a listing by reading it carefully, searching the seller's name, and going with their gut. That works reasonably well for obvious scams, but it takes time and still misses those signals — recycled images, slightly-off pricing, or urgency language designed specifically to bypass exactly that kind of gut check.",
    ),
    (
        "modal.vs_manual.body2",
        "Safely runs the same checks a careful buyer would want to do, automatically, in about 10 seconds, every time you open a listing.",
    ),
    ("modal.vs_reviews.title", "Safely vs basic seller reviews"),
    (
        "modal.vs_reviews.body1",
        "Star ratings and reviews built into marketplaces are useful, but a brand-new seller account naturally has none yet — which tells you nothing either way, and is exactly the situation most scam listings are posted from.",
    ),
    (
        "modal.vs_reviews.body2",
        "Safely doesn't rely on a review history existing at all. It analyzes the listing itself — the language used, the images, the price, the seller's account age — so it can flag risk even on a seller's very first post.",
    ),
    (
        "modal.vs_extensions.title",
        "Safely vs generic browser extensions",
    ),
    (
        "modal.vs_extensions.body1",
        "Ad blockers, price trackers, and coupon extensions are built to change or add something to a page — they were never designed to evaluate whether the person selling to you is trustworthy.",
    ),
    (
        "modal.vs_extensions.body2",
        "Safely is purpose-built around one question: is this specific listing and seller safe to send money to. That's the only thing it does, which is why it can go deep on it — 15 distinct fraud signals, seller history, and a community fraud report network, all in one focused tool.",
    ),
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
}

/// Only lowercase letters, digits and dashes. Stops anyone asking for
/// files outside the tools folder (like "../../.env").
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

/// Where a language's list of tools lives: /tools for English,
/// /pt-br/tools for Portuguese.
pub fn tools_base(lang: &str) -> String {
    if lang == DEFAULT_LANG {
        "/tools".to_string()
    } else {
        format!("/{}/tools", lang)
    }
}

/// Where one tool lives: /tools/<name> for English,
/// /pt-br/tools/<name> for Portuguese.
pub fn tool_path(lang: &str, slug: &str) -> String {
    format!("{}/{}", tools_base(lang), slug)
}

/// The box of a tool, if it has one.
pub fn widget_for<'a>(widgets: &[(&str, &'a str)], slug: &str) -> Option<&'a str> {
    widgets.iter().find(|(s, _)| *s == slug).map(|(_, w)| *w)
}

/// Every language that has a folder in content/tools. English first.
pub fn available_languages(tools_dir: &Path) -> Vec<String> {
    let mut langs: Vec<String> = std::fs::read_dir(tools_dir)
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

pub fn lang_dir(tools_dir: &Path, lang: &str) -> PathBuf {
    tools_dir.join(lang)
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

/// Reads one tool file's text. Returns an error message that says
/// exactly what is wrong, so a broken file is easy to fix.
pub fn parse_tool(slug: &str, source: &str) -> Result<Tool, String> {
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
            "draft" => draft = value == "true",
            other => return Err(format!("unknown detail \"{}\"", other)),
        }
    }

    let need = |v: Option<String>, name: &str| {
        v.filter(|s| !s.is_empty())
            .ok_or_else(|| format!("\"{}\" is missing", name))
    };
    Ok(Tool {
        slug: slug.to_string(),
        title: need(title, "title")?,
        description: need(description, "description")?,
        date: date.ok_or("\"date\" is missing")?,
        body_html: markdown_to_html(body),
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

/// All published tools in one folder, newest first. A file with a
/// mistake, or a tool with no box yet, is skipped (and logged) so one
/// bad file never breaks the page.
pub fn load_tools(dir: &Path, widgets: &[(&str, &str)]) -> Vec<Tool> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        eprintln!("Safely: tools folder not found: {}", dir.display());
        return Vec::new();
    };
    let mut tools: Vec<Tool> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                return None;
            }
            let slug = path.file_stem()?.to_str()?.to_string();
            if !is_valid_slug(&slug) {
                eprintln!(
                    "Safely: tool file name must be lowercase-with-dashes: {}",
                    path.display()
                );
                return None;
            }
            let source = std::fs::read_to_string(&path).ok()?;
            match parse_tool(&slug, &source) {
                Ok(tool) if tool.draft => None,
                Ok(_) if widget_for(widgets, &slug).is_none() => {
                    eprintln!(
                        "Safely: tool {} has no box in WIDGETS yet, so it is hidden",
                        slug
                    );
                    None
                }
                Ok(tool) => Some(tool),
                Err(e) => {
                    eprintln!("Safely: tool file {} has a problem: {}", path.display(), e);
                    None
                }
            }
        })
        .collect();
    tools.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.slug.cmp(&b.slug)));
    tools
}

/// Every language with its published tools. Used to build the
/// language menu and the sitemap.
pub struct Library {
    pub languages: Vec<(String, Vec<Tool>)>,
}

impl Library {
    /// The real tools: content/tools and the boxes in WIDGETS.
    pub fn load(tools_dir: &Path) -> Library {
        Library::load_with(tools_dir, WIDGETS)
    }

    pub fn load_with(tools_dir: &Path, widgets: &[(&str, &str)]) -> Library {
        Library {
            languages: available_languages(tools_dir)
                .into_iter()
                .map(|lang| {
                    let tools = load_tools(&lang_dir(tools_dir, &lang), widgets);
                    (lang, tools)
                })
                .collect(),
        }
    }

    pub fn has_language(&self, lang: &str) -> bool {
        self.languages.iter().any(|(l, _)| l == lang)
    }

    pub fn tools(&self, lang: &str) -> &[Tool] {
        self.languages
            .iter()
            .find(|(l, _)| l == lang)
            .map(|(_, t)| t.as_slice())
            .unwrap_or(&[])
    }

    fn has_tool(&self, lang: &str, slug: &str) -> bool {
        self.tools(lang).iter().any(|t| t.slug == slug)
    }
}

/// One language version of a page: where it lives, and whether it is
/// the same tool (true) or only that language's tool list (false).
struct Version {
    lang: String,
    path: String,
    exact: bool,
}

fn tools_versions(library: &Library) -> Vec<Version> {
    library
        .languages
        .iter()
        .map(|(lang, _)| Version {
            lang: lang.clone(),
            path: tools_base(lang),
            exact: true,
        })
        .collect()
}

fn tool_versions(library: &Library, slug: &str) -> Vec<Version> {
    library
        .languages
        .iter()
        .map(|(lang, _)| {
            let exact = library.has_tool(lang, slug);
            Version {
                lang: lang.clone(),
                path: if exact {
                    tool_path(lang, slug)
                } else {
                    tools_base(lang)
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

/// Tool data for Google (structured data), safe inside a <script>.
/// Tells Google this page is a free web app, not an article.
fn tool_json_ld(tool: &Tool, lang: &str) -> String {
    let url = format!("{}{}", SITE_URL, tool_path(lang, &tool.slug));
    let data = serde_json::json!({
        "@context": "https://schema.org",
        "@type": "WebApplication",
        "name": tool.title,
        "description": tool.description,
        "url": url,
        "inLanguage": hreflang(lang),
        "applicationCategory": "SecurityApplication",
        "operatingSystem": "Any",
        "isAccessibleForFree": true,
        "dateModified": tool.date.to_string(),
        "image": format!("{}/images/og-image.png", SITE_URL),
        "offers": { "@type": "Offer", "price": "0", "priceCurrency": "USD" },
        "publisher": { "@type": "Organization", "name": "Safely", "url": SITE_URL }
    });
    data.to_string().replace("</", "<\\/")
}

/// Cards linking to tools. Cards from `visible` onwards start hidden
/// and are shown by the "Show more tools" button.
fn tool_cards(tools: &[&Tool], lang: &str, texts: &Texts, visible: usize) -> String {
    tools
        .iter()
        .enumerate()
        .map(|(i, t)| {
            format!(
                "<li{hidden}><a href=\"{href}\"><span class=\"tool-card-badge\">{badge}</span>\
                 <span class=\"tool-card-title\">{title}</span>\
                 <span class=\"tool-card-desc\">{desc}</span>\
                 <span class=\"tool-card-more\">{use_it}</span></a></li>",
                hidden = if i >= visible { " hidden" } else { "" },
                href = tool_path(lang, &t.slug),
                badge = texts.get("tools.card_badge"),
                title = escape_html(&t.title),
                desc = escape_html(&t.description),
                use_it = texts.get("tools.use"),
            )
        })
        .collect()
}

/// One tool page. `widget` is the tool's box, still with its
/// {{t:key}} spots; they are filled here in the page's language.
pub fn render_tool(
    tool: &Tool,
    widget: &str,
    lang: &str,
    library: &Library,
    texts: &Texts,
) -> String {
    let others: Vec<&Tool> = library
        .tools(lang)
        .iter()
        .filter(|t| t.slug != tool.slug)
        .take(MORE_TOOLS)
        .collect();
    let more = if others.is_empty() {
        String::new()
    } else {
        format!(
            "<div class=\"tool-related\"><h2>{}</h2><ul class=\"tool-cards\">{}</ul></div>",
            texts.get("tools.more"),
            tool_cards(&others, lang, texts, usize::MAX)
        )
    };
    let versions = tool_versions(library, &tool.slug);
    let canonical = format!("{}{}", SITE_URL, tool_path(lang, &tool.slug));
    let title = escape_html(&tool.title);
    let description = escape_html(&tool.description);
    let json_ld = tool_json_ld(tool, lang);
    let html_lang = hreflang(lang);
    let options = lang_options(&versions, lang);
    let alternates = alternate_links(&versions);
    let base = tools_base(lang);
    let widget = fill_template(widget, &[("lang", lang)], texts);
    fill_template(
        TOOL_TEMPLATE,
        &[
            ("lang", lang),
            ("html_lang", &html_lang),
            ("slug", &tool.slug),
            ("title", &title),
            ("description", &description),
            ("canonical", &canonical),
            ("alternates", &alternates),
            ("lang_options", &options),
            ("tools_base", &base),
            ("widget", &widget),
            ("body", &tool.body_html),
            ("more_tools", &more),
            ("json_ld", &json_ld),
        ],
        texts,
    )
}

pub fn render_tools(lang: &str, library: &Library, texts: &Texts) -> String {
    let tools = library.tools(lang);
    let cards = if tools.is_empty() {
        format!("<p class=\"tool-empty\">{}</p>", texts.get("tools.empty"))
    } else {
        let all: Vec<&Tool> = tools.iter().collect();
        let button = if tools.len() > FIRST_CARDS {
            format!(
                "<div class=\"tool-more-row\"><button type=\"button\" class=\"btn btn-lg tool-more\" data-tools-more hidden>{}</button></div>",
                texts.get("tools.show_more")
            )
        } else {
            String::new()
        };
        format!(
            "<ul class=\"tool-cards\">{}</ul>{}",
            tool_cards(&all, lang, texts, FIRST_CARDS),
            button
        )
    };
    let versions = tools_versions(library);
    let canonical = format!("{}{}", SITE_URL, tools_base(lang));
    let html_lang = hreflang(lang);
    let options = lang_options(&versions, lang);
    let alternates = alternate_links(&versions);
    let base = tools_base(lang);
    fill_template(
        TOOLS_TEMPLATE,
        &[
            ("lang", lang),
            ("html_lang", &html_lang),
            ("canonical", &canonical),
            ("alternates", &alternates),
            ("lang_options", &options),
            ("tools_base", &base),
            ("cards", &cards),
        ],
        texts,
    )
}

/// The page shown for a tool that does not exist, in the right language.
pub fn render_not_found(lang: &str, texts: &Texts) -> String {
    format!(
        "<!doctype html><html lang=\"{lang}\"><head><meta charset=\"utf-8\"><title>{title} | Safely</title>\
         <meta name=\"robots\" content=\"noindex\"></head><body style=\"font-family:sans-serif;padding:60px;text-align:center\">\
         <h1>{title}</h1><p><a href=\"{base}\">{see_all}</a></p></body></html>",
        lang = hreflang(lang),
        title = escape_html(texts.get("tools.not_found")),
        base = tools_base(lang),
        see_all = escape_html(texts.get("tools.see_all")),
    )
}

/// sitemap.xml: the fixed pages, then each language's tool list and tools.
pub fn render_sitemap(library: &Library) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for page in SITEMAP_PAGES {
        xml.push_str(&format!("  <url><loc>{}{}</loc></url>\n", SITE_URL, page));
    }
    for (lang, tools) in &library.languages {
        xml.push_str(&format!(
            "  <url><loc>{}{}</loc></url>\n",
            SITE_URL,
            tools_base(lang)
        ));
        for tool in tools {
            xml.push_str(&format!(
                "  <url><loc>{}{}</loc><lastmod>{}</lastmod></url>\n",
                SITE_URL,
                tool_path(lang, &tool.slug),
                tool.date
            ));
        }
    }
    xml.push_str("</urlset>\n");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "---\ntitle: Domain Age Checker\ndescription: See how old a website is & who registered it.\ndate: 2026-10-20\n---\n\n## What it checks\nSome **bold** text.\n";
    const BOX: &str = "<form data-lang=\"{{lang}}\"><button>{{t:nav.home}}</button></form>";

    fn english() -> Texts {
        Texts::load("en", Path::new(LOCALES_DIR))
    }

    fn tool(slug: &str, day: u32) -> Tool {
        let mut t = parse_tool(slug, GOOD).unwrap();
        t.date = NaiveDate::from_ymd_opt(2026, 10, day).unwrap();
        t
    }

    fn library(langs: Vec<(&str, Vec<Tool>)>) -> Library {
        Library {
            languages: langs.into_iter().map(|(l, t)| (l.to_string(), t)).collect(),
        }
    }

    #[test]
    fn parses_a_good_file() {
        let t = parse_tool("domain-age-checker", GOOD).unwrap();
        assert_eq!(t.title, "Domain Age Checker");
        assert_eq!(t.date, NaiveDate::from_ymd_opt(2026, 10, 20).unwrap());
        assert!(t.body_html.contains("<h2>What it checks</h2>"));
        assert!(t.body_html.contains("<strong>bold</strong>"));
        assert!(!t.draft);
    }

    #[test]
    fn windows_line_endings_and_bom_work() {
        let t = parse_tool("x", &GOOD.replace('\n', "\r\n")).unwrap();
        assert_eq!(t.title, "Domain Age Checker");
        let t = parse_tool("x", &format!("\u{feff}{}", GOOD)).unwrap();
        assert_eq!(t.title, "Domain Age Checker");
    }

    #[test]
    fn clear_errors_for_broken_files() {
        assert!(
            parse_tool("x", "title: no dashes")
                .unwrap_err()
                .contains("---")
        );
        assert!(parse_tool("x", "---\ntitle: a\n").is_err());
        let no_date = GOOD.replace("date: 2026-10-20\n", "");
        assert!(parse_tool("x", &no_date).unwrap_err().contains("date"));
        let bad_date = GOOD.replace("2026-10-20", "20/10/2026");
        assert!(
            parse_tool("x", &bad_date)
                .unwrap_err()
                .contains("2026-10-20")
        );
        let typo = GOOD.replace("title:", "titel:");
        assert!(parse_tool("x", &typo).unwrap_err().contains("titel"));
    }

    #[test]
    fn colon_inside_a_value_is_kept() {
        let s = GOOD.replace("See how old", "Tip: see how old");
        assert!(
            parse_tool("x", &s)
                .unwrap()
                .description
                .starts_with("Tip: see how old")
        );
    }

    #[test]
    fn draft_is_read() {
        let s = GOOD.replace("---\n\n##", "draft: true\n---\n\n##");
        assert!(parse_tool("x", &s).unwrap().draft);
    }

    #[test]
    fn slugs_and_languages() {
        assert!(is_valid_slug("domain-age-checker-2"));
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
        assert_eq!(tools_base("en"), "/tools");
        assert_eq!(tools_base("pt-br"), "/pt-br/tools");
        assert_eq!(tool_path("pt-br", "x"), "/pt-br/tools/x");
        assert_eq!(tool_path("en", "x"), "/tools/x");
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
            "<h1>{{title}}</h1>{{body}}{{unknown}}<p>{{t:nav.home}}</p><i title=\"{{a:tools.list_title}}\">",
            &[("title", "{{body}}"), ("body", "B")],
            &t,
        );
        assert_eq!(
            out,
            "<h1>{{body}}</h1>B{{unknown}}<p>Home</p><i title=\"Free tools to &lt;span class=&quot;u&quot;&gt;check suppliers&lt;/span&gt;\">"
        );
    }

    #[test]
    fn texts_fall_back_to_english() {
        let dir = std::env::temp_dir().join(format!("safely-locales-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("pt-br.json"), r#"{"nav.home": "Início"}"#).unwrap();
        let pt = Texts::load("pt-br", &dir);
        let missing = Texts::load("xx", &dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(pt.get("nav.home"), "Início");
        assert_eq!(pt.get("nav.faq"), "FAQ", "missing key uses English");
        assert_eq!(missing.get("nav.home"), "Home", "no file means English");
    }

    #[test]
    fn rendered_page_has_everything_and_is_escaped() {
        let t = parse_tool("domain-age-checker", GOOD).unwrap();
        let lib = library(vec![("en", vec![t.clone()])]);
        let html = render_tool(&t, BOX, "en", &lib, &english());
        assert!(html.contains("<title>Domain Age Checker | Safely</title>"));
        assert!(html.contains("https://safely.sh/tools/domain-age-checker"));
        assert!(html.contains("See how old a website is &amp; who registered it."));
        assert!(html.contains("\"@type\":\"WebApplication\""));
        assert!(html.contains("<html lang=\"en\""));
        assert!(
            html.contains("<form data-lang=\"en\"><button>Home</button></form>"),
            "the box is in the page, in the page's language"
        );
        assert!(!html.contains("{{"), "a {{spot}} was left empty");
        assert!(!html.contains("More free tools"), "no other tools yet");
        assert!(!html.contains("hreflang"), "only one language");
    }

    #[test]
    fn related_tools_are_shown() {
        let lib = library(vec![("en", vec![tool("a", 3), tool("b", 2), tool("c", 1)])]);
        let html = render_tool(&lib.tools("en")[0], BOX, "en", &lib, &english());
        assert!(html.contains("More free tools"));
        assert!(html.contains("href=\"/tools/b\"") && html.contains("href=\"/tools/c\""));
        assert!(!html.contains("href=\"/tools/a\""), "not linked to itself");
    }

    #[test]
    fn language_menu_and_google_links() {
        let en = vec![tool("both", 1), tool("only-english", 2)];
        let pt = vec![tool("both", 1)];
        let lib = library(vec![("en", en.clone()), ("pt-br", pt)]);
        let t = english();

        // Tool in both languages: menu goes to the same tool, Google is told.
        let html = render_tool(&en[0], BOX, "en", &lib, &t);
        assert!(html.contains(
            "<option value=\"/tools/both\" data-lang=\"en\" data-exact=\"true\" selected>EN</option>"
        ));
        assert!(html.contains(
            "<option value=\"/pt-br/tools/both\" data-lang=\"pt-br\" data-exact=\"true\">PT</option>"
        ));
        assert!(html.contains("hreflang=\"pt-BR\" href=\"https://safely.sh/pt-br/tools/both\""));
        assert!(html.contains("hreflang=\"x-default\" href=\"https://safely.sh/tools/both\""));

        // English-only tool: PT goes to the Portuguese list, no Google link.
        let html = render_tool(&en[1], BOX, "en", &lib, &t);
        assert!(html.contains(
            "<option value=\"/pt-br/tools\" data-lang=\"pt-br\" data-exact=\"false\">PT</option>"
        ));
        assert!(!html.contains("hreflang"));

        // List pages link to each other.
        let list = render_tools("pt-br", &lib, &t);
        assert!(list.contains("<html lang=\"pt-BR\""));
        assert!(list.contains("<link rel=\"canonical\" href=\"https://safely.sh/pt-br/tools\""));
        assert!(list.contains("href=\"/pt-br/tools/both\""));
        assert!(list.contains("hreflang=\"en\" href=\"https://safely.sh/tools\""));
    }

    #[test]
    fn list_shows_six_then_a_button() {
        let t = english();
        let five: Vec<Tool> = (1..=5).rev().map(|d| tool(&format!("a{d}"), d)).collect();
        let html = render_tools("en", &library(vec![("en", five)]), &t);
        assert!(!html.contains("<li hidden>"));
        assert!(
            !html.contains("data-tools-more"),
            "no button for 6 or fewer"
        );

        let eight: Vec<Tool> = (1..=8).rev().map(|d| tool(&format!("a{d}"), d)).collect();
        let html = render_tools("en", &library(vec![("en", eight)]), &t);
        assert_eq!(html.matches("<li><a").count(), FIRST_CARDS);
        assert_eq!(html.matches("<li hidden><a").count(), 2);
        assert!(html.contains("data-tools-more hidden>Show more tools</button>"));
        // newest first
        assert!(html.find("/tools/a8").unwrap() < html.find("/tools/a1").unwrap());
        assert!(!html.contains("{{"));
    }

    #[test]
    fn empty_language_and_not_found() {
        let t = english();
        let html = render_tools("en", &library(vec![("en", vec![])]), &t);
        assert!(html.contains("The first free tools are on their way."));
        let nf = render_not_found("pt-br", &t);
        assert!(nf.contains("href=\"/pt-br/tools\""));
        assert!(nf.contains("noindex"));
    }

    #[test]
    fn sitemap_has_every_language() {
        let lib = library(vec![
            ("en", vec![tool("newer", 20), tool("older", 1)]),
            ("pt-br", vec![tool("newer", 20)]),
        ]);
        let xml = render_sitemap(&lib);
        assert!(xml.contains("<loc>https://safely.sh/</loc>"));
        assert!(xml.contains("<loc>https://safely.sh/tools</loc>"));
        assert!(xml.contains("<loc>https://safely.sh/pt-br/tools</loc>"));
        assert!(
            xml.contains("<loc>https://safely.sh/tools/older</loc><lastmod>2026-10-01</lastmod>")
        );
        assert!(xml.contains(
            "<loc>https://safely.sh/pt-br/tools/newer</loc><lastmod>2026-10-20</lastmod>"
        ));
        assert!(!xml.contains("/try"));
    }

    #[test]
    fn loads_folders_and_skips_bad_files() {
        let dir = std::env::temp_dir().join(format!("safely-tools-test-{}", std::process::id()));
        for lang in ["en", "pt-br", "Bad_Folder"] {
            std::fs::create_dir_all(dir.join(lang)).unwrap();
        }
        let en = dir.join("en");
        std::fs::write(en.join("good-one.md"), GOOD).unwrap();
        std::fs::write(en.join("no-box-yet.md"), GOOD).unwrap();
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
        let widgets: &[(&str, &str)] = &[("good-one", BOX), ("hidden", BOX), ("broken", BOX)];
        let lib = Library::load_with(&dir, widgets);
        std::fs::remove_dir_all(&dir).unwrap();
        let langs: Vec<&str> = lib.languages.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(langs, vec!["en", "pt-br"]);
        let slugs: Vec<&str> = lib.tools("en").iter().map(|t| t.slug.as_str()).collect();
        assert_eq!(
            slugs,
            vec!["good-one"],
            "no box, draft and broken are left out"
        );
        assert!(lib.has_language("pt-br") && !lib.has_language("es"));
    }

    #[test]
    fn the_shipped_tools_and_texts_are_valid() {
        let langs = available_languages(Path::new(TOOLS_DIR));
        assert!(langs.contains(&"en".to_string()));
        for (slug, widget) in WIDGETS {
            assert!(is_valid_slug(slug), "{slug}");
            assert!(!widget.trim().is_empty(), "the box of {slug} is empty");
        }
        for lang in &langs {
            let dir = lang_dir(Path::new(TOOLS_DIR), lang);
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries {
                    let path = entry.unwrap().path();
                    if path.extension().and_then(|e| e.to_str()) != Some("md") {
                        continue;
                    }
                    let slug = path.file_stem().unwrap().to_str().unwrap();
                    assert!(is_valid_slug(slug), "{slug}");
                    let source = std::fs::read_to_string(&path).unwrap();
                    parse_tool(slug, &source).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                }
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
