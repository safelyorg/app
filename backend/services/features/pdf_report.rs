use crate::{
    models::history::HistoryDetailResponse,
    services::{features::osint::is_web_scam_row, sources::websites::PLATFORM as WEBSITE},
};
use genpdf::{Alignment, Document, Element, Margins, SimplePageDecorator, elements, fonts, style};
use serde_json::Value;

const COLOR_BRAND: style::Color = style::Color::Rgb(111, 179, 239);
const COLOR_GOOD: style::Color = style::Color::Rgb(21, 140, 105);
const COLOR_CAUTION: style::Color = style::Color::Rgb(180, 120, 10);
const COLOR_HIGH: style::Color = style::Color::Rgb(200, 40, 40);
const COLOR_MUTED: style::Color = style::Color::Rgb(120, 120, 128);
const COLOR_INK: style::Color = style::Color::Rgb(25, 25, 30);

/// The marker the completeness checks use to carry their per-field
/// checklist ("Employee count|true;Sales revenue|false"). The extension
/// turns it into ticks and crosses; the PDF turns it into plain text.
const CHECKLIST_MARKER: &str = "###CHECKLIST###";

/// Longest piece of a web address put on one line. The PDF library
/// only wraps at spaces, so a long address with no spaces would not
/// fit the column and was left out entirely.
const URL_LINE_CHARS: usize = 60;

