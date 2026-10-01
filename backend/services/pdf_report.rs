use crate::models::history::HistoryDetailResponse;
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
            capitalize(&detail.platform),
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
        push_summary_strip(&mut doc, signals);
        doc.push(elements::Break::new(2));
    }

    // Listing + seller info table
    push_section_heading(&mut doc, "LISTING & SELLER");
    let mut info_table = elements::TableLayout::new(vec![1, 2]);
    push_kv_lines(
        &mut info_table,
        "Listing URL",
        &url_lines(&detail.listing_url, URL_LINE_CHARS),
    );
    push_kv_row(
        &mut info_table,
        "Seller",
        &detail
            .seller
            .name
            .clone()
            .unwrap_or_else(|| "Not found".to_string()),
    );
    push_kv_row(
        &mut info_table,
        "Username",
        &detail
            .seller
            .handle
            .clone()
            .unwrap_or_else(|| "Not found".to_string()),
    );
    push_kv_row(
        &mut info_table,
        "Phone",
        &detail
            .seller
            .phone
            .clone()
            .unwrap_or_else(|| "Not found".to_string()),
    );
    let account_age = report_account_age(detail);
    push_kv_row(&mut info_table, "Account age", &account_age);
    push_kv_row(
        &mut info_table,
        "Location",
        &detail
            .seller
            .location
            .clone()
            .unwrap_or_else(|| "Not found".to_string()),
    );
    push_kv_row(
        &mut info_table,
        "Last active",
        &detail
            .seller
            .last_active
            .clone()
            .unwrap_or_else(|| "Not found".to_string()),
    );
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
        if let Some(price_signal) = signals
            .iter()
            .find(|s| s.get("label").and_then(|v| v.as_str()) == Some("Price analysis"))
        {
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
    push_section_heading(&mut doc, "LISTING SIGNALS");
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

    // Social presence
    if let Some(social) = detail.social_candidates.as_ref().and_then(|v| v.as_array()) {
        if !social.is_empty() {
            let found_count = social
                .iter()
                .filter(|r| r.get("found").and_then(|f| f.as_bool()).unwrap_or(false))
                .count();
            push_section_heading(&mut doc, "SOCIAL PRESENCE CHECK");
            doc.push(
                elements::Paragraph::new(format!(
                    "{} of {} platform checks found a real, candidate result.",
                    found_count,
                    social.len()
                ))
                .styled(style::Style::new().with_font_size(10).with_color(COLOR_INK)),
            );
            doc.push(elements::Break::new(2));
        }
    }

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

fn push_summary_strip(doc: &mut Document, signals: &[Value]) {
    let watch_list = [
        "Domain check",
        "Advance payment request",
        "Urgency language",
        "Contact info",
        "Image authenticity",
    ];
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

/// On a B2B report only the founding YEAR is known, so the age is taken
/// from the "Account age" check ("About 11 years", "Founded this year")
/// instead of being counted in months from 1 January. Other reports
/// (OLX) keep the seller's normal account age.
fn report_account_age(detail: &HistoryDetailResponse) -> String {
    let signals = detail.signals.as_array();
    let is_b2b = signals.map_or(false, |s| {
        s.iter().any(|x| {
            matches!(
                x.get("label").and_then(|v| v.as_str()),
                Some("Listing completeness") | Some("Company profile completeness")
            )
        })
    });
    if is_b2b {
        let check_value = signals
            .and_then(|s| {
                s.iter()
                    .find(|x| x.get("label").and_then(|v| v.as_str()) == Some("Account age"))
            })
            .and_then(|x| x.get("value").and_then(|v| v.as_str()))
            .filter(|v| *v != "Not provided" && *v != "Invalid date");
        if let Some(age) = check_value {
            return age.to_string();
        }
    }
    detail.seller.account_age.clone()
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
    fn signal_values_are_capitalized() {
        assert_eq!(capitalize("normal"), "Normal");
        assert_eq!(capitalize("Verified"), "Verified");
    }
}
