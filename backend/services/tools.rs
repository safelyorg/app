//! The "Free Tools" page: free tools and blogs, one folder per language.
//!
//! One page lists both, with a switch under the hero: "Free Tools"
//! (shown first) and "Blogs". Each tool and each blog has its own page:
//!
//! ```text
//! the list (both)                     ->  safely.sh/tools        safely.sh/pt-br/tools
//! content/tools/en/<name>.md          ->  safely.sh/tools/<name>
//! content/tools/pt-br/<name>.md       ->  safely.sh/pt-br/tools/<name>
//! content/blogs/en/<name>.md          ->  safely.sh/blog/<name>
//! content/blogs/pt-br/<name>.md       ->  safely.sh/pt-br/blog/<name>
//! ```
//!
//! The same file name in two language folders is the same tool (or
//! blog) in two languages. That is how the language menu knows where
//! to go. Blogs are read in services/blogs.rs; this file has the tools,
//! the list page, the sitemap and the parts both share.
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
//! image: /images/tool-banners/domain-age-checker.svg
//! ---
//!
//! ## What this tool checks
//! Normal Markdown text...
//! ```
//!
//! `image` is the banner on the tool's card (optional; a file in
//! site/images/). Optional: `draft: true` hides a tool until it is ready. A Markdown
//! file without a box in WIDGETS is also left out, so a page never
//! shows up with no tool on it.
//!
//! The page text around the content (menu, buttons, footer) is English
//! in ENGLISH below. Other languages take it from site/locales/<code>.json,
//! the same file the rest of the site uses. A missing key falls back
//! to English, so a half-translated language never shows an empty spot.
//!
//! To add a language: make the folders content/tools/<code>/ and
//! content/blogs/<code>/, and the file site/locales/<code>.json.
//! No code change is needed.

use crate::models::blogs::BlogPost;
use crate::models::tools::Tool;
use crate::services::blogs::{blog_path, load_posts};
use chrono::{Datelike, NaiveDate, Utc};
use pulldown_cmark::{Options, Parser, html};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const SITE_URL: &str = "https://safely.sh";
/// Holds the two folders: tools/ and blogs/.
pub const CONTENT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../content");
pub const LOCALES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../site/locales");
pub const DEFAULT_LANG: &str = "en";
/// How many other tools or blogs are linked at the bottom of a page.
pub const MORE_ITEMS: usize = 3;
/// Each list shows 2 rows of 3 first, then 1 more row per click.
pub const FIRST_CARDS: usize = 6;

/// Tools that are planned but not built yet, shown as "Coming soon"
/// cards (not links) under the tools on the list page, as (the slug the
/// tool will have, the start of its text keys). A card goes away by
/// itself once a tool with that slug is published. To add one, add a
/// line here and its ".title" and ".desc" texts in ENGLISH and in every
/// site/locales/<lang>.json.
const COMING_SOON: &[(&str, &str)] = &[
    ("lookalike-domain-checker", "tools.soon.lookalike"),
    ("business-email-checker", "tools.soon.email"),
    ("bank-details-checker", "tools.soon.bank"),
];

/// A tool or blog published this many days ago or less gets a "New" tag
/// (only the newest one).
const NEW_FOR_DAYS: i64 = 30;
/// The address part that opens the "Blogs" side of the list: /tools#blogs
pub const BLOGS_TAB: &str = "#blogs";

const TOOL_TEMPLATE: &str = include_str!("../../site/templates/tool.html");
const TOOLS_TEMPLATE: &str = include_str!("../../site/templates/tools.html");
/// The site's "page not found" page, shown for a tool or blog that does not exist.
pub const NOT_FOUND_PAGE: &str = include_str!("../../site/templates/404.html");

/// The box of each tool, by its name (the Markdown file name).
/// A new tool adds one entry here, like the Domain Age Checker below.
pub const WIDGETS: &[(&str, &str)] = &[(
    "domain-age-checker",
    include_str!("../../site/templates/tool_widgets/domain-age-checker.html"),
)];

/// Pages that are always in the sitemap. The list page, every tool and
/// every blog are added for each language.
pub const SITEMAP_PAGES: &[&str] = &["/", "/partners", "/subscribe", "/privacy", "/terms"];