pub fn generate_evidence_pdf(detail: &HistoryDetailResponse) -> Result<Vec<u8>, String> {
    let font_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/fonts");
    let font_family = fonts::from_files(font_dir, "Roboto", None)
        .map_err(|e| format!("Could not load the real font files: {}", e))?;

    let mut doc = Document::new(font_family);
    doc.set_title("Safely Evidence Report");

    let mut decorator = SimplePageDecorator::new();
    decorator.set_margins(18);
    doc.set_page_decorator(decorator);

    // Brand mark
    doc.push(
        elements::Paragraph::new("SAFELY").styled(
            style::Style::new()
                .bold()
                .with_font_size(10)
                .with_color(COLOR_BRAND),
        ),
    );
    doc.push(
        elements::Paragraph::new("Evidence Report").styled(
            style::Style::new()
                .bold()
                .with_font_size(22)
                .with_color(COLOR_INK),
        ),
    );
    // Which kind of scan this report is for.
    let website = detail.platform == WEBSITE;
    let supplier_site = website
        && is_supplier_website(
            detail
                .signals
                .as_array()
                .map(|v| v.as_slice())
                .unwrap_or(&[]),
        );
    doc.push(
        elements::Paragraph::new(if supplier_site {
            "Supplier website check"
        } else if website {
            "Shop website check"
        } else {
            "Marketplace supplier scan"
        })
        .styled(
            style::Style::new()
                .bold()
                .with_font_size(11)
                .with_color(COLOR_MUTED),
        ),
    );
    doc.push(elements::Break::new(1));

    // Metadata block (black text)
    let title = detail
        .listing_title
        .clone()
        .unwrap_or_else(|| "Untitled listing".to_string());
    let mut meta = elements::Paragraph::default();
    meta.push_styled(
        format!(
            "{}  ·  {}  ·  Checked {}",
            title,
            if website {
                "Website".to_string()
            } else {
                capitalize(&detail.platform)
            },
            detail.created_at.format("%B %d, %Y")
        ),
        style::Style::new().with_font_size(9).with_color(COLOR_INK),
    );
    doc.push(meta);
    doc.push(elements::Break::new(1));

    // Score card - score on the left, verdict on the right
    let (risk_color, risk_label) = risk_color_and_label(&detail.risk_level);
    let mut score_table = elements::TableLayout::new(vec![1, 1]);
    let mut score_line = elements::Paragraph::default();
    score_line.push_styled(
        format!("{}", detail.risk_score),
        style::Style::new()
            .bold()
            .with_font_size(32)
            .with_color(risk_color),
    );
    score_line.push_styled(
        " / 100",
        style::Style::new()
            .with_font_size(12)
            .with_color(COLOR_MUTED),
    );
    let mut verdict_line = elements::Paragraph::default();
    verdict_line.push_styled(
        risk_label.to_uppercase(),
        style::Style::new()
            .bold()
            .with_font_size(14)
            .with_color(risk_color),
    );
    score_table
        .row()
        .element(score_line)
        .element(verdict_line.aligned(Alignment::Right))
        .push()
        .ok();
    doc.push(elements::PaddedElement::new(
        elements::FramedElement::new(elements::PaddedElement::new(
            score_table,
            Margins::trbl(6, 8, 6, 8),
        )),
        Margins::trbl(0, 0, 6, 0),
    ));

    // Quick-scan summary strip
    if let Some(signals) = detail.signals.as_array() {
        push_summary_strip(&mut doc, signals, website);
        doc.push(elements::Break::new(2));
    }

    // Listing + seller info table (a website check: the shop's own
    // details instead - it has no username, account or last-active).
    let not_found = |v: &Option<String>| v.clone().unwrap_or_else(|| "Not found".to_string());
    let mut info_table = elements::TableLayout::new(vec![1, 2]);
    if website {
        push_section_heading(&mut doc, "WEBSITE & COMPANY");
        push_kv_lines(
            &mut info_table,
            "Website",
            &url_lines(&detail.listing_url, URL_LINE_CHARS),
        );
        push_kv_row(&mut info_table, "Company", &not_found(&detail.seller.name));
        push_kv_row(&mut info_table, "Phone", &not_found(&detail.seller.phone));
        push_kv_row(
            &mut info_table,
            "Website age",
            signal_value(detail, "Website age").unwrap_or("Not found"),
        );
        push_kv_row(
            &mut info_table,
            "Country",
            &not_found(&detail.seller.location),
        );
    } else {
        push_section_heading(&mut doc, "LISTING & SELLER");
        push_kv_lines(
            &mut info_table,
            "Listing URL",
            &url_lines(&detail.listing_url, URL_LINE_CHARS),
        );
        push_kv_row(&mut info_table, "Seller", &not_found(&detail.seller.name));
        push_kv_row(
            &mut info_table,
            "Username",
            &not_found(&detail.seller.handle),
        );
        push_kv_row(&mut info_table, "Phone", &not_found(&detail.seller.phone));
        let account_age = report_account_age(detail);
        push_kv_row(&mut info_table, "Account age", &account_age);
        push_kv_row(
            &mut info_table,
            "Location",
            &not_found(&detail.seller.location),
        );
        push_kv_row(
            &mut info_table,
            "Last active",
            &not_found(&detail.seller.last_active),
        );
    }
    // Safely's own status for this seller (fraud reports), the same as
    // "Status" in the extension - not the platform's verified badge,
    // which is the separate "Platform verification" check below.
    push_kv_row(
        &mut info_table,
        "Safely status",
        &format!("{:?}", detail.seller.verification),
    );
    doc.push(info_table);
    doc.push(elements::Break::new(2));

    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let activity_line = detail
        .seller
        .monthly_activity
        .iter()
        .zip(months.iter())
        .map(|(count, month)| format!("{}: {}", month, count))
        .collect::<Vec<_>>()
        .join("   ");
    push_section_heading(&mut doc, "VISIT ACTIVITY (12 MONTHS)");
    doc.push(
        elements::Paragraph::new(activity_line)
            .styled(style::Style::new().with_font_size(9).with_color(COLOR_INK)),
    );
    doc.push(elements::Break::new(2));

    // Price vs market
    if let Some(signals) = detail.signals.as_array() {
        if let Some(price_signal) = signals.iter().find(|s| {
            s.get("label").and_then(|v| v.as_str())
                == Some(if website { "Prices" } else { "Price analysis" })
        }) {
            let verdict = price_signal
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            let reasoning = price_signal
                .get("sub")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            push_section_heading(&mut doc, "PRICE VS MARKET");
            doc.push(
                elements::Paragraph::new(capitalize(verdict)).styled(
                    style::Style::new()
                        .bold()
                        .with_font_size(11)
                        .with_color(COLOR_INK),
                ),
            );
            doc.push(elements::Break::new(1));
            if !reasoning.is_empty() {
                doc.push(
                    elements::Paragraph::new(reasoning)
                        .styled(style::Style::new().with_font_size(9).with_color(COLOR_INK)),
                );
            }
            doc.push(elements::Break::new(2));
        }
    }

    // Signals - each as its own, bordered card
    push_section_heading(
        &mut doc,
        if website {
            "WEBSITE CHECKS"
        } else {
            "LISTING SIGNALS"
        },
    );
    if let Some(signals) = detail.signals.as_array() {
        for signal in signals {
            doc.push(build_signal_card(signal));
            doc.push(elements::Break::new(1));
        }
    }
    doc.push(elements::Break::new(1));

    // Risk factors
    if let Some(risk_factors) = detail.risk_factors.as_ref().and_then(|v| v.as_array()) {
        if !risk_factors.is_empty() {
            let mut heading = elements::Paragraph::default();
            heading.push_styled(
                "REASONS FOR CAUTION",
                style::Style::new()
                    .bold()
                    .with_font_size(12)
                    .with_color(COLOR_BRAND),
            );
            doc.push(heading);
            doc.push(elements::Break::new(1));
            for factor in risk_factors {
                doc.push(build_risk_factor_card(factor));
                doc.push(elements::Break::new(1));
            }
            doc.push(elements::Break::new(1));
        }
    }

    // Social presence: where the name was found, then the whole-web scam
    // searches in their own section - the same split as the extension
    // and the dashboard.
    let social: Vec<Value> = detail
        .social_candidates
        .as_ref()
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let presence = link_groups(&social, false);
    if !presence.is_empty() {
        push_section_heading(&mut doc, "ONLINE PRESENCE");
        let found: Vec<String> = presence
            .iter()
            .filter(|(_, links)| !links.is_empty())
            .map(|(platform, links)| format!("{} ({})", platform, links_count(links.len())))
            .collect();
        let missing: Vec<&str> = presence
            .iter()
            .filter(|(_, links)| links.is_empty())
            .map(|(platform, _)| platform.as_str())
            .collect();
        if !found.is_empty() {
            push_text(&mut doc, &format!("Found on: {}.", found.join(", ")));
        }
        if !missing.is_empty() {
            push_text(&mut doc, &format!("Not found on: {}.", missing.join(", ")));
        }
        doc.push(elements::Break::new(2));
    }
    let warnings = link_groups(&social, true);
    if !warnings.is_empty() {
        push_section_heading(&mut doc, "SCAM WARNINGS ONLINE");
        let with_links: Vec<&(String, Vec<(String, String)>)> = warnings
            .iter()
            .filter(|(_, links)| !links.is_empty())
            .collect();
        if with_links.is_empty() {
            push_text(&mut doc, "No scam warnings found for this name.");
        } else {
            push_text(
                &mut doc,
                "Read these before you pay. Some may be about something else.",
            );
            doc.push(elements::Break::new(1));
            for (platform, links) in with_links {
                doc.push(
                    elements::Paragraph::new(format!(
                        "{} ({})",
                        platform,
                        links_count(links.len())
                    ))
                    .styled(
                        style::Style::new()
                            .bold()
                            .with_font_size(10)
                            .with_color(COLOR_CAUTION),
                    ),
                );
                for (title, url) in links {
                    push_text(&mut doc, title);
                    for line in url_lines(url, URL_LINE_CHARS) {
                        doc.push(
                            elements::Paragraph::new(line).styled(
                                style::Style::new()
                                    .with_font_size(8)
                                    .with_color(COLOR_MUTED),
                            ),
                        );
                    }
                }
                doc.push(elements::Break::new(1));
            }
        }
        doc.push(elements::Break::new(1));
    }

    // Before you pay: the same steps as the extension and the dashboard.
    push_section_heading(&mut doc, "BEFORE YOU PAY");
    let signals: Vec<Value> = detail.signals.as_array().cloned().unwrap_or_default();
    for (i, (title, body)) in before_you_pay(website, &signals).iter().enumerate() {
        doc.push(
            elements::Paragraph::new(format!("{}. {}", i + 1, title)).styled(
                style::Style::new()
                    .bold()
                    .with_font_size(10)
                    .with_color(COLOR_INK),
            ),
        );
        push_text(&mut doc, body);
        doc.push(elements::Break::new(1));
    }
    doc.push(elements::Break::new(1));

    // Reports filed
    push_section_heading(&mut doc, "REPORTS FILED");
    if detail.reports.is_empty() {
        doc.push(
            elements::Paragraph::new("You have not reported this seller.")
                .styled(style::Style::new().with_font_size(10).with_color(COLOR_INK)),
        );
    } else {
        for report in &detail.reports {
            let mut line = elements::Paragraph::default();
            line.push_styled(
                format!("{:?}", report.report_type),
                style::Style::new()
                    .bold()
                    .with_font_size(10)
                    .with_color(COLOR_INK),
            );
            line.push_styled(
                format!("  —  {}", report.reported_at.format("%B %d, %Y")),
                style::Style::new()
                    .with_font_size(9)
                    .with_color(COLOR_MUTED),
            );
            doc.push(line);
        }
    }
    doc.push(elements::Break::new(2));

    // Footer disclaimer - no box, black text
    doc.push(
        elements::Paragraph::new(
            "This report reflects Safely's automated analysis at the time of the check. It is guidance to help inform your decision, not a guarantee. Always use your own judgement before proceeding with a transaction.",
        )
        .styled(style::Style::new().italic().with_font_size(9).with_color(COLOR_INK)),
    );

    let mut buffer = Vec::new();
    doc.render(&mut buffer)
        .map_err(|e| format!("Could not render the real PDF: {}", e))?;

    Ok(buffer)
}

