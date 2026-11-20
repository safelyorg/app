//! The vendor check as a PDF (GET /api/v1/vendors/{id}/pdf), in the same
//! look as a scan's evidence report (pdf_report.rs): the score, what
//! needs a look, what looks fine, outside records, online presence,
//! scam warnings, the vendor's answers, the files, the decision and
//! "Before you pay". Built from the same data the Vendors page shows
//! (vendors.rs vendor_detail), so the PDF never says more or less than
//! the page.

use crate::services::features::pdf_report::{before_you_pay, capitalize, status_color};
use genpdf::{Alignment, Document, Element, Margins, SimplePageDecorator, elements, fonts, style};
use serde_json::Value;

const COLOR_BRAND: style::Color = style::Color::Rgb(111, 179, 239);
const COLOR_GOOD: style::Color = style::Color::Rgb(21, 140, 105);
const COLOR_CAUTION: style::Color = style::Color::Rgb(180, 120, 10);
const COLOR_HIGH: style::Color = style::Color::Rgb(200, 40, 40);
const COLOR_MUTED: style::Color = style::Color::Rgb(120, 120, 128);
const COLOR_INK: style::Color = style::Color::Rgb(25, 25, 30);

/// Rows of the web search that are scam searches, not the company's own
/// pages (osint.rs WEB_SCAM_SEARCH and COMPLAINT_SITES).
const SCAM_ROWS: &[&str] = &["Google search", "Complaint sites"];

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn heading(doc: &mut Document, title: &str) {
    doc.push(elements::Break::new(1));
    doc.push(
        elements::Paragraph::new(title).styled(
            style::Style::new()
                .bold()
                .with_font_size(11)
                .with_color(COLOR_BRAND),
        ),
    );
    doc.push(elements::Break::new(1));
}

fn line(doc: &mut Document, words: &str, color: style::Color, size: u8) {
    doc.push(
        elements::Paragraph::new(words)
            .styled(style::Style::new().with_font_size(size).with_color(color)),
    );
}

/// One check as a framed card: its name, its answer in colour, and the
/// explanation under it.
fn check_card(doc: &mut Document, label: &str, value: &str, kind: &str, detail: &str) {
    let (color, _) = status_color(kind);
    let mut head = elements::Paragraph::default();
    head.push_styled(
        label.to_string(),
        style::Style::new()
            .bold()
            .with_font_size(10)
            .with_color(COLOR_INK),
    );
    head.push_styled(
        format!("   {}", capitalize(value)),
        style::Style::new()
            .bold()
            .with_font_size(10)
            .with_color(color),
    );
    let mut inner = elements::LinearLayout::vertical();
    inner.push(head);
    if !detail.is_empty() {
        inner.push(elements::PaddedElement::new(
            elements::Paragraph::new(detail.to_string())
                .styled(style::Style::new().with_font_size(8).with_color(COLOR_INK)),
            Margins::trbl(2, 0, 0, 0),
        ));
    }
    doc.push(elements::PaddedElement::new(
        elements::FramedElement::new(elements::PaddedElement::new(
            inner,
            Margins::trbl(4, 6, 4, 6),
        )),
        Margins::trbl(0, 0, 3, 0),
    ));
}

fn kv_row(table: &mut elements::TableLayout, key: &str, value: &str) {
    table
        .row()
        .element(elements::PaddedElement::new(
            elements::Paragraph::new(key.to_string()).styled(
                style::Style::new()
                    .bold()
                    .with_font_size(9)
                    .with_color(COLOR_INK),
            ),
            Margins::trbl(0, 0, 3, 0),
        ))
        .element(elements::PaddedElement::new(
            elements::Paragraph::new(value.to_string())
                .styled(style::Style::new().with_font_size(9).with_color(COLOR_INK)),
            Margins::trbl(0, 0, 3, 0),
        ))
        .push()
        .ok();
}

/// Like kv_row, with the value on several lines.
fn kv_lines(table: &mut elements::TableLayout, key: &str, lines: &[String]) {
    let mut value = elements::LinearLayout::vertical();
    for l in lines {
        value.push(
            elements::Paragraph::new(l.clone())
                .styled(style::Style::new().with_font_size(8).with_color(COLOR_INK)),
        );
    }
    table
        .row()
        .element(elements::PaddedElement::new(
            elements::Paragraph::new(key.to_string()).styled(
                style::Style::new()
                    .bold()
                    .with_font_size(9)
                    .with_color(COLOR_INK),
            ),
            Margins::trbl(0, 0, 3, 0),
        ))
        .element(elements::PaddedElement::new(
            value,
            Margins::trbl(0, 0, 3, 0),
        ))
        .push()
        .ok();
}