/// The English page text. Keys that already exist on the other pages
/// (nav.*, footer.*, cmp.*, cta.*, modal.*) use the same English words here.
const ENGLISH: &[(&str, &str)] = &[
    ("nav.pricing", "Pricing"),
    ("nav.faq", "FAQ"),
    ("nav.partners", "Partners"),
    ("nav.tools", "Free Tools"),
    ("try.nav_btn", "Add to Chrome"),
    // The list page
    ("tools.lang_label", "Choose language"),
    (
        "tools.list_title_tag",
        "Free Supplier Check Tools and Guides | Safely",
    ),
    (
        "tools.list_description",
        "Free tools and plain-language guides for B2B buyers: check a supplier's website, email or bank details, and learn to spot payment scams before you send money.",
    ),
    ("tools.badge", "Free tools"),
    (
        "tools.list_title",
        "Free tools to <span class=\"u\">check suppliers</span>",
    ),
    (
        "tools.list_sub",
        "Check a website, an email or bank details in seconds, and read plain-language guides for B2B buyers. Free, with no sign-up.",
    ),
    ("tools.tabs_label", "Choose what to see"),
    ("tools.tab_tools", "Free Tools"),
    ("tools.tab_blogs", "Blogs"),
    (
        "tools.intro_tools",
        "Free checks you can run in seconds, before you pay a supplier. No sign-up.",
    ),
    (
        "tools.intro_blogs",
        "Plain-language guides for B2B buyers. Read one before your next order.",
    ),
    ("tools.new", "New"),
    // "Coming soon" cards (see COMING_SOON)
    ("tools.soon_title", "Coming soon"),
    ("tools.soon_badge", "Soon"),
    ("tools.soon.lookalike.title", "Lookalike Domain Checker"),
    (
        "tools.soon.lookalike.desc",
        "Spot fake websites that copy a real company's name with one letter changed.",
    ),
    ("tools.soon.email.title", "Business Email Checker"),
    (
        "tools.soon.email.desc",
        "See if an email address really belongs to the company it claims to be from.",
    ),
    ("tools.soon.bank.title", "Bank Details Checker"),
    (
        "tools.soon.bank.desc",
        "Check that a supplier's bank details match its country and company name.",
    ),
    // Tools
    ("tools.empty", "The first free tools are on their way."),
    ("tools.show_more", "Show more tools"),
    ("tools.card_badge", "Free"),
    ("tools.use", "Use the tool →"),
    ("tools.badge_tool", "Free tool"),
    ("tools.hero_meta", "Free · No sign-up · Results in seconds"),
    ("tools.more", "More free tools"),
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
    // Blogs
    ("blogs.empty", "The first blogs are on their way."),
    ("blogs.show_more", "Show more blogs"),
    ("blogs.read", "Read the blog →"),
    ("blogs.badge", "Blog"),
    ("blogs.min_read", "min read"),
    ("blogs.more", "More blogs"),
    ("blogs.short_answer", "The short answer"),
    (
        "blogs.disclaimer",
        "Safely gives a risk score from 0 to 100 as guidance. It is not a guarantee, and it does not replace calling the supplier on a phone number you already had.",
    ),
    (
        "blogs.months",
        "January,February,March,April,May,June,July,August,September,October,November,December",
    ),
    ("blogs.date_format", "{month} {day}, {year}"),
    // The Domain Age Checker box (site/templates/tool-widgets/domain-age-checker.html)
    ("dac.label", "Website address"),
    ("dac.placeholder", "supplier.com"),
    ("dac.button", "Check age"),
    (
        "dac.hint",
        "Paste any link or type a domain, like supplier.com. We only read the public registration record.",
    ),
    ("dac.checking", "Checking..."),
    ("dac.registered", "Registered on"),
    ("dac.expires", "Expires on"),
    ("dac.updated", "Last changed"),
    ("dac.registrar", "Registrar"),
    ("dac.domain", "Domain"),
    ("dac.not_shared", "Not shared by the registry"),
    ("dac.year", "1 year"),
    ("dac.years", "{n} years"),
    ("dac.month", "1 month"),
    ("dac.months", "{n} months"),
    ("dac.day", "1 day"),
    ("dac.days", "{n} days"),
    ("dac.and", "and"),
    ("dac.new_title", "Very new: registered {age} ago"),
    (
        "dac.new_text",
        "This domain is less than 6 months old. That is a red flag if the company says it has been in business for years. Check it more before you pay.",
    ),
    ("dac.young_title", "Fairly new: registered {age} ago"),
    (
        "dac.young_text",
        "This domain is less than 2 years old. That is not a problem by itself, but compare it with how long the company says it has been trading.",
    ),
    ("dac.established_title", "Established: registered {age} ago"),
    (
        "dac.established_text",
        "This domain is more than 2 years old, which is a good sign. Old domains can still be bought or taken over, so keep checking the other details.",
    ),
    ("dac.unknown_title", "The registration date is not public"),
    (
        "dac.unknown_text",
        "This domain's registry does not share when it was created. Check the company in its official business registry instead.",
    ),
    ("dac.expires_soon", "Expires in less than 3 months"),
    (
        "dac.err_invalid",
        "That does not look like a website address. Try something like supplier.com.",
    ),
    (
        "dac.err_not_found",
        "We could not find this domain. It may not be registered, or it may be spelled differently.",
    ),
    (
        "dac.err_busy",
        "You have run a lot of checks in the last hour. Please wait a little and try again.",
    ),
    (
        "dac.err_failed",
        "The lookup did not work this time. Please try again in a moment.",
    ),
    (
        "dac.upsell",
        "Safely checks domain age, and 14 other signals, on every supplier page automatically.",
    ),
    ("dac.upsell_link", "Add Safely to Chrome →"),
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
    ("footer.tools", "Free Tools 🛠️"),
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

    /// "October 16, 2026" in English, "16 de outubro de 2026" in Portuguese.
    pub fn date(&self, date: NaiveDate) -> String {
        let months: Vec<&str> = self.get("blogs.months").split(',').map(str::trim).collect();
        let month = months
            .get(date.month0() as usize)
            .copied()
            .unwrap_or_default();
        self.get("blogs.date_format")
            .replace("{month}", month)
            .replace("{day}", &date.day().to_string())
            .replace("{year}", &date.year().to_string())
    }
}