fn push_section_heading(doc: &mut Document, text: &str) {
    doc.push(
        elements::Paragraph::new(text).styled(
            style::Style::new()
                .bold()
                .with_font_size(11)
                .with_color(COLOR_BRAND),
        ),
    );
    doc.push(elements::Break::new(1));
}

fn push_kv_row(table: &mut elements::TableLayout, key: &str, value: &str) {
    table
        .row()
        .element(elements::PaddedElement::new(
            elements::Paragraph::new(key).styled(
                style::Style::new()
                    .bold()
                    .with_font_size(9)
                    .with_color(COLOR_INK),
            ),
            Margins::trbl(0, 0, 4, 0),
        ))
        .element(elements::PaddedElement::new(
            elements::Paragraph::new(value)
                .styled(style::Style::new().with_font_size(10).with_color(COLOR_INK)),
            Margins::trbl(0, 0, 4, 0),
        ))
        .push()
        .ok();
}

/// Like push_kv_row, but the value is several lines (a long web address
/// split into pieces that fit the column).
fn push_kv_lines(table: &mut elements::TableLayout, key: &str, lines: &[String]) {
    let mut value = elements::LinearLayout::vertical();
    for line in lines {
        value.push(
            elements::Paragraph::new(line.as_str())
                .styled(style::Style::new().with_font_size(9).with_color(COLOR_INK)),
        );
    }
    table
        .row()
        .element(elements::PaddedElement::new(
            elements::Paragraph::new(key).styled(
                style::Style::new()
                    .bold()
                    .with_font_size(9)
                    .with_color(COLOR_INK),
            ),
            Margins::trbl(0, 0, 4, 0),
        ))
        .element(elements::PaddedElement::new(
            value,
            Margins::trbl(0, 0, 4, 0),
        ))
        .push()
        .ok();
}