/// A web address cut into pieces that fit the page.
fn url_pieces(url: &str) -> Vec<String> {
    let clean = url.split(['?', '#']).next().unwrap_or("").trim();
    clean
        .chars()
        .collect::<Vec<_>>()
        .chunks(80)
        .map(|c| c.iter().collect())
        .collect()
}

fn is_flagged(kind: &str) -> bool {
    kind == "bad" || kind == "caution"
}

fn level_words(level: &str) -> (style::Color, &'static str, &'static str) {
    match level {
        "high" => (COLOR_HIGH, "High risk", "Don't pay this vendor yet."),
        "caution" => (
            COLOR_CAUTION,
            "Caution",
            "Check these points before you pay.",
        ),
        _ => (COLOR_GOOD, "Low risk", "No serious warning signs."),
    }
}

/// The PDF of one vendor check, from vendor_detail's answer. Err when
/// the fonts can't be read or the check has no result yet.
pub fn generate_vendor_pdf(detail: &Value) -> Result<Vec<u8>, String> {
    let result = detail
        .get("result")
        .filter(|r| !r.is_null())
        .ok_or("This vendor has no result yet")?;
    let font_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/fonts");
    let font_family = fonts::from_files(font_dir, "Roboto", None)
        .map_err(|e| format!("Could not load the font files: {e}"))?;
    let mut doc = Document::new(font_family);
    doc.set_title("Safely Vendor Check");
    let mut decorator = SimplePageDecorator::new();
    decorator.set_margins(18);
    doc.set_page_decorator(decorator);

    // Title
    doc.push(
        elements::Paragraph::new("SAFELY").styled(
            style::Style::new()
                .bold()
                .with_font_size(10)
                .with_color(COLOR_BRAND),
        ),
    );
    doc.push(
        elements::Paragraph::new("Vendor Check Report").styled(
            style::Style::new()
                .bold()
                .with_font_size(22)
                .with_color(COLOR_INK),
        ),
    );
    doc.push(
        elements::Paragraph::new("Vendor onboarding form").styled(
            style::Style::new()
                .bold()
                .with_font_size(11)
                .with_color(COLOR_MUTED),
        ),
    );
    doc.push(elements::Break::new(1));
    let mut meta = vec![text(detail, "vendor_name").to_string()];
    meta.push(text(detail, "vendor_email").to_string());
    if !text(detail, "sent_on").is_empty() {
        meta.push(format!("Sent {}", text(detail, "sent_on")));
    }
    if !text(detail, "checked_on").is_empty() {
        meta.push(format!("Checked {}", text(detail, "checked_on")));
    }
    line(&mut doc, &meta.join("  ·  "), COLOR_INK, 9);
    line(
        &mut doc,
        &format!("Asked by {}", text(detail, "buyer_company")),
        COLOR_MUTED,
        9,
    );
    doc.push(elements::Break::new(1));

    // Score
    let level = text(detail, "risk_level");
    let (color, label, say) = level_words(level);
    let score = detail
        .get("risk_score")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let mut score_table = elements::TableLayout::new(vec![1, 2]);
    let mut left = elements::Paragraph::default();
    left.push_styled(
        score.to_string(),
        style::Style::new()
            .bold()
            .with_font_size(32)
            .with_color(color),
    );
    left.push_styled(
        " / 100",
        style::Style::new()
            .with_font_size(12)
            .with_color(COLOR_MUTED),
    );
    let mut right = elements::LinearLayout::vertical();
    right.push(
        elements::Paragraph::new(label.to_uppercase())
            .aligned(Alignment::Right)
            .styled(
                style::Style::new()
                    .bold()
                    .with_font_size(14)
                    .with_color(color),
            ),
    );
    right.push(
        elements::Paragraph::new(say)
            .aligned(Alignment::Right)
            .styled(style::Style::new().with_font_size(10).with_color(COLOR_INK)),
    );
    score_table.row().element(left).element(right).push().ok();
    doc.push(elements::FramedElement::new(elements::PaddedElement::new(
        score_table,
        Margins::trbl(6, 8, 6, 8),
    )));

    // Decision
    let status = text(detail, "status");
    if (status == "approved" || status == "rejected") && !text(detail, "decided_on").is_empty() {
        doc.push(elements::Break::new(1));
        let (c, word) = if status == "approved" {
            (COLOR_GOOD, "Approved")
        } else {
            (COLOR_HIGH, "Rejected")
        };
        let mut p = elements::Paragraph::default();
        p.push_styled(
            word,
            style::Style::new().bold().with_font_size(10).with_color(c),
        );
        p.push_styled(
            format!(
                "  by {} on {}",
                text(detail, "decided_by"),
                text(detail, "decided_on")
            ),
            style::Style::new().with_font_size(9).with_color(COLOR_INK),
        );
        doc.push(p);
        if !text(detail, "decision_note").is_empty() {
            line(
                &mut doc,
                &format!("\"{}\"", text(detail, "decision_note")),
                COLOR_INK,
                9,
            );
        }
    }

    let empty = Vec::new();
    let cards = result
        .get("cards")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let records = result
        .get("records")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let order = |k: &str| match k {
        "bad" => 0,
        "caution" => 1,
        "good" => 2,
        _ => 3,
    };
    let mut cards: Vec<&Value> = cards.iter().collect();
    cards.sort_by_key(|c| order(text(c, "type")));
    let mut records: Vec<&Value> = records.iter().collect();
    records.sort_by_key(|r| order(text(r, "type")));

    // Needs a look: the vendor's checks and outside records that warn.
    let flagged: Vec<(&str, &str, &str, &str)> = cards
        .iter()
        .filter(|c| is_flagged(text(c, "type")))
        .map(|c| {
            (
                text(c, "label"),
                text(c, "value"),
                text(c, "type"),
                text(c, "detail"),
            )
        })
        .chain(
            records
                .iter()
                .filter(|r| is_flagged(text(r, "type")))
                .map(|r| {
                    (
                        text(r, "label"),
                        text(r, "value"),
                        text(r, "type"),
                        text(r, "sub"),
                    )
                }),
        )
        .collect();
    heading(&mut doc, &format!("NEEDS A LOOK ({})", flagged.len()));
    if flagged.is_empty() {
        line(&mut doc, "Nothing needs a look.", COLOR_MUTED, 9);
    }
    for (l, v, k, d) in &flagged {
        check_card(&mut doc, l, v, k, d);
    }

    let fine: Vec<&&Value> = cards
        .iter()
        .filter(|c| !is_flagged(text(c, "type")))
        .collect();
    heading(
        &mut doc,
        &format!("LOOKS FINE OR NOT CHECKED ({})", fine.len()),
    );
    for c in fine {
        check_card(
            &mut doc,
            text(c, "label"),
            text(c, "value"),
            text(c, "type"),
            text(c, "detail"),
        );
    }

    let outside: Vec<&&Value> = records
        .iter()
        .filter(|r| !is_flagged(text(r, "type")))
        .collect();
    if !outside.is_empty() {
        heading(&mut doc, "OUTSIDE RECORDS");
        line(
            &mut doc,
            "What official lists, trade fairs and review sites say about this company.",
            COLOR_MUTED,
            8,
        );
        doc.push(elements::Break::new(0.5));
        for r in outside {
            check_card(
                &mut doc,
                text(r, "label"),
                text(r, "value"),
                text(r, "type"),
                text(r, "sub"),
            );
        }
    }

    // Online presence and scam warnings (the web search's rows).
    let social = result
        .get("social_candidates")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    if !social.is_empty() {
        heading(&mut doc, "ONLINE PRESENCE");
        let mut missing = Vec::new();
        for row in social
            .iter()
            .filter(|r| !SCAM_ROWS.contains(&text(r, "platform")))
        {
            let links = row
                .get("candidates")
                .and_then(Value::as_array)
                .unwrap_or(&empty);
            if links.is_empty() {
                missing.push(text(row, "platform"));
                continue;
            }
            let mut table = elements::TableLayout::new(vec![1, 4]);
            let pieces: Vec<String> = links
                .iter()
                .take(3)
                .flat_map(|l| {
                    let mut p = vec![text(l, "title").to_string()];
                    p.extend(url_pieces(text(l, "url")));
                    p
                })
                .collect();
            kv_lines(&mut table, text(row, "platform"), &pieces);
            doc.push(table);
        }
        if !missing.is_empty() {
            line(
                &mut doc,
                &format!("Not found on {}.", missing.join(", ")),
                COLOR_MUTED,
                9,
            );
        }
        heading(&mut doc, "SCAM WARNINGS ONLINE");
        let scam: Vec<&Value> = social
            .iter()
            .filter(|r| SCAM_ROWS.contains(&text(r, "platform")))
            .flat_map(|r| {
                r.get("candidates")
                    .and_then(Value::as_array)
                    .unwrap_or(&empty)
                    .iter()
            })
            .collect();
        if scam.is_empty() {
            line(
                &mut doc,
                "No scam warnings found for this name.",
                COLOR_GOOD,
                9,
            );
        }
        for page in scam.iter().take(8) {
            line(&mut doc, text(page, "title"), COLOR_INK, 9);
            for piece in url_pieces(text(page, "url")) {
                line(&mut doc, &piece, COLOR_MUTED, 8);
            }
        }
    }

    // The vendor's answers
    heading(&mut doc, "THE VENDOR'S ANSWERS");
    let mut table = elements::TableLayout::new(vec![2, 3]);
    let answers = detail
        .get("answers")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    for a in answers {
        let value = text(a, "value");
        let mut shown = if value.is_empty() {
            "-".to_string()
        } else {
            value.to_string()
        };
        if text(a, "field") == "email"
            && !value.is_empty()
            && detail.get("email_confirmed").and_then(Value::as_str) == Some(value)
        {
            shown.push_str("  (confirmed with a code)");
        }
        kv_row(&mut table, text(a, "label"), &shown);
    }
    doc.push(table);

    let docs = detail
        .get("documents")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    if !docs.is_empty() {
        heading(&mut doc, "FILES THE VENDOR SENT");
        for d in docs {
            let kind = match text(d, "kind") {
                "registration" => "Company registration",
                "bank_letter" => "Bank letter",
                other => other,
            };
            line(
                &mut doc,
                &format!(
                    "{kind}: {} ({})",
                    text(d, "file_name"),
                    text(d, "uploaded_on")
                ),
                COLOR_INK,
                9,
            );
        }
    }

    // Before you pay: the supplier steps (no "buyer protection": a vendor
    // is paid directly, not through a platform).
    heading(&mut doc, "BEFORE YOU PAY");
    let steps: Vec<(&str, &str)> = before_you_pay(false, &[])
        .into_iter()
        .filter(|(title, _)| !title.to_lowercase().contains("buyer protection"))
        .collect();
    for (i, (title, body)) in steps.iter().enumerate() {
        doc.push(
            elements::Paragraph::new(format!("{}. {}", i + 1, title)).styled(
                style::Style::new()
                    .bold()
                    .with_font_size(10)
                    .with_color(COLOR_INK),
            ),
        );
        line(&mut doc, body, COLOR_INK, 9);
        doc.push(elements::Break::new(0.5));
    }

    doc.push(elements::Break::new(1.5));
    doc.push(
        elements::Paragraph::new(
            "This report shows Safely's automated checks of the vendor's answers at the time of the check. Only the buyer's team can see it; the vendor never does. It is guidance to help your decision, not a guarantee. Always confirm bank details by phone with someone you know at the company before you pay.",
        )
        .styled(style::Style::new().italic().with_font_size(8).with_color(COLOR_INK)),
    );

    let mut buffer = Vec::new();
    doc.render(&mut buffer)
        .map_err(|e| format!("Could not render the PDF: {e}"))?;
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn detail() -> Value {
        json!({
            "vendor_name": "Mirian Solutions", "vendor_email": "veinstree@gmail.com",
            "buyer_company": "Acme Industrial", "status": "rejected", "risk_score": 90, "risk_level": "high",
            "sent_on": "2026-11-19", "checked_on": "2026-11-19",
            "decided_by": "bilal@acme.com", "decided_on": "2026-11-19", "decision_note": "Person's bank account.",
            "email_confirmed": "veinstree@gmail.com",
            "answers": [
                {"field": "company_name", "label": "Company name", "value": "Mirian Solutions"},
                {"field": "email", "label": "Work email", "value": "veinstree@gmail.com"},
                {"field": "website", "label": "Website", "value": null}
            ],
            "documents": [{"kind": "registration", "file_name": "cnpj.pdf", "uploaded_on": "2026-11-19"}],
            "result": {
                "cards": [
                    {"key": "bank_name", "label": "Bank account name", "value": "Different name", "type": "bad",
                     "detail": "The bank account is in the name \"Bilal Khan\", not \"Mirian Solutions\"."},
                    {"key": "email_code", "label": "Email confirmed", "value": "Confirmed", "type": "good", "detail": "Typed the code."}
                ],
                "records": [{"label": "Sanctions lists", "sub": "Not on the lists.", "value": "Not on the lists", "type": "good"}],
                "social_candidates": [
                    {"platform": "LinkedIn", "found": true, "candidates": [{"platform": "LinkedIn", "title": "Mirian | LinkedIn", "url": "https://linkedin.com/company/mirian?x=1"}]},
                    {"platform": "Facebook", "found": false, "candidates": []},
                    {"platform": "Google search", "found": false, "candidates": []}
                ]
            }
        })
    }

    #[test]
    fn a_vendor_check_becomes_a_pdf() {
        let pdf = generate_vendor_pdf(&detail()).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        if let Ok(path) = std::env::var("SAFELY_VENDOR_PDF_SAMPLE") {
            std::fs::write(path, &pdf).unwrap();
        }
        assert!(pdf.len() > 2000);
    }

    #[test]
    fn no_result_no_pdf() {
        let mut d = detail();
        d["result"] = Value::Null;
        assert!(generate_vendor_pdf(&d).is_err());
    }

    #[test]
    fn no_buyer_protection_step_for_vendors() {
        let steps = before_you_pay(false, &[]);
        assert!(
            steps
                .iter()
                .any(|(t, _)| t.to_lowercase().contains("buyer protection"))
        );
        // The PDF leaves it out (see generate_vendor_pdf).
    }
}