/// Only lowercase letters, digits and dashes. Stops anyone asking for
/// files outside the content folders (like "../../.env").
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

/// Where a language's list page lives: /tools for English,
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

/// The folders with the tools and the blogs, inside content/.
pub fn tools_dir(content_dir: &Path) -> PathBuf {
    content_dir.join("tools")
}

pub fn blogs_dir(content_dir: &Path) -> PathBuf {
    content_dir.join("blogs")
}

/// Every language that has a folder in content/tools or content/blogs.
/// English first, and always there.
pub fn available_languages(content_dir: &Path) -> Vec<String> {
    let mut langs: Vec<String> = Vec::new();
    for dir in [tools_dir(content_dir), blogs_dir(content_dir)] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if is_valid_lang(&name) && !langs.contains(&name) {
                langs.push(name);
            }
        }
    }
    langs.sort_by(|a, b| (a != DEFAULT_LANG).cmp(&(b != DEFAULT_LANG)).then(a.cmp(b)));
    if !langs.iter().any(|l| l == DEFAULT_LANG) {
        langs.insert(0, DEFAULT_LANG.to_string());
    }
    langs
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

/// Splits a Markdown file into its details (the `key: value` lines
/// between the two --- lines) and its text. Used by tools and blogs.
pub fn split_front_matter(source: &str) -> Result<(Vec<(String, String)>, String), String> {
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
    let mut details = Vec::new();
    for line in front.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| format!("this line has no \"key: value\": {}", line))?;
        details.push((key.trim().to_string(), value.trim().to_string()));
    }
    Ok((details, body.to_string()))
}

/// "2026-10-20" -> a date, with a clear message when it is written wrong.
pub fn parse_date(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("date must look like 2026-10-20, got: {}", value))
}

/// A detail that must be there and not empty.
pub fn required(value: Option<String>, name: &str) -> Result<String, String> {
    value
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("\"{}\" is missing", name))
}

/// Reads one tool file's text. Returns an error message that says
/// exactly what is wrong, so a broken file is easy to fix.
pub fn parse_tool(slug: &str, source: &str) -> Result<Tool, String> {
    let (details, body) = split_front_matter(source)?;
    let mut title = None;
    let mut description = None;
    let mut date = None;
    let mut image = None;
    let mut draft = false;
    for (key, value) in details {
        match key.as_str() {
            "title" => title = Some(value),
            "description" => description = Some(value),
            "date" => date = Some(parse_date(&value)?),
            "image" => image = Some(parse_image(&value)?),
            "draft" => draft = value == "true",
            other => return Err(format!("unknown detail \"{}\"", other)),
        }
    }
    Ok(Tool {
        slug: slug.to_string(),
        title: required(title, "title")?,
        description: required(description, "description")?,
        date: date.ok_or("\"date\" is missing")?,
        image,
        body_html: markdown_to_html(&body),
        draft,
    })
}

/// A card banner must be one of the site's own images:
/// "/images/tool-banners/domain-age-checker.svg".
///
/// The folder must not be called "tools" or "blog": an address like
/// /images/tools/x.svg looks like a tool page in another language
/// (/{lang}/tools/{name}), so the server would never send the image.
pub fn parse_image(value: &str) -> Result<String, String> {
    let second_folder = value.split('/').nth(2).unwrap_or_default();
    if second_folder == "tools" || second_folder == "blog" {
        return Err(format!(
            "image folder must not be called \"{}\" (it looks like a page address): {}",
            second_folder, value
        ));
    }
    let ok = value.starts_with("/images/")
        && !value.contains("..")
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/-_.".contains(c));
    if ok {
        Ok(value.to_string())
    } else {
        Err(format!(
            "image must be a file in site/images, like /images/tool-banners/name.svg, got: {}",
            value
        ))
    }
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

/// The Markdown files in one folder, as (name, text). A missing folder
/// is normal (nothing written in that language yet) and gives nothing.
/// A file name that is not lowercase-with-dashes is skipped and logged.
pub fn markdown_files(dir: &Path) -> Vec<(String, String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                return None;
            }
            let slug = path.file_stem()?.to_str()?.to_string();
            if !is_valid_slug(&slug) {
                eprintln!(
                    "Safely: file name must be lowercase-with-dashes: {}",
                    path.display()
                );
                return None;
            }
            let source = std::fs::read_to_string(&path).ok()?;
            Some((slug, source, path))
        })
        .collect()
}

/// All published tools in one folder, newest first. A file with a
/// mistake, or a tool with no box yet, is skipped (and logged) so one
/// bad file never breaks the page.
pub fn load_tools(dir: &Path, widgets: &[(&str, &str)]) -> Vec<Tool> {
    let mut tools: Vec<Tool> = markdown_files(dir)
        .into_iter()
        .filter_map(|(slug, source, path)| match parse_tool(&slug, &source) {
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
        })
        .collect();
    tools.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.slug.cmp(&b.slug)));
    tools
}

/// Everything published in one language.
pub struct LangContent {
    pub lang: String,
    pub tools: Vec<Tool>,
    pub blogs: Vec<BlogPost>,
}

/// Every language with its published tools and blogs. Used to build
/// the pages, the language menu and the sitemap.
pub struct Library {
    pub languages: Vec<LangContent>,
}