/// A web address without its tracking part ("?spm=...") and cut into
/// pieces of at most `max_chars`, so it fits in the PDF column.
fn url_lines(url: &str, max_chars: usize) -> Vec<String> {
    let clean = url.split(['?', '#']).next().unwrap_or("").trim();
    if clean.is_empty() {
        return vec!["Not found".to_string()];
    }
    let chars: Vec<char> = clean.chars().collect();
    chars
        .chunks(max_chars.max(1))
        .map(|c| c.iter().collect())
        .collect()
}

/// Turns a completeness check's hidden checklist into readable text:
/// "2 of 3 ... filled in.###CHECKLIST###Employee count|true;Export
/// percentage|false" -> "2 of 3 ... filled in. Provided: Employee
/// count. Missing: Export percentage."
fn readable_sub(sub: &str) -> String {
    let Some((text, checklist)) = sub.split_once(CHECKLIST_MARKER) else {
        return sub.to_string();
    };
    let mut provided = Vec::new();
    let mut missing = Vec::new();
    for item in checklist.split(';') {
        if let Some((name, present)) = item.split_once('|') {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            if present.trim() == "true" {
                provided.push(name);
            } else {
                missing.push(name);
            }
        }
    }
    let mut out = text.trim().to_string();
    if !provided.is_empty() {
        out.push_str(&format!(" Provided: {}.", provided.join(", ")));
    }
    if !missing.is_empty() {
        out.push_str(&format!(" Missing: {}.", missing.join(", ")));
    }
    out
}

fn push_summary_strip(doc: &mut Document, signals: &[Value], website: bool) {
    let watch_list = if website {
        [
            "Domain check",
            "Website age",
            "Payment at checkout",
            "Contact info",
            "Shop policies",
        ]
    } else {
        [
            "Domain check",
            "Advance payment request",
            "Urgency language",
            "Contact info",
            "Image authenticity",
        ]
    };
    let mut table = elements::TableLayout::new(vec![1; watch_list.len()]);
    let mut row = table.row();
    for label in watch_list {
        let signal = signals
            .iter()
            .find(|s| s.get("label").and_then(|v| v.as_str()) == Some(label));
        let (dot_color, _) = signal
            .and_then(|s| s.get("type").and_then(|v| v.as_str()))
            .map(status_color)
            .unwrap_or((COLOR_MUTED, ""));
        let mut cell = elements::Paragraph::default();
        cell.push_styled(
            "● ",
            style::Style::new().with_font_size(9).with_color(dot_color),
        );
        cell.push_styled(
            label,
            style::Style::new()
                .with_font_size(7)
                .with_color(COLOR_MUTED),
        );
        row = row.element(elements::PaddedElement::new(
            cell,
            Margins::trbl(0, 2, 0, 0),
        ));
    }
    row.push().ok();
    doc.push(table);
}

