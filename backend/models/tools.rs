use chrono::NaiveDate;

/// One free tool page, read from a Markdown file in content/tools/.
#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    /// From the file name: "domain-age-checker.md" -> "domain-age-checker".
    /// This is the address: safely.sh/tools/<slug>
    pub slug: String,
    /// The tool's name, shown as the page heading and on its card.
    pub title: String,
    /// One line under the heading. Also shown in Google results,
    /// link previews and on the tool's card.
    pub description: String,
    /// The day the tool was added or last changed. Newest tools come
    /// first on the list, and Google sees it in the sitemap.
    pub date: NaiveDate,
    /// The text under the tool (what it checks, how to read the
    /// result, questions), already turned from Markdown into HTML.
    pub body_html: String,
    /// `draft: true` in the file hides the tool everywhere.
    pub draft: bool,
}