impl Library {
    /// The real content: content/tools, content/blogs and the boxes in WIDGETS.
    pub fn load(content_dir: &Path) -> Library {
        Library::load_with(content_dir, WIDGETS)
    }

    pub fn load_with(content_dir: &Path, widgets: &[(&str, &str)]) -> Library {
        Library {
            languages: available_languages(content_dir)
                .into_iter()
                .map(|lang| LangContent {
                    tools: load_tools(&tools_dir(content_dir).join(&lang), widgets),
                    blogs: load_posts(&blogs_dir(content_dir).join(&lang)),
                    lang,
                })
                .collect(),
        }
    }

    pub fn has_language(&self, lang: &str) -> bool {
        self.languages.iter().any(|c| c.lang == lang)
    }

    fn content(&self, lang: &str) -> Option<&LangContent> {
        self.languages.iter().find(|c| c.lang == lang)
    }

    pub fn tools(&self, lang: &str) -> &[Tool] {
        self.content(lang)
            .map(|c| c.tools.as_slice())
            .unwrap_or(&[])
    }

    pub fn blogs(&self, lang: &str) -> &[BlogPost] {
        self.content(lang)
            .map(|c| c.blogs.as_slice())
            .unwrap_or(&[])
    }

    pub fn has_tool(&self, lang: &str, slug: &str) -> bool {
        self.tools(lang).iter().any(|t| t.slug == slug)
    }

    pub fn has_blog(&self, lang: &str, slug: &str) -> bool {
        self.blogs(lang).iter().any(|p| p.slug == slug)
    }
}

/// One language version of a page: where it lives, and whether it is
/// the same page (true) or only that language's list (false).
pub struct Version {
    pub lang: String,
    pub path: String,
    pub exact: bool,
}

fn list_versions(library: &Library) -> Vec<Version> {
    library
        .languages
        .iter()
        .map(|c| Version {
            lang: c.lang.clone(),
            path: tools_base(&c.lang),
            exact: true,
        })
        .collect()
}

fn tool_versions(library: &Library, slug: &str) -> Vec<Version> {
    library
        .languages
        .iter()
        .map(|c| {
            let exact = library.has_tool(&c.lang, slug);
            Version {
                lang: c.lang.clone(),
                path: if exact {
                    tool_path(&c.lang, slug)
                } else {
                    tools_base(&c.lang)
                },
                exact,
            }
        })
        .collect()
}