fn build_signal_card(
    signal: &Value,
) -> elements::FramedElement<elements::PaddedElement<elements::LinearLayout>> {
    let label = signal
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();
    let value = capitalize(signal.get("value").and_then(|v| v.as_str()).unwrap_or(""));
    let sub = readable_sub(signal.get("sub").and_then(|v| v.as_str()).unwrap_or(""));
    let signal_type = signal
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let (color, _) = status_color(&signal_type);

    let mut header = elements::Paragraph::default();
    header.push_styled(
        label,
        style::Style::new()
            .bold()
            .with_font_size(11)
            .with_color(COLOR_INK),
    );
    header.push_styled(
        format!("   {}", value),
        style::Style::new()
            .bold()
            .with_font_size(11)
            .with_color(color),
    );

    let mut inner = elements::LinearLayout::vertical();
    inner.push(header);
    if !sub.is_empty() {
        inner.push(elements::PaddedElement::new(
            elements::Paragraph::new(sub)
                .styled(style::Style::new().with_font_size(9).with_color(COLOR_INK)),
            Margins::trbl(2, 0, 0, 0),
        ));
    }
    elements::FramedElement::new(elements::PaddedElement::new(
        inner,
        Margins::trbl(4, 6, 4, 6),
    ))
}

fn build_risk_factor_card(
    factor: &Value,
) -> elements::FramedElement<elements::PaddedElement<elements::LinearLayout>> {
    let name = factor
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();
    let description = factor
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let severity = factor
        .get("severity")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let severity_color = match severity.as_str() {
        "hard" => COLOR_HIGH,
        "compound" => COLOR_CAUTION,
        _ => COLOR_MUTED,
    };

    let mut header = elements::Paragraph::default();
    header.push_styled(
        capitalize(&name.replace('_', " ")),
        style::Style::new()
            .bold()
            .with_font_size(11)
            .with_color(COLOR_INK),
    );
    header.push_styled(
        format!("   {}", capitalize(&severity)),
        style::Style::new()
            .bold()
            .with_font_size(11)
            .with_color(severity_color),
    );

    let mut inner = elements::LinearLayout::vertical();
    inner.push(header);
    if !description.is_empty() {
        inner.push(elements::PaddedElement::new(
            elements::Paragraph::new(description)
                .styled(style::Style::new().with_font_size(9).with_color(COLOR_INK)),
            Margins::trbl(2, 0, 0, 0),
        ));
    }
    elements::FramedElement::new(elements::PaddedElement::new(
        inner,
        Margins::trbl(5, 6, 5, 6),
    ))
}

fn push_text(doc: &mut Document, text: &str) {
    doc.push(
        elements::Paragraph::new(text)
            .styled(style::Style::new().with_font_size(9).with_color(COLOR_INK)),
    );
}

fn links_count(n: usize) -> String {
    if n == 1 {
        "1 link".to_string()
    } else {
        format!("{n} links")
    }
}

/// The value of one check ("Website age" -> "3 years old").
fn signal_value<'a>(detail: &'a HistoryDetailResponse, label: &str) -> Option<&'a str> {
    detail
        .signals
        .as_array()?
        .iter()
        .find(|s| s.get("label").and_then(|v| v.as_str()) == Some(label))
        .and_then(|s| s.get("value").and_then(|v| v.as_str()))
}

/// The order the social sites are listed in (the same as the extension
/// and the dashboard); any other row name comes after.
const PLATFORM_ORDER: &[&str] = &[
    "Facebook",
    "LinkedIn",
    "TikTok",
    "Instagram",
    "Reddit",
    "Trustpilot",
    "Reviews",
    "Contact (Facebook)",
    "Contact (LinkedIn)",
    "Contact (Web)",
    "Google search",
    "Complaint sites",
];

