use chrono::NaiveDate;

/// One article, read from a Markdown file in content/blogs/.
#[derive(Debug, Clone, PartialEq)]
pub struct BlogPost {
    /// From the file name: "how-to-check-a-supplier.md" -> "how-to-check-a-supplier".
    /// This is the address: safely.sh/blogs/<slug>
    pub slug: String,
    pub title: String,
    /// Shown in Google results and link previews.
    pub description: String,
    pub date: NaiveDate,
    /// The 2-3 sentence answer shown in the box at the top.
    pub short_answer: String,
    /// The article body, already turned from Markdown into HTML.
    pub body_html: String,
    pub reading_minutes: usize,
    /// `draft: true` in the file hides the article everywhere.
    pub draft: bool,
}