/// <option>s for the language menu. Picking one opens that page.
pub fn lang_options(versions: &[Version], current: &str) -> String {
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
pub fn alternate_links(versions: &[Version]) -> String {
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

/// Cards linking to tools: the name, the line under it, the banner
/// (when the tool has one) and "Use the tool". Cards from `visible`
/// onwards start hidden and are shown by the "Show more tools" button.
fn tool_cards(tools: &[&Tool], lang: &str, texts: &Texts, visible: usize) -> String {
    tools
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let banner = t
                .image
                .as_deref()
                .map(|src| {
                    format!(
                        "<span class=\"tool-card-img\"><img src=\"{}\" alt=\"\" width=\"640\" height=\"320\" loading=\"lazy\" /></span>",
                        escape_html(src)
                    )
                })
                .unwrap_or_default();
            format!(
                "<li{hidden}><a href=\"{href}\"><span class=\"tool-card-badge\">{badge}</span>\
                 <span class=\"tool-card-title\">{title}</span>\
                 <span class=\"tool-card-desc\">{desc}</span>\
                 {banner}<span class=\"tool-card-more\">{use_it}</span></a></li>",
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

/// Cards linking to blogs, with the date in the pill.
/// The newest blog gets a "New" tag while it is recent (NEW_FOR_DAYS).
pub fn blog_cards(posts: &[&BlogPost], lang: &str, texts: &Texts, visible: usize) -> String {
    let today = Utc::now().date_naive();
    posts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let new = if i == 0 && is_new(p.date, today) {
                new_tag(texts)
            } else {
                String::new()
            };
            format!(
                "<li{hidden}><a href=\"{href}\"><span class=\"tool-badges\"><span class=\"tool-card-badge\">{date}</span>{new}</span>\
                 <span class=\"tool-card-title\">{title}</span>\
                 <span class=\"tool-card-desc\">{desc}</span>\
                 <span class=\"tool-card-meta\">{minutes} {min_read}</span>\
                 <span class=\"tool-card-more\">{read}</span></a></li>",
                hidden = if i >= visible { " hidden" } else { "" },
                href = blog_path(lang, &p.slug),
                date = escape_html(&texts.date(p.date)),
                title = escape_html(&p.title),
                desc = escape_html(&p.description),
                minutes = p.reading_minutes,
                min_read = texts.get("blogs.min_read"),
                read = texts.get("blogs.read"),
            )
        })
        .collect()
}

/// true when `date` is today or up to NEW_FOR_DAYS days before it.
pub fn is_new(date: NaiveDate, today: NaiveDate) -> bool {
    let age = (today - date).num_days();
    (0..=NEW_FOR_DAYS).contains(&age)
}

fn new_tag(texts: &Texts) -> String {
    format!(
        "<span class=\"tool-card-badge new\">{}</span>",
        texts.get("tools.new")
    )
}

/// The newest tool on the list page, as one wide card: the text and a
/// "Use the tool" button on the left, its banner on the right (on top,
/// on a phone).
fn featured_tool(t: &Tool, lang: &str, texts: &Texts, today: NaiveDate) -> String {
    let banner = t
        .image
        .as_deref()
        .map(|src| {
            format!(
                "<span class=\"tool-card-img\"><img src=\"{}\" alt=\"\" width=\"640\" height=\"320\" /></span>",
                escape_html(src)
            )
        })
        .unwrap_or_default();
    let new = if is_new(t.date, today) {
        new_tag(texts)
    } else {
        String::new()
    };
    format!(
        "<div class=\"tool-featured{no_img}\"><a href=\"{href}\"><span class=\"tool-card-text\">\
         <span class=\"tool-badges\"><span class=\"tool-card-badge\">{badge}</span>{new}</span>\
         <span class=\"tool-card-title\">{title}</span>\
         <span class=\"tool-card-desc\">{desc}</span>\
         <span class=\"tool-cta\">{use_it}</span></span>{banner}</a></div>",
        no_img = if banner.is_empty() { " no-img" } else { "" },
        href = tool_path(lang, &t.slug),
        badge = texts.get("tools.card_badge"),
        title = escape_html(&t.title),
        desc = escape_html(&t.description),
        use_it = texts.get("tools.use"),
    )
}

/// The "Coming soon" row: the planned tools (COMING_SOON) that are not
/// published yet. Plain cards, not links. Empty when all are live.
fn coming_soon(tools: &[&Tool], texts: &Texts) -> String {
    let cards: String = COMING_SOON
        .iter()
        .filter(|(slug, _)| !tools.iter().any(|t| t.slug == *slug))
        .map(|(_, key)| {
            format!(
                "<li><div class=\"tool-soon-card\"><span class=\"tool-card-badge soon\">{badge}</span>\
                 <span class=\"tool-card-title\">{title}</span>\
                 <span class=\"tool-card-desc\">{desc}</span></div></li>",
                badge = texts.get("tools.soon_badge"),
                title = texts.get(&format!("{key}.title")),
                desc = texts.get(&format!("{key}.desc")),
            )
        })
        .collect();
    if cards.is_empty() {
        return String::new();
    }
    format!(
        "<div class=\"tool-soon\"><div class=\"tool-soon-head\"><h3>{}</h3><span></span></div><ul class=\"tool-cards soon\">{}</ul></div>",
        texts.get("tools.soon_title"),
        cards
    )
}

/// One side of the list page: the cards, and a "Show more" button
/// when there are more than 6. An empty side shows a short note.
fn card_list(
    cards: String,
    count: usize,
    empty_key: &str,
    more_key: &str,
    texts: &Texts,
) -> String {
    if count == 0 {
        return format!("<p class=\"tool-empty\">{}</p>", texts.get(empty_key));
    }
    let button = if count > FIRST_CARDS {
        format!(
            "<div class=\"tool-more-row\"><button type=\"button\" class=\"btn btn-lg tool-more\" data-more hidden>{}</button></div>",
            texts.get(more_key)
        )
    } else {
        String::new()
    };
    format!("<ul class=\"tool-cards\">{}</ul>{}", cards, button)
}

/// The "More free tools" or "More blogs" row at the bottom of a page.
pub fn related(title: &str, cards: String) -> String {
    if cards.is_empty() {
        return String::new();
    }
    format!(
        "<div class=\"tool-related\"><h2>{}</h2><ul class=\"tool-cards\">{}</ul></div>",
        title, cards
    )
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
        .take(MORE_ITEMS)
        .collect();
    let more = related(
        texts.get("tools.more"),
        tool_cards(&others, lang, texts, usize::MAX),
    );
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

/// The list page: the hero, the "Free Tools / Blogs" switch, and both
/// lists. Both are in the page (so Google sees every link); the
/// "Blogs" side starts hidden and the switch shows it.
pub fn render_tools(lang: &str, library: &Library, texts: &Texts) -> String {
    let tools: Vec<&Tool> = library.tools(lang).iter().collect();
    let blogs: Vec<&BlogPost> = library.blogs(lang).iter().collect();
    let today = Utc::now().date_naive();
    // The newest tool is one wide card; the others are cards under it,
    // then the planned tools ("Coming soon").
    let tool_list = match tools.split_first() {
        None => card_list(String::new(), 0, "tools.empty", "tools.show_more", texts),
        Some((newest, rest)) => {
            let others = if rest.is_empty() {
                String::new()
            } else {
                card_list(
                    tool_cards(rest, lang, texts, FIRST_CARDS),
                    rest.len(),
                    "tools.empty",
                    "tools.show_more",
                    texts,
                )
            };
            format!("{}{}", featured_tool(newest, lang, texts, today), others)
        }
    };
    let tool_list = format!("{}{}", tool_list, coming_soon(&tools, texts));
    let blog_list = card_list(
        blog_cards(&blogs, lang, texts, FIRST_CARDS),
        blogs.len(),
        "blogs.empty",
        "blogs.show_more",
        texts,
    );
    let versions = list_versions(library);
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
            ("tool_list", &tool_list),
            ("blog_list", &blog_list),
        ],
        texts,
    )
}

/// sitemap.xml: the fixed pages, then for each language the list page,
/// every tool and every blog. A new tool or blog is in it as soon as
/// its file is on the server.
pub fn render_sitemap(library: &Library) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for page in SITEMAP_PAGES {
        xml.push_str(&format!("  <url><loc>{}{}</loc></url>\n", SITE_URL, page));
    }
    for c in &library.languages {
        xml.push_str(&format!(
            "  <url><loc>{}{}</loc></url>\n",
            SITE_URL,
            tools_base(&c.lang)
        ));
        for tool in &c.tools {
            xml.push_str(&format!(
                "  <url><loc>{}{}</loc><lastmod>{}</lastmod></url>\n",
                SITE_URL,
                tool_path(&c.lang, &tool.slug),
                tool.date
            ));
        }
        for post in &c.blogs {
            xml.push_str(&format!(
                "  <url><loc>{}{}</loc><lastmod>{}</lastmod></url>\n",
                SITE_URL,
                blog_path(&c.lang, &post.slug),
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
    use crate::services::blogs::parse_post;

    const GOOD: &str = "---\ntitle: Domain Age Checker\ndescription: See how old a website is & who registered it.\ndate: 2026-10-20\n---\n\n## What it checks\nSome **bold** text.\n";
    const POST: &str = "---\ntitle: How to check a supplier\ndescription: Seven checks.\ndate: 2026-10-16\nshort_answer: Check the registration.\n---\n\n## First\nText.\n";
    const BOX: &str = "<form data-lang=\"{{lang}}\"><button>{{t:nav.faq}}</button></form>";

    fn english() -> Texts {
        Texts::load("en", Path::new(LOCALES_DIR))
    }

    fn tool(slug: &str, day: u32) -> Tool {
        let mut t = parse_tool(slug, GOOD).unwrap();
        t.date = NaiveDate::from_ymd_opt(2026, 10, day).unwrap();
        t
    }

    fn post(slug: &str, day: u32) -> BlogPost {
        let mut p = parse_post(slug, POST).unwrap();
        p.date = NaiveDate::from_ymd_opt(2026, 10, day).unwrap();
        p
    }

    fn library(langs: Vec<(&str, Vec<Tool>, Vec<BlogPost>)>) -> Library {
        Library {
            languages: langs
                .into_iter()
                .map(|(l, tools, blogs)| LangContent {
                    lang: l.to_string(),
                    tools,
                    blogs,
                })
                .collect(),
        }
    }

    #[test]
    fn parses_a_good_tool_file() {
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
    fn card_banner_is_shown_and_checked() {
        let s = GOOD.replace(
            "---\n\n##",
            "image: /images/tool-banners/x-y_1.svg\n---\n\n##",
        );
        let t = parse_tool("x", &s).unwrap();
        assert_eq!(t.image.as_deref(), Some("/images/tool-banners/x-y_1.svg"));
        let lib = library(vec![("en", vec![t], vec![])]);
        let html = render_tools("en", &lib, &english());
        assert!(html.contains(
            "<span class=\"tool-card-img\"><img src=\"/images/tool-banners/x-y_1.svg\" alt=\"\""
        ));
        for bad in [
            "/images/tools/a.svg",
            "/images/blog/a.svg",
            "https://evil.com/a.png",
            "/images/../.env",
            "/images/a\".png",
            "images/a.png",
        ] {
            let s = GOOD.replace("---\n\n##", &format!("image: {bad}\n---\n\n##"));
            assert!(parse_tool("x", &s).is_err(), "{bad}");
        }
        let no_image = render_tools(
            "en",
            &library(vec![("en", vec![tool("y", 1)], vec![])]),
            &english(),
        );
        assert!(!no_image.contains("tool-card-img"));
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
            "<h1>{{title}}</h1>{{body}}{{unknown}}<p>{{t:nav.faq}}</p><i title=\"{{a:tools.list_title}}\">",
            &[("title", "{{body}}"), ("body", "B")],
            &t,
        );
        assert_eq!(
            out,
            "<h1>{{body}}</h1>B{{unknown}}<p>FAQ</p><i title=\"Free tools to &lt;span class=&quot;u&quot;&gt;check suppliers&lt;/span&gt;\">"
        );
    }

    #[test]
    fn texts_fall_back_to_english() {
        let dir = std::env::temp_dir().join(format!("safely-locales-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("pt-br.json"),
            r#"{"nav.faq": "Perguntas", "blogs.months": "janeiro,fevereiro,março,abril,maio,junho,julho,agosto,setembro,outubro,novembro,dezembro", "blogs.date_format": "{day} de {month} de {year}"}"#,
        )
        .unwrap();
        let pt = Texts::load("pt-br", &dir);
        let missing = Texts::load("xx", &dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(pt.get("nav.faq"), "Perguntas");
        assert_eq!(pt.get("nav.pricing"), "Pricing", "missing key uses English");
        let day = NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
        assert_eq!(pt.date(day), "16 de outubro de 2026");
        assert_eq!(english().date(day), "October 16, 2026");
        assert_eq!(missing.get("nav.faq"), "FAQ", "no file means English");
    }

    #[test]
    fn rendered_tool_page_has_everything_and_is_escaped() {
        let t = parse_tool("domain-age-checker", GOOD).unwrap();
        let lib = library(vec![("en", vec![t.clone()], vec![])]);
        let html = render_tool(&t, BOX, "en", &lib, &english());
        assert!(html.contains("<title>Domain Age Checker | Safely</title>"));
        assert!(html.contains("https://safely.sh/tools/domain-age-checker"));
        assert!(html.contains("See how old a website is &amp; who registered it."));
        assert!(html.contains("\"@type\":\"WebApplication\""));
        assert!(html.contains("<html lang=\"en\""));
        assert!(
            html.contains("<form data-lang=\"en\"><button>FAQ</button></form>"),
            "the box is in the page, in the page's language"
        );
        assert!(!html.contains("{{"), "a {{spot}} was left empty");
        assert!(!html.contains("More free tools"), "no other tools yet");
        assert!(!html.contains("hreflang"), "only one language");
    }

    #[test]
    fn related_tools_are_shown() {
        let lib = library(vec![(
            "en",
            vec![tool("a", 3), tool("b", 2), tool("c", 1)],
            vec![],
        )]);
        let html = render_tool(&lib.tools("en")[0], BOX, "en", &lib, &english());
        assert!(html.contains("More free tools"));
        assert!(html.contains("href=\"/tools/b\"") && html.contains("href=\"/tools/c\""));
        assert!(!html.contains("href=\"/tools/a\""), "not linked to itself");
    }

    #[test]
    fn language_menu_and_google_links() {
        let en = vec![tool("both", 1), tool("only-english", 2)];
        let pt = vec![tool("both", 1)];
        let lib = library(vec![("en", en.clone(), vec![]), ("pt-br", pt, vec![])]);
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
    fn list_page_has_both_sides_and_a_button_each() {
        let t = english();
        let tools: Vec<Tool> = (1..=8).rev().map(|d| tool(&format!("t{d}"), d)).collect();
        let blogs: Vec<BlogPost> = (1..=5).rev().map(|d| post(&format!("b{d}"), d)).collect();
        let html = render_tools("en", &library(vec![("en", tools, blogs)]), &t);
        // Both switch buttons, tools first and chosen
        assert!(
            html.find("data-tab=\"tools\"").unwrap() < html.find("data-tab=\"blogs\"").unwrap()
        );
        assert!(
            html.contains("id=\"blogs\""),
            "/tools#blogs opens the blogs side"
        );
        // 8 tools: the newest is the wide card, then 6 shown and 1
        // waiting, one button. 5 blogs: all shown, no button.
        assert!(html.contains("<a href=\"/tools/t8\"><span class=\"tool-card-text\">"));
        assert_eq!(html.matches("class=\"tool-featured").count(), 1);
        assert_eq!(html.matches("<li hidden><a href=\"/tools/").count(), 1);
        assert_eq!(html.matches("<li><a href=\"/tools/").count(), FIRST_CARDS);
        assert_eq!(html.matches("<li><a href=\"/blog/").count(), 5);
        assert_eq!(html.matches("data-more hidden>").count(), 1);
        assert!(html.contains("data-more hidden>Show more tools</button>"));
        assert!(html.contains("October 5, 2026"), "blog cards show the date");
        // newest first
        assert!(html.find("/tools/t8").unwrap() < html.find("/tools/t1").unwrap());
        assert!(!html.contains("{{"));
    }

    #[test]
    fn one_tool_is_the_wide_card_with_no_grid_and_no_button() {
        let t = english();
        let html = render_tools(
            "en",
            &library(vec![("en", vec![tool("only", 5)], vec![])]),
            &t,
        );
        assert!(html.contains("<a href=\"/tools/only\"><span class=\"tool-card-text\">"));
        assert_eq!(html.matches("class=\"tool-featured").count(), 1);
        assert!(!html.contains("<li><a href=\"/tools/"));
        assert!(!html.contains("data-more"));
        assert!(html.contains("<span class=\"tool-cta\">Use the tool →</span>"));
    }

    #[test]
    fn coming_soon_lists_planned_tools_until_they_are_published() {
        let t = english();
        let html = render_tools(
            "en",
            &library(vec![("en", vec![tool("only", 5)], vec![])]),
            &t,
        );
        assert!(html.contains("Coming soon"));
        assert!(html.contains("Lookalike Domain Checker"));
        assert!(html.contains("Bank Details Checker"));
        // "Coming soon" cards are not links.
        assert!(!html.contains("href=\"/tools/lookalike-domain-checker\""));
        // Once published, its "Coming soon" card goes away.
        let live = vec![tool("lookalike-domain-checker", 6), tool("only", 5)];
        let html = render_tools("en", &library(vec![("en", live, vec![])]), &t);
        assert_eq!(
            html.matches("tool-soon-card").count(),
            COMING_SOON.len() - 1
        );
        // All published: no "Coming soon" row at all.
        let all: Vec<Tool> = COMING_SOON.iter().map(|(slug, _)| tool(slug, 5)).collect();
        let html = render_tools("en", &library(vec![("en", all, vec![])]), &t);
        assert!(!html.contains("tool-soon"));
    }

    #[test]
    fn new_tag_is_only_for_recent_dates() {
        let d = |m: u32, day: u32| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        let today = d(10, 25);
        assert!(is_new(d(10, 25), today));
        assert!(is_new(d(9, 25), today), "30 days ago is still new");
        assert!(!is_new(d(9, 24), today));
        assert!(!is_new(d(10, 26), today), "a future date is not new");
    }

    #[test]
    fn blog_cards_show_the_reading_time() {
        let t = english();
        let p = post("b1", 5);
        let html = blog_cards(&[&p], "en", &t, FIRST_CARDS);
        assert!(html.contains(&format!(
            "<span class=\"tool-card-meta\">{} min read</span>",
            p.reading_minutes
        )));
    }

    #[test]
    fn empty_sides_show_a_note() {
        let t = english();
        let html = render_tools("en", &library(vec![("en", vec![], vec![])]), &t);
        assert!(html.contains("The first free tools are on their way."));
        assert!(html.contains("The first blogs are on their way."));
        assert!(
            NOT_FOUND_PAGE.contains("noindex"),
            "Google must not index the 404 page"
        );
        assert!(NOT_FOUND_PAGE.contains("href=\"/tools\""));
    }

    #[test]
    fn sitemap_has_every_tool_and_blog_in_every_language() {
        let lib = library(vec![
            (
                "en",
                vec![tool("newer", 20), tool("older", 1)],
                vec![post("guide", 16)],
            ),
            ("pt-br", vec![tool("newer", 20)], vec![post("guide", 16)]),
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
        assert!(
            xml.contains("<loc>https://safely.sh/blog/guide</loc><lastmod>2026-10-16</lastmod>")
        );
        assert!(xml.contains(
            "<loc>https://safely.sh/pt-br/blog/guide</loc><lastmod>2026-10-16</lastmod>"
        ));
        assert!(!xml.contains("/try"));
    }

    #[test]
    fn loads_folders_and_skips_bad_files() {
        let dir = std::env::temp_dir().join(format!("safely-content-test-{}", std::process::id()));
        for lang in ["en", "pt-br", "Bad_Folder"] {
            std::fs::create_dir_all(dir.join("tools").join(lang)).unwrap();
        }
        // A language that only has blogs still counts.
        std::fs::create_dir_all(dir.join("blogs").join("es")).unwrap();
        std::fs::create_dir_all(dir.join("blogs").join("en")).unwrap();
        std::fs::write(dir.join("blogs").join("en").join("a-guide.md"), POST).unwrap();
        let en = dir.join("tools").join("en");
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
        std::fs::write(dir.join("tools").join("pt-br").join("good-one.md"), GOOD).unwrap();
        let widgets: &[(&str, &str)] = &[("good-one", BOX), ("hidden", BOX), ("broken", BOX)];
        let lib = Library::load_with(&dir, widgets);
        let empty = Library::load_with(&dir.join("nothing-here"), widgets);
        std::fs::remove_dir_all(&dir).unwrap();
        let langs: Vec<&str> = lib.languages.iter().map(|c| c.lang.as_str()).collect();
        assert_eq!(langs, vec!["en", "es", "pt-br"]);
        let slugs: Vec<&str> = lib.tools("en").iter().map(|t| t.slug.as_str()).collect();
        assert_eq!(
            slugs,
            vec!["good-one"],
            "no box, draft and broken are left out"
        );
        assert_eq!(lib.blogs("en").len(), 1);
        assert!(lib.has_blog("en", "a-guide") && !lib.has_blog("pt-br", "a-guide"));
        // No content folder at all: English only, empty, no crash.
        assert_eq!(empty.languages.len(), 1);
        assert!(empty.tools("en").is_empty() && empty.blogs("en").is_empty());
    }

    #[test]
    fn the_shipped_content_and_texts_are_valid() {
        let content = Path::new(CONTENT_DIR);
        let langs = available_languages(content);
        assert!(langs.contains(&"en".to_string()));
        for (slug, widget) in WIDGETS {
            assert!(is_valid_slug(slug), "{slug}");
            assert!(!widget.trim().is_empty(), "the box of {slug} is empty");
            // Every word in the box has English text (and the language
            // files are checked against ENGLISH below).
            let filled = fill_template(
                widget,
                &[("lang", "en")],
                &Texts::load("en", Path::new(LOCALES_DIR)),
            );
            assert!(
                !filled.contains("{{"),
                "the box of {slug} has an empty spot"
            );
            for key in widget
                .split("{{t:")
                .skip(1)
                .chain(widget.split("{{a:").skip(1))
            {
                let key = key.split("}}").next().unwrap();
                assert!(
                    ENGLISH.iter().any(|(k, _)| *k == key),
                    "the box of {slug} uses {key}, which has no English text"
                );
            }
            // Every tool with a box has its page text in English.
            assert!(
                tools_dir(content)
                    .join("en")
                    .join(format!("{slug}.md"))
                    .exists(),
                "content/tools/en/{slug}.md is missing"
            );
        }
        for lang in &langs {
            for (slug, source, path) in markdown_files(&tools_dir(content).join(lang)) {
                let tool = parse_tool(&slug, &source)
                    .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                if let Some(image) = tool.image {
                    let file = Path::new(CONTENT_DIR)
                        .join("../site")
                        .join(image.trim_start_matches('/'));
                    assert!(
                        file.exists(),
                        "{}: the image {} is missing",
                        path.display(),
                        file.display()
                    );
                }
            }
            for (slug, source, path) in markdown_files(&blogs_dir(content).join(lang)) {
                parse_post(&slug, &source).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
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