/// The search results grouped by row name ("Facebook"), each with its
/// links (title, address) without repeats. `web_scam` picks the
/// whole-web scam searches ("Google search", "Complaint sites") or every
/// other row. A row with no links is kept, for "Not found on".
fn link_groups(social: &[Value], web_scam: bool) -> Vec<(String, Vec<(String, String)>)> {
    let mut groups: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for row in social {
        let Some(platform) = row.get("platform").and_then(|v| v.as_str()) else {
            continue;
        };
        if is_web_scam_row(platform) != web_scam {
            continue;
        }
        let index = match groups.iter().position(|(p, _)| p == platform) {
            Some(i) => i,
            None => {
                groups.push((platform.to_string(), Vec::new()));
                groups.len() - 1
            }
        };
        for link in row
            .get("candidates")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            let url = link.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if url.is_empty() || groups[index].1.iter().any(|(_, u)| u == url) {
                continue;
            }
            let title = link.get("title").and_then(|v| v.as_str()).unwrap_or(url);
            groups[index].1.push((title.to_string(), url.to_string()));
        }
    }
    let rank = |p: &str| {
        PLATFORM_ORDER
            .iter()
            .position(|x| *x == p)
            .unwrap_or(usize::MAX)
    };
    groups.sort_by_key(|(p, _)| rank(p));
    groups
}

/// "Before you pay" steps for a marketplace supplier - the same list and
/// the same rules as the extension (tabs/intelligence.ts) and the
/// dashboard (ts/detail_view.ts). Change all three together.
const B2B_CHECKS: &[(&str, &str)] = &[
    (
        "Use buyer protection if the platform offers it",
        "If the platform has protected payment (e.g. Trade Assurance on Alibaba), pay through it so the order is covered. If it has none, pay only by bank transfer to an account in the company's own name.",
    ),
    (
        "Match the business licence to the bank account",
        "Ask for the business licence and check that the company name on it matches the listing and the name on the bank account exactly.",
    ),
    (
        "Confirm bank details by phone or video",
        "Before the first payment, and whenever bank details change, confirm them live with a contact you already know. Never act on changed details sent only by email.",
    ),
    (
        "Order a sample first",
        "Pay for a sample and check its quality before placing a bulk order.",
    ),
    (
        "Ask for a live video call",
        "Ask the supplier to show where they work and your goods live: the production line if they are a factory, the warehouse or office if they are a trader or shipping company. This confirms they really operate where they say.",
    ),
];
const B2B_RISKY_PAYMENT_CHECK: (&str, &str) = (
    "Do not pay by Western Union, MoneyGram or crypto",
    "This supplier lists payment methods that cannot be reversed or traced to a company. Pay only by bank transfer to an account in the company's own name, or through the platform's protected payment.",
);
const B2B_FULL_PREPAYMENT_CHECK: (&str, &str) = (
    "Don't pay everything before shipment",
    "This supplier asks for the full price before the goods are shipped. Try to pay the balance only against a copy of the Bill of Lading, or use a Letter of Credit, and pay only to a bank account in the company's own name.",
);
const B2B_REGULATED_PRODUCT_CHECK: (&str, &str) = (
    "Buy only from the brand owner or an authorised distributor",
    "This product needs a licence or prescription, and fakes of it can be dangerous. Ask the supplier for a letter from the brand owner showing they are an authorised distributor, and check it with the brand owner directly. You may also need your own import licence.",
);
const B2B_LICENCE_CHECK: (&str, &str) = (
    "Check the licences before you order",
    "This product needs a licence to sell or import. Make sure you are allowed to import it into your country, and ask the supplier for its export licence and any end-user certificate the sale needs before you pay.",
);
const B2B_COMMODITY_CHECK: (&str, &str) = (
    "Don't pay fees before an inspection",
    "Fake commodity sellers ask for fees, a deposit or document costs before anything ships. Pay nothing until the goods are inspected by a company you choose (such as SGS), and pay only by Letter of Credit.",
);

/// "Before you pay" steps for a shop's own website.
const WEBSITE_CHECKS: &[(&str, &str)] = &[
    (
        "Pay by card or PayPal",
        "Card and PayPal payments can be disputed if the order never arrives or is not as described. Money sent by bank transfer, Pix, crypto or gift card cannot be got back.",
    ),
    (
        "Read the return policy first",
        "Check how returns and refunds work before you buy, and save a copy. If the shop has no clear return policy, ask before paying.",
    ),
    (
        "Start with a small order",
        "Buy one item first and wait until it arrives as described before placing a bigger order.",
    ),
    (
        "Keep proof of your order",
        "Save screenshots of the product page, the price and your order confirmation. You will need them for a refund or a dispute.",
    ),
    (
        "Contact the shop before a big order",
        "Write to the email or phone number the shop gives and see if a real person answers. A shop that never replies is a warning sign.",
    ),
];
/// "Before you pay" steps for a factory or supplier's own website: it
/// sells by quote (no prices, no way to pay shown), so its buyers pay by
/// bank transfer, like a marketplace supplier.
const SUPPLIER_WEBSITE_FIRST_CHECK: (&str, &str) = (
    "Pay only to the company's own bank account",
    "Pay by bank transfer only to an account in the company's own name, never to a person. Ask for the bank details in writing and check that the name matches the company.",
);

/// True for a factory or supplier's own website (see above).
fn is_supplier_website(signals: &[Value]) -> bool {
    let not_shown = |label: &str| {
        signals.iter().any(|s| {
            s.get("label").and_then(|v| v.as_str()) == Some(label)
                && s.get("value").and_then(|v| v.as_str()) == Some("Not shown")
        })
    };
    not_shown("Payment at checkout") && not_shown("Prices")
}

const WEBSITE_LOOKALIKE_CHECK: (&str, &str) = (
    "Buy only on the brand's real website",
    "This address copies a famous brand's name, but it is not the brand's own website. Type the brand's real address yourself and buy there instead.",
);
const WEBSITE_RISKY_PAYMENT_CHECK: (&str, &str) = (
    "Don't pay by bank transfer, Pix or crypto",
    "This shop only shows ways to pay that cannot be reversed. Pay only by card or PayPal. If the shop does not accept them, do not buy.",
);

/// The "Before you pay" steps for this scan: the special ones its
/// checks call for first, then the usual ones.
pub fn before_you_pay(website: bool, signals: &[Value]) -> Vec<(&'static str, &'static str)> {
    let find = |label: &str| {
        signals
            .iter()
            .find(|s| s.get("label").and_then(|v| v.as_str()) == Some(label))
    };
    let field = |s: &Value, key: &str| {
        s.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let caution = |label: &str| find(label).filter(|s| field(s, "type") == "caution");
    let mut checks = Vec::new();
    if website {
        if caution("Domain check").is_some_and(|s| field(s, "value").starts_with("Looks like")) {
            checks.push(WEBSITE_LOOKALIKE_CHECK);
        }
        if find("Payment at checkout").is_some_and(|s| field(s, "value") == "Risky methods only") {
            checks.push(WEBSITE_RISKY_PAYMENT_CHECK);
        }
        if is_supplier_website(signals) {
            checks.push(SUPPLIER_WEBSITE_FIRST_CHECK);
            checks.extend_from_slice(&B2B_CHECKS[1..]);
        } else {
            checks.extend_from_slice(WEBSITE_CHECKS);
        }
        return checks;
    }
    if find("Product range").is_some_and(|s| field(s, "value") == "Commodity scam pattern") {
        checks.push(B2B_COMMODITY_CHECK);
    }
    if let Some(s) = caution("Regulated product") {
        checks.push(if field(s, "value") == "Licence needed, not the maker" {
            B2B_REGULATED_PRODUCT_CHECK
        } else {
            B2B_LICENCE_CHECK
        });
    }
    if let Some(s) = caution("Advance payment request") {
        checks.push(if field(s, "value") == "Full prepayment" {
            B2B_FULL_PREPAYMENT_CHECK
        } else {
            B2B_RISKY_PAYMENT_CHECK
        });
    }
    checks.extend_from_slice(B2B_CHECKS);
    checks
}

pub fn status_color(signal_type: &str) -> (style::Color, &'static str) {
    match signal_type {
        "good" => (COLOR_GOOD, "Good"),
        "caution" => (COLOR_CAUTION, "Caution"),
        "info" => (COLOR_MUTED, "Info"),
        _ => (COLOR_HIGH, "Flag"),
    }
}

pub fn risk_color_and_label(
    risk_level: &crate::models::analysis::RiskLevel,
) -> (style::Color, &'static str) {
    match risk_level {
        crate::models::analysis::RiskLevel::Low => (COLOR_GOOD, "Low risk"),
        crate::models::analysis::RiskLevel::Caution => (COLOR_CAUTION, "Caution"),
        crate::models::analysis::RiskLevel::High => (COLOR_HIGH, "High risk"),
    }
}

pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Only the founding YEAR is known, so the age is taken from the
/// "Account age" check ("About 11 years", "Founded this year") instead
/// of being counted in months from 1 January. Without that check, the
/// seller's saved account age is used.
fn report_account_age(detail: &HistoryDetailResponse) -> String {
    let check_value = detail
        .signals
        .as_array()
        .and_then(|s| {
            s.iter()
                .find(|x| x.get("label").and_then(|v| v.as_str()) == Some("Account age"))
        })
        .and_then(|x| x.get("value").and_then(|v| v.as_str()))
        .filter(|v| *v != "Not provided" && *v != "Invalid date");
    match check_value {
        Some(age) => age.to_string(),
        None => detail.seller.account_age.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checklist_becomes_readable_text() {
        let sub = "2 of 3 transparency fields (employees, sales volume, export percentage) are filled in.###CHECKLIST###Employee count|true;Sales revenue|true;Export percentage|false";
        assert_eq!(
            readable_sub(sub),
            "2 of 3 transparency fields (employees, sales volume, export percentage) are filled in. Provided: Employee count, Sales revenue. Missing: Export percentage."
        );
        assert_eq!(readable_sub("No checklist here."), "No checklist here.");
        assert!(!readable_sub(sub).contains("###"));
    }

    #[test]
    fn long_url_is_cleaned_and_split_to_fit() {
        let url = "https://www.alibaba.com/product-detail/Door-to-Door-DHL-FEDEX-UPS_1601630546813.html?spm=a2700.product_home&priceId=e46a";
        let lines = url_lines(url, 60);
        assert!(lines.iter().all(|l| l.chars().count() <= 60));
        assert_eq!(
            lines.concat(),
            "https://www.alibaba.com/product-detail/Door-to-Door-DHL-FEDEX-UPS_1601630546813.html"
        );
        assert_eq!(url_lines("", 60), vec!["Not found".to_string()]);
    }

    #[test]
    fn websites_and_marketplaces_get_their_own_before_you_pay_steps() {
        let sig = |label: &str, value: &str, kind: &str| serde_json::json!({ "label": label, "value": value, "type": kind, "sub": "" });
        let market = before_you_pay(false, &[]);
        assert_eq!(
            market[0].0,
            "Use buyer protection if the platform offers it"
        );
        assert_eq!(market.len(), 5);
        let prepay = before_you_pay(
            false,
            &[sig("Advance payment request", "Full prepayment", "caution")],
        );
        assert_eq!(prepay[0].0, "Don't pay everything before shipment");

        let shop = before_you_pay(true, &[]);
        assert_eq!(shop[0].0, "Pay by card or PayPal");
        assert!(shop.iter().all(|(t, _)| !t.contains("sample")));
        let risky = before_you_pay(
            true,
            &[
                sig("Domain check", "Looks like Amazon", "caution"),
                sig("Payment at checkout", "Risky methods only", "caution"),
            ],
        );
        assert_eq!(risky[0].0, "Buy only on the brand's real website");
        assert_eq!(risky[1].0, "Don't pay by bank transfer, Pix or crypto");

        // A factory selling by quote (gloflux.com, xtoinc.com): supplier steps.
        let factory = before_you_pay(
            true,
            &[
                sig("Payment at checkout", "Not shown", "info"),
                sig("Prices", "Not shown", "info"),
            ],
        );
        assert_eq!(factory[0].0, "Pay only to the company's own bank account");
        assert_eq!(
            factory[1].0,
            "Match the business licence to the bank account"
        );
        assert!(factory.iter().all(|(t, _)| !t.contains("PayPal")));
        assert_eq!(factory.len(), 5);
    }

    #[test]
    fn scam_warnings_are_kept_apart_from_online_presence() {
        let social = serde_json::json!([
            { "platform": "LinkedIn", "found": true, "candidates": [
                { "platform": "LinkedIn", "title": "Acme | LinkedIn", "url": "https://linkedin.com/company/acme" }
            ] },
            { "platform": "Facebook", "found": false, "candidates": [] },
            { "platform": "Google search", "found": true, "candidates": [
                { "platform": "Google search", "title": "Acme scam", "url": "https://forum.example/acme" },
                { "platform": "Google search", "title": "Acme scam", "url": "https://forum.example/acme" }
            ] },
            { "platform": "Complaint sites", "found": false }
        ]);
        let rows = social.as_array().unwrap();
        let presence = link_groups(rows, false);
        assert_eq!(presence[0].0, "Facebook");
        assert_eq!(presence[1].1.len(), 1);
        let warnings = link_groups(rows, true);
        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].0, "Google search");
        assert_eq!(warnings[0].1.len(), 1, "the same link is listed once");
        assert!(warnings[1].1.is_empty());
    }

    #[test]
    fn signal_values_are_capitalized() {
        assert_eq!(capitalize("normal"), "Normal");
        assert_eq!(capitalize("Verified"), "Verified");
    }
}
