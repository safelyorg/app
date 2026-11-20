//! Vendor checks: a buyer invites a vendor (a supplier) to fill in a
//! short form, Safely checks the answers, and only the buyer's team sees
//! the result.
//!
//! 1. Invite (dashboard /vendors): one vendor, or a list (CSV). Each
//!    vendor gets a link safely.sh/v/<link>, sent by email by Safely, or
//!    copied / sent on WhatsApp by the buyer. The link works for 14 days
//!    and can be canceled. One reminder email after 3 days.
//! 2. The vendor's form (site/templates/vendor_form.html): no account,
//!    no dashboard, nothing else of Safely. 7 answers (FIELDS), the email
//!    confirmed with a 6-digit code, progress saved, English or
//!    Portuguese. After sending: a thank-you page. The vendor never sees
//!    the result.
//! 3. The check (run_check), in the background:
//!    - the email code was typed (the vendor owns the address);
//!    - free email (gmail.com...) or the company's own domain;
//!    - the email's domain against the website (lookalike.rs), and how
//!      old the website and the email's domain are (domain_age.rs);
//!    - the bank account's name against the company's name;
//!    - the answers against what the buyer already knew;
//!    - scam reports from Safely users with the same company name;
//!    - scam warnings online (osint.rs web search);
//!    - the outside records (sanctions lists, company registers, company
//!      number, VAT, SEC, SAM.gov, chambers, trade fairs, Reclame Aqui,
//!      BBB, website records): the same check_outside_records as a scan.
//!
//!    Then a score and a level (low / caution / high), and an email to
//!    the person who invited.
//! 4. After the result: Approve, Reject, Ask for more info (a link that
//!    only opens the answers to fix), a new link for bank detail
//!    changes, and Watch (watch.rs checks the vendor again later).
//!
//! Team and Enterprise plans only. A team shares one list (the plan
//! owner's account, plan_holder). Each vendor counts as one scan of the
//! plan, when the vendor sends the form; the vendor's later changes are
//! checked again without a new scan. With no scans left the check waits
//! and runs when scans are back (start_vendor_scheduler).

use crate::models::analysis::Signal;
use crate::models::domain_age::Verdict;
use crate::models::fraud_reports::ReportTypes;
use crate::models::lookalike::LookalikeVerdict;
use crate::services::features::analysis::serper_enabled;
use crate::services::features::billing::{is_unlimited_user, reserve_scan};
use crate::services::features::domain_age::{lookup, normalize_domain};
use crate::services::features::email::{VendorEmailText, send_vendor_email};
use crate::services::features::fraud_reports::{already_reported, count_fraud_reports};
use crate::services::features::lookalike::{check_against, check_brands, parse_domain};
use crate::services::features::osint::{
    PlatformCheckResult, SCAM_MENTIONS_FOUND, build_social_presence_matrix, is_web_scam_row,
    run_serper_search,
};
use crate::services::features::team::{clean_invite_email, plan_holder};
use crate::services::features::team_history::{HistoryWho, history_choices};
use crate::services::features::vendor_checks::{
    DOCUMENT_KINDS, DOCUMENT_MAX_BYTES, DocumentReading, address_cards, bank_cards, clean_account,
    company_number_card, document_cards, document_type, fetch_home_page, map_search,
    other_company_pages, phone_country_card, read_document, website_card,
};
use crate::services::features::watch::{TEAM_WATCH_LIMIT, remember_seller_facts};
use crate::services::shared::standards::database_card_enabled;
use crate::services::sources::databases::{
    OUTSIDE_RECORDS, SellerFacts, check_outside_records, sanctions,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Pool, Postgres, query, query_as, query_scalar};
use std::collections::BTreeMap;
use std::time::Duration;
use uuid::Uuid;

/// How long a form link works.
pub const LINK_DAYS: i64 = 14;
/// One reminder email this many days after the invite.
pub const REMIND_AFTER_DAYS: i64 = 3;
/// The most vendors in one list (CSV).
pub const LIST_MAX: usize = 200;
/// An email code works for 15 minutes, 5 tries, one new code a minute.
pub const CODE_MINUTES: i64 = 15;
pub const CODE_TRIES: i32 = 5;
pub const CODE_WAIT_SECONDS: i64 = 60;
/// Invite, reminder and "please fix" emails to one address, per day,
/// from all Safely customers together.
pub const EMAILS_PER_ADDRESS_PER_DAY: i64 = 3;
/// The longest answer kept.
const ANSWER_MAX: usize = 200;

/// The vendor's answers, in the form's order. More can be added later:
/// they are saved in one JSON column (vendor_checks.answers).
pub const FIELDS: &[&str] = &[
    "company_name",
    "country",
    "company_number",
    "email",
    "phone",
    "website",
    "address",
    "bank_holder",
    "bank_account",
    "bank_code",
];
/// The answers that may stay empty.
pub const OPTIONAL_FIELDS: &[&str] = &["website", "address", "bank_account", "bank_code"];

/// Email services anyone can sign up to. A company using one is not a
/// warning on its own, but a real company usually has its own domain.
const FREE_EMAIL_DOMAINS: &[&str] = &[
    "gmail.com",
    "googlemail.com",
    "yahoo.com",
    "yahoo.com.br",
    "yahoo.co.uk",
    "ymail.com",
    "hotmail.com",
    "hotmail.co.uk",
    "hotmail.com.br",
    "outlook.com",
    "outlook.com.br",
    "live.com",
    "msn.com",
    "icloud.com",
    "me.com",
    "aol.com",
    "gmx.com",
    "gmx.de",
    "mail.com",
    "proton.me",
    "protonmail.com",
    "zoho.com",
    "yandex.com",
    "yandex.ru",
    "mail.ru",
    "qq.com",
    "163.com",
    "126.com",
    "sina.com",
    "bol.com.br",
    "uol.com.br",
    "terra.com.br",
    "ig.com.br",
    "web.de",
];

// ───────────────────────── errors ─────────────────────────

#[derive(Debug)]
pub enum VendorError {
    /// Free plan: vendor checks are on Team and Enterprise.
    NeedsPaidPlan,
    NotFound,
    /// A missing or wrong answer; the code says which.
    BadInput(&'static str),
    /// The link was canceled, replaced or is too old.
    LinkClosed,
    /// This can't be done in the check's current state.
    WrongStatus,
    CodeWrong,
    CodeExpired,
    CodeTooManyTries,
    /// A new code was sent less than a minute ago.
    CodeWait,
    /// The email in the form was not confirmed with the code.
    EmailNotConfirmed,
    WatchLimit(i64),
    /// This person already reported this vendor.
    AlreadyReported,
    /// This address already got the most Safely emails for today.
    EmailLimit,
    /// A file that is not a PDF, JPG, PNG or WebP, or over 5 MB.
    BadFile,
    Database(String),
}

impl From<sqlx::Error> for VendorError {
    fn from(e: sqlx::Error) -> Self {
        VendorError::Database(e.to_string())
    }
}

impl VendorError {
    /// The short code the dashboard and the form read.
    pub fn code(&self) -> &'static str {
        match self {
            VendorError::NeedsPaidPlan => "needs_paid_plan",
            VendorError::NotFound => "not_found",
            VendorError::BadInput(code) => code,
            VendorError::LinkClosed => "link_closed",
            VendorError::WrongStatus => "wrong_status",
            VendorError::CodeWrong => "code_wrong",
            VendorError::CodeExpired => "code_expired",
            VendorError::CodeTooManyTries => "code_too_many_tries",
            VendorError::CodeWait => "code_wait",
            VendorError::EmailNotConfirmed => "email_not_confirmed",
            VendorError::WatchLimit(_) => "watch_limit_reached",
            VendorError::AlreadyReported => "already_reported",
            VendorError::EmailLimit => "email_limit",
            VendorError::BadFile => "bad_file",
            VendorError::Database(_) => "internal_error",
        }
    }
}

// ───────────────────────── small helpers ─────────────────────────

/// A new link secret: 64 random hex characters.
pub fn new_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// What is saved for a link: its SHA-256, never the link itself.
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.trim().as_bytes()))
}

/// The form's address for a link secret.
pub fn form_link(token: &str) -> String {
    let base = std::env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| "https://safely.sh".to_string());
    format!("{}/v/{}", base.trim_end_matches('/'), token)
}

/// A 6-digit email code.
pub fn new_code() -> String {
    let n = u128::from_le_bytes(*Uuid::new_v4().as_bytes()) % 1_000_000;
    format!("{n:06}")
}

pub fn hash_code(check_id: Uuid, code: &str) -> String {
    hex::encode(Sha256::digest(
        format!("{check_id}:{}", code.trim()).as_bytes(),
    ))
}

/// "en" or "pt" (anything starting with "pt", like "pt-br").
pub fn clean_language(text: Option<&str>) -> &'static str {
    match text {
        Some(t) if t.trim().to_lowercase().starts_with("pt") => "pt",
        _ => "en",
    }
}

/// An answer as it is kept: spaces tidied, at most 200 characters.
pub fn clean_answer(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.chars().take(ANSWER_MAX).collect()
}

/// The domain of an email address ("ana@Supplier.com" -> "supplier.com").
pub fn email_domain(email: &str) -> Option<String> {
    let (_, domain) = email.trim().rsplit_once('@')?;
    let domain = domain.trim().trim_end_matches('.').to_lowercase();
    (domain.contains('.') && !domain.starts_with('.')).then_some(domain)
}

pub fn is_free_email(domain: &str) -> bool {
    FREE_EMAIL_DOMAINS.contains(&domain.to_lowercase().as_str())
}

/// True when two company names are the same company: the same name once
/// legal forms and punctuation are left out ("ACME Ltda." = "Acme"), or
/// one name inside the other ("Acme" in "Acme Trading"). Names with too
/// little left to compare (under 4 letters) are compared as written.
pub fn names_match(a: &str, b: &str) -> bool {
    let (a, b) = (&fold_accents(a), &fold_accents(b));
    let plain = |s: &str| {
        s.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
    };
    let (ca, cb) = match (sanctions::compared_name(a), sanctions::compared_name(b)) {
        (Some(x), Some(y)) => (x, y),
        _ => return !plain(a).is_empty() && plain(a) == plain(b),
    };
    if ca == cb {
        return true;
    }
    let (short, long) = if ca.len() <= cb.len() {
        (&ca, &cb)
    } else {
        (&cb, &ca)
    };
    let words: Vec<&str> = long.split_whitespace().collect();
    let short_words: Vec<&str> = short.split_whitespace().collect();
    words
        .windows(short_words.len().max(1))
        .any(|w| w == short_words.as_slice())
        || plain(long) == plain(short)
}

/// "Comércio" -> "Comercio", so a name typed with or without accents
/// is the same name.
pub fn fold_accents(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'Á' | 'À' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'A',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'É' | 'È' | 'Ê' | 'Ë' => 'E',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'O',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
            'ç' => 'c',
            'Ç' => 'C',
            'ñ' => 'n',
            'Ñ' => 'N',
            other => other,
        })
        .collect()
}

/// True when two phone numbers are the same: the last 8 digits match
/// (so "+55 11 91234-5678" = "(11) 91234-5678").
pub fn phones_match(a: &str, b: &str) -> bool {
    let digits = |s: &str| s.chars().filter(char::is_ascii_digit).collect::<String>();
    let (da, db) = (digits(a), digits(b));
    if da.len() < 8 || db.len() < 8 {
        return da == db;
    }
    da[da.len() - 8..] == db[db.len() - 8..]
}

/// True when two countries are the same ("Brasil" = "Brazil" when the
/// country files know both; else compared as written).
pub fn countries_match(a: &str, b: &str) -> bool {
    use crate::services::sources::databases::country_in_words;
    let words = |s: &str| {
        country_in_words(Some(s))
            .unwrap_or_else(|| s.to_string())
            .trim()
            .to_lowercase()
    };
    words(a) == words(b)
}

fn websites_match(a: &str, b: &str) -> bool {
    match (normalize_domain(a), normalize_domain(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a.trim().eq_ignore_ascii_case(b.trim()),
    }
}

/// The field's name for the buyer, in English.
pub fn field_label(field: &str) -> &'static str {
    match field {
        "company_name" => "Company name",
        "country" => "Country",
        "company_number" => "Company number",
        "email" => "Work email",
        "phone" => "Phone",
        "website" => "Website",
        "bank_holder" => "Bank account name",
        "address" => "Company address",
        "bank_account" => "Bank account number (IBAN)",
        "bank_code" => "Bank code (SWIFT/BIC or routing)",
        _ => "Answer",
    }
}

fn field_label_pt(field: &str) -> &'static str {
    match field {
        "company_name" => "Nome da empresa",
        "country" => "País",
        "company_number" => "Número de registro da empresa",
        "email" => "E-mail de trabalho",
        "phone" => "Telefone",
        "website" => "Site",
        "bank_holder" => "Nome do titular da conta bancária",
        "address" => "Endereço da empresa",
        "bank_account" => "Número da conta (IBAN)",
        "bank_code" => "Código do banco (SWIFT/BIC)",
        _ => "Resposta",
    }
}

/// Answers as a field -> text map. Unknown fields and non-text values
/// are left out.
pub fn answers_from(value: &Value) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(map) = value.as_object() {
        for field in FIELDS {
            if let Some(text) = map.get(*field).and_then(Value::as_str) {
                let text = clean_answer(text);
                if !text.is_empty() {
                    out.insert(field.to_string(), text);
                }
            }
        }
    }
    out
}

fn fields_from(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .filter(|f| FIELDS.contains(f))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The answers still missing before the form can be sent.
pub fn missing_fields(answers: &BTreeMap<String, String>) -> Vec<&'static str> {
    FIELDS
        .iter()
        .filter(|f| !OPTIONAL_FIELDS.contains(f))
        .filter(|f| answers.get(**f).is_none_or(|v| v.trim().is_empty()))
        .copied()
        .collect()
}

// ───────────────────────── the result ─────────────────────────

/// One line of the result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VendorCard {
    /// "email_code", "free_email", "bank_name", "known_country"...
    pub key: String,
    pub label: String,
    pub value: String,
    /// "good", "caution", "bad" or "info".
    #[serde(rename = "type")]
    pub kind: String,
    pub detail: String,
}

fn card(key: &str, label: &str, value: &str, kind: &str, detail: String) -> VendorCard {
    VendorCard {
        key: key.to_string(),
        label: label.to_string(),
        value: value.to_string(),
        kind: kind.to_string(),
        detail,
    }
}

/// What the buyer knew when inviting.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Known {
    pub name: Option<String>,
    pub email: Option<String>,
    pub country: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
}

/// The checks that need nothing but the answers: the email code, free
/// email, the bank account's name, and what the buyer already knew.
pub fn answer_cards(
    known: &Known,
    answers: &BTreeMap<String, String>,
    email_confirmed: Option<&str>,
) -> Vec<VendorCard> {
    let mut cards = Vec::new();
    let get = |f: &str| answers.get(f).map(String::as_str).filter(|v| !v.is_empty());
    let email = get("email").unwrap_or("");

    let confirmed = email_confirmed.is_some_and(|c| c.eq_ignore_ascii_case(email));
    cards.push(if confirmed {
        card(
            "email_code",
            "Email confirmed",
            "Confirmed",
            "good",
            format!("The vendor typed the code Safely sent to {email}."),
        )
    } else {
        card(
            "email_code",
            "Email confirmed",
            "Not confirmed",
            "bad",
            "The vendor did not confirm the email address with the code.".to_string(),
        )
    });

    if let Some(domain) = email_domain(email) {
        cards.push(if is_free_email(&domain) {
            card(
                "free_email",
                "Email address",
                "Free email",
                "caution",
                format!(
                    "{domain} is a free email service anyone can sign up to. Real companies usually write from their own domain. (Its age is not checked: a free service says nothing about the company.)"
                ),
            )
        } else {
            card(
                "free_email",
                "Email address",
                "Company domain",
                "good",
                format!("The email is on the company's own domain, {domain}."),
            )
        });
    }

    if let (Some(company), Some(holder)) = (get("company_name"), get("bank_holder")) {
        cards.push(if names_match(company, holder) {
            card(
                "bank_name",
                "Bank account name",
                "Same name",
                "good",
                format!("The bank account is in the company's name ({holder})."),
            )
        } else {
            card(
                "bank_name",
                "Bank account name",
                "Different name",
                "bad",
                format!(
                    "The bank account is in the name \"{holder}\", not \"{company}\". Ask why before you pay: money sent there may not reach this company."
                ),
            )
        });
    }

    // What the buyer knew, against the answers.
    type Same = fn(&str, &str) -> bool;
    let pairs: [(&str, &str, Option<&String>, Same); 5] = [
        ("name", "company_name", known.name.as_ref(), names_match),
        // A colleague at the same company (same domain) is the same.
        ("email", "email", known.email.as_ref(), |a, b| {
            a.trim().eq_ignore_ascii_case(b.trim())
                || matches!((email_domain(a), email_domain(b)), (Some(x), Some(y)) if x == y && !is_free_email(&x))
        }),
        (
            "country",
            "country",
            known.country.as_ref(),
            countries_match,
        ),
        ("phone", "phone", known.phone.as_ref(), phones_match),
        ("website", "website", known.website.as_ref(), websites_match),
    ];
    let mut compared = 0;
    let mut differ = 0;
    for (key, field, had, same) in pairs {
        let (Some(had), Some(now)) = (had.map(|h| h.trim()).filter(|h| !h.is_empty()), get(field))
        else {
            continue;
        };
        compared += 1;
        if !same(had, now) {
            differ += 1;
            // Another country is a big change (another company, or a
            // vendor hiding where it is): red. Other answers: yellow.
            let company_to_free = key == "email"
                && email_domain(had).is_some_and(|d| !is_free_email(&d))
                && email_domain(now).is_some_and(|d| is_free_email(&d));
            let (kind, more) = if key == "country" {
                (
                    "bad",
                    " A company in another country than you expected may be another company. Ask why before you pay.",
                )
            } else if company_to_free {
                (
                    "bad",
                    " You invited a company address, but the form was filled from a free email. The link may have been forwarded or taken over: confirm by phone with someone you know at the company.",
                )
            } else {
                ("caution", "")
            };
            cards.push(card(
                &format!("known_{key}"),
                field_label(field),
                "Not what you had",
                kind,
                format!("You had \"{had}\". The vendor wrote \"{now}\".{more}"),
            ));
        }
    }
    if compared > 0 && differ == 0 {
        cards.push(card(
            "known_all",
            "What you already knew",
            "Matches",
            "good",
            "The answers match what you had for this vendor.".to_string(),
        ));
    }

    // No website and a free email together: nothing ties this vendor to
    // a real company online. Each alone is common; both together are a
    // warning (like the scans' combination rules).
    if get("website").is_none() && email_domain(email).is_some_and(|d| is_free_email(&d)) {
        cards.push(card(
            "no_website_free_email",
            "No website and a free email",
            "Nothing online of its own",
            "caution",
            "The vendor has no website and writes from a free email. A real company usually has at least its own website or email domain. Ask for one, or check the company another way (registration document, video call).".to_string(),
        ));
    }
    if get("website").is_none() {
        cards.push(card(
            "website_missing",
            "Website",
            "None given",
            "info",
            "The vendor did not give a website, so its age, history and traffic were not checked."
                .to_string(),
        ));
    }
    cards
}

/// The card for a domain's age (the website, or the email's domain).
pub fn age_card(
    key: &str,
    label: &str,
    domain: &str,
    answer: Option<(Verdict, Option<&str>, Option<i64>)>,
) -> VendorCard {
    let Some((verdict, registered, days)) = answer else {
        return card(
            key,
            label,
            "Not checked",
            "info",
            format!("The registry for {domain} did not answer. Try again later."),
        );
    };
    let since = registered.unwrap_or("an unknown date");
    let days = days.map(|d| format!(", {d} days ago")).unwrap_or_default();
    match verdict {
        Verdict::New => card(
            key,
            label,
            "New",
            "bad",
            format!("{domain} was registered on {since}{days}. Scams often use new domains."),
        ),
        Verdict::Young => card(
            key,
            label,
            "Young",
            "caution",
            format!("{domain} was registered on {since}{days}, less than 2 years ago."),
        ),
        Verdict::Established => card(
            key,
            label,
            "Established",
            "good",
            format!("{domain} was registered on {since}{days}."),
        ),
        Verdict::Unknown => card(
            key,
            label,
            "Date not shared",
            "info",
            format!("The registry for {domain} does not share when it was registered."),
        ),
    }
}

/// The email's domain against the website's.
pub fn email_vs_website_card(email_domain: &str, website: &str) -> Option<VendorCard> {
    let email = parse_domain(email_domain)?;
    let site = parse_domain(website)?;
    let answer = check_against(&email, &site);
    let real = site.site();
    Some(match answer.verdict {
        LookalikeVerdict::Same => card(
            "email_website",
            "Email and website",
            "Same domain",
            "good",
            format!("The email is on the website's own domain, {real}."),
        ),
        LookalikeVerdict::Lookalike => card(
            "email_website",
            "Email and website",
            "Lookalike",
            "bad",
            format!(
                "{} looks like a copy of {real}. Fake suppliers use lookalike domains to pass as a real company.",
                email.site()
            ),
        ),
        LookalikeVerdict::Close => card(
            "email_website",
            "Email and website",
            "Close",
            "caution",
            format!("{} is close to {real}, but not the same.", email.site()),
        ),
        LookalikeVerdict::Different | LookalikeVerdict::NoBrand => card(
            "email_website",
            "Email and website",
            "Different domain",
            "caution",
            format!(
                "The email ({}) is not on the website's domain ({real}). Ask which one the company really uses.",
                email.site()
            ),
        ),
    })
}

/// A score from 0 (nothing found) to 100, and the level.
pub fn score(cards: &[VendorCard], records: &[Signal]) -> (i32, &'static str) {
    let mut points = 0;
    let mut sanctioned = false;
    for c in cards {
        points += match c.kind.as_str() {
            "bad" => 25,
            "caution" => 8,
            _ => 0,
        };
    }
    for r in records {
        if r.signal_type == "bad" && r.label == sanctions::LABEL {
            sanctioned = true;
        }
        points += match r.signal_type.as_str() {
            "bad" => 25,
            "caution" => 8,
            _ => 0,
        };
    }
    let points = if sanctioned { 100 } else { points.min(100) };
    let level = if sanctioned || points >= 50 {
        "high"
    } else if points >= 20 {
        "caution"
    } else {
        "low"
    };
    (points, level)
}

// ───────────────────────── who can use it ─────────────────────────

/// The team's account (plan_holder) when the person may use vendor
/// checks: a paid Team or Enterprise plan, or a test account.
pub async fn team_for(pool: &Pool<Postgres>, viewer: Uuid) -> Result<Uuid, VendorError> {
    match plan_of(pool, viewer).await? {
        (owner, true) => Ok(owner),
        (_, false) => Err(VendorError::NeedsPaidPlan),
    }
}

/// The team's account (plan_holder) and whether its plan is paid (Team,
/// Enterprise or the unlimited list). A Free account can still LOOK at
/// the vendors it invited while it was paid, in short (vendor_list,
/// vendor_detail); everything else needs a paid plan (team_for).
pub async fn plan_of(pool: &Pool<Postgres>, viewer: Uuid) -> Result<(Uuid, bool), VendorError> {
    let owner = plan_holder(pool, viewer).await;
    if is_unlimited_user(pool, viewer).await || is_unlimited_user(pool, owner).await {
        return Ok((owner, true));
    }
    let paid: bool = query_scalar(
        "SELECT EXISTS (SELECT 1 FROM subscriptions WHERE user_id = $1 AND status = 'active')",
    )
    .bind(owner)
    .fetch_one(pool)
    .await?;
    Ok((owner, paid))
}

/// How many reasons a Free plan sees on a vendor, like a scan's top 3.
pub const FREE_VENDOR_REASONS: usize = 3;

/// The Free plan's short vendor result: the score and level, and the 3
/// most important checks (warnings first, then checks that passed).
/// Outside records, online presence, scam warnings, the vendor's
/// answers, files and earlier answers are left out, so they never reach
/// a Free browser. "counts" keeps how many checks there were.
pub fn free_vendor_view(detail: &mut Value) {
    let rank = |kind: &str| match kind {
        "bad" => 0,
        "caution" => 1,
        "good" => 2,
        _ => 3,
    };
    if let Some(result) = detail.get_mut("result").filter(|r| r.is_object()) {
        let mut cards: Vec<Value> = result["cards"].as_array().cloned().unwrap_or_default();
        let records: Vec<Value> = result["records"].as_array().cloned().unwrap_or_default();
        let kinds: Vec<String> = cards
            .iter()
            .chain(records.iter())
            .map(|c| c["type"].as_str().unwrap_or("").to_string())
            .collect();
        let flagged = kinds
            .iter()
            .filter(|k| *k == "bad" || *k == "caution")
            .count();
        cards.sort_by_key(|c| rank(c["type"].as_str().unwrap_or("")));
        cards.retain(|c| c["type"] != "info");
        cards.truncate(FREE_VENDOR_REASONS);
        *result = json!({
            "cards": cards,
            "records": [],
            "counts": { "total": kinds.len(), "flagged": flagged },
        });
    }
    detail["answers"] = json!([]);
    detail["known"] = json!({});
    detail["previous"] = json!([]);
    detail["documents"] = json!([]);
    detail["ask_fields"] = json!([]);
    detail["ask_note"] = Value::Null;
    detail["full_result"] = Value::Bool(false);
}

#[derive(Debug, Clone, FromRow)]
pub struct CheckRow {
    pub id: Uuid,
    pub team_owner_id: Uuid,
    pub invited_by: Option<Uuid>,
    pub buyer_company: String,
    pub vendor_name: String,
    pub vendor_email: String,
    pub status: String,
    pub language: String,
    pub send_email: bool,
    pub known: Value,
    pub answers: Value,
    pub ask_fields: Value,
    pub ask_note: Option<String>,
    pub previous: Value,
    pub email_code_hash: Option<String>,
    pub email_code_for: Option<String>,
    pub email_code_sent_at: Option<DateTime<Utc>>,
    pub email_code_tries: i32,
    pub email_confirmed: Option<String>,
    pub result: Option<Value>,
    pub risk_score: Option<i32>,
    pub risk_level: Option<String>,
    pub decision_note: Option<String>,
    pub decided_by: Option<Uuid>,
    pub seller_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub opened_at: Option<DateTime<Utc>>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub checked_at: Option<DateTime<Utc>>,
    pub decided_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub asked_at: DateTime<Utc>,
    pub reminder_sent_at: Option<DateTime<Utc>>,
    pub scan_counted: bool,
    pub updated_at: DateTime<Utc>,
}

async fn load(pool: &Pool<Postgres>, id: Uuid) -> Result<CheckRow, VendorError> {
    query_as("SELECT * FROM vendor_checks WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(VendorError::NotFound)
}

/// One of the team's vendor checks.
async fn load_for_team(
    pool: &Pool<Postgres>,
    viewer: Uuid,
    id: Uuid,
) -> Result<CheckRow, VendorError> {
    let owner = team_for(pool, viewer).await?;
    let row = load(pool, id).await?;
    if row.team_owner_id != owner {
        return Err(VendorError::NotFound);
    }
    Ok(row)
}

/// True when Safely may send one more invite, reminder or "please fix"
/// email to this address today (EMAILS_PER_ADDRESS_PER_DAY), and counts
/// it. The count is for the address, whoever sends it.
pub async fn may_email_vendor(pool: &Pool<Postgres>, email: &str) -> bool {
    let email = email.trim().to_lowercase();
    let sent: i64 = query_scalar(
        "SELECT COUNT(*) FROM vendor_email_sends WHERE email = $1 AND sent_at > NOW() - INTERVAL '1 day'",
    )
    .bind(&email)
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    if sent >= EMAILS_PER_ADDRESS_PER_DAY {
        eprintln!("Safely: vendor email not sent - this address already got {sent} today");
        return false;
    }
    let _ = query("INSERT INTO vendor_email_sends (email) VALUES ($1)")
        .bind(&email)
        .execute(pool)
        .await;
    true
}

/// Makes a new form link for a check. `close_old`: the old links stop
/// working ("New link"); otherwise they keep working (a reminder).
async fn add_link(pool: &Pool<Postgres>, id: Uuid, close_old: bool) -> Result<String, VendorError> {
    if close_old {
        query("UPDATE vendor_check_links SET revoked_at = NOW() WHERE vendor_check_id = $1 AND revoked_at IS NULL")
            .bind(id)
            .execute(pool)
            .await?;
    }
    let token = new_token();
    query("INSERT INTO vendor_check_links (token_hash, vendor_check_id) VALUES ($1, $2)")
        .bind(hash_token(&token))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(form_link(&token))
}

// ───────────────────────── the buyer ─────────────────────────

/// One vendor to invite.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct NewVendor {
    pub name: String,
    pub email: String,
    pub country: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct InviteRequest {
    /// The buyer's company, shown on the vendor's form and emails.
    pub buyer_company: String,
    /// "en" or "pt": the form's and the emails' language.
    pub language: Option<String>,
    /// False: no email, the buyer sends the link (copy / WhatsApp).
    pub send_email: Option<bool>,
    pub vendors: Vec<NewVendor>,
}

fn optional(text: &Option<String>) -> Option<String> {
    text.as_deref().map(clean_answer).filter(|t| !t.is_empty())
}

/// Invites one vendor or a list. Returns each vendor's link (shown once,
/// to copy or send on WhatsApp) and whether the email went out.
pub async fn invite_vendors(
    pool: &Pool<Postgres>,
    viewer: Uuid,
    req: InviteRequest,
) -> Result<Value, VendorError> {
    let owner = team_for(pool, viewer).await?;
    let buyer = clean_answer(&req.buyer_company);
    if buyer.is_empty() {
        return Err(VendorError::BadInput("buyer_company_missing"));
    }
    if req.vendors.is_empty() {
        return Err(VendorError::BadInput("no_vendors"));
    }
    if req.vendors.len() > LIST_MAX {
        return Err(VendorError::BadInput("too_many_vendors"));
    }
    let mut clean = Vec::new();
    for v in &req.vendors {
        let name = clean_answer(&v.name);
        let email = clean_invite_email(&v.email).ok_or(VendorError::BadInput("bad_email"))?;
        if name.is_empty() {
            return Err(VendorError::BadInput("name_missing"));
        }
        let known = Known {
            name: Some(name.clone()),
            email: Some(email.clone()),
            country: optional(&v.country),
            phone: optional(&v.phone),
            website: optional(&v.website),
        };
        clean.push((name, email, known));
    }
    let language = clean_language(req.language.as_deref());
    let send_email = req.send_email.unwrap_or(true);
    let reply_to: Option<String> = query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(viewer)
        .fetch_optional(pool)
        .await?;

    let mut out = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (name, email, known) in clean {
        // The same email twice in one list: once.
        if seen.contains(&email) {
            continue;
        }
        seen.push(email.clone());
        // One open check per email in a team: while it waits for the
        // vendor, is being checked or waits for your decision, it is
        // opened instead of inviting again. After a decision (or when the
        // link expired or was canceled) the vendor can be invited again.
        let open: Option<(Uuid, String)> = query_as(
            "SELECT id, vendor_name FROM vendor_checks
             WHERE team_owner_id = $1 AND lower(vendor_email) = $2
               AND (status IN ('checking', 'no_scans', 'ready')
                    OR (status IN ('invited', 'opened', 'more_info') AND expires_at > NOW()))
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(owner)
        .bind(&email)
        .fetch_optional(pool)
        .await?;
        if let Some((id, existing)) = open {
            out.push(
                json!({ "id": id, "name": existing, "email": email, "link": null,
                              "email_sent": false, "already": true }),
            );
            continue;
        }
        let id: Uuid = query_scalar(
            "INSERT INTO vendor_checks
                (team_owner_id, invited_by, buyer_company, vendor_name, vendor_email,
                 language, send_email, known, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW() + make_interval(days => $9))
             RETURNING id",
        )
        .bind(owner)
        .bind(viewer)
        .bind(&buyer)
        .bind(&name)
        .bind(&email)
        .bind(language)
        .bind(send_email)
        .bind(serde_json::to_value(&known).unwrap_or(json!({})))
        .bind(LINK_DAYS as i32)
        .fetch_one(pool)
        .await?;
        let link = add_link(pool, id, false).await?;
        let mut sent = false;
        let mut limited = false;
        if send_email {
            if may_email_vendor(pool, &email).await {
                let text = invite_text(language, &buyer, &name, &link, false);
                match send_vendor_email(&email, &text, reply_to.as_deref()).await {
                    Ok(()) => sent = true,
                    Err(e) => eprintln!("Safely: could not send a vendor invite email: {e:?}"),
                }
            } else {
                limited = true;
            }
        }
        out.push(
            json!({ "id": id, "name": name, "email": email, "link": link,
                          "email_sent": sent, "email_limited": limited }),
        );
    }
    Ok(json!({ "invited": out }))
}

/// The team's vendor list, newest first.
/// The vendor list, like History: "My vendors" (the ones I invited),
/// "All team vendors" (the team owner only), or one teammate's vendors.
/// The same people and rules as History's dropdown (team_history.rs);
/// a choice the viewer may not see falls back to "My vendors". Any
/// teammate can still open any of the team's vendors.
pub async fn vendor_list(
    pool: &Pool<Postgres>,
    viewer: Uuid,
    who: HistoryWho,
) -> Result<Value, VendorError> {
    // Free: the vendors invited while the plan was paid are still
    // listed ("paid": false; no invites or actions).
    let (owner, paid) = plan_of(pool, viewer).await?;
    let (in_team, can_see_all, people) = history_choices(pool, viewer).await?;
    let who = match who {
        HistoryWho::AllTeam if in_team && can_see_all => HistoryWho::AllTeam,
        HistoryWho::Person(id) if in_team && people.iter().any(|p| p.id == id) => {
            HistoryWho::Person(id)
        }
        _ => HistoryWho::Mine,
    };
    let only: Option<Uuid> = match who {
        HistoryWho::AllTeam => None,
        HistoryWho::Person(id) => Some(id),
        HistoryWho::Mine => Some(viewer),
    };
    let rows: Vec<(CheckRow, Option<String>, Option<String>)> = {
        let rows: Vec<CheckRow> = query_as(
            "SELECT * FROM vendor_checks
             WHERE team_owner_id = $1 AND ($2::uuid IS NULL OR invited_by = $2)
             ORDER BY created_at DESC LIMIT 1000",
        )
        .bind(owner)
        .bind(only)
        .fetch_all(pool)
        .await?;
        let people: Vec<(Uuid, String, Option<String>)> =
            query_as("SELECT id, lower(email), name FROM users WHERE id = ANY($1)")
                .bind(rows.iter().filter_map(|r| r.invited_by).collect::<Vec<_>>())
                .fetch_all(pool)
                .await?;
        rows.into_iter()
            .map(|r| {
                let who = people.iter().find(|p| Some(p.0) == r.invited_by);
                let email = who.map(|p| p.1.clone());
                let name = who
                    .and_then(|p| p.2.clone())
                    .filter(|n| !n.trim().is_empty());
                (r, email, name)
            })
            .collect()
    };
    // The company name last used by anyone in the team (the invite box).
    let buyer_company: Option<String> = query_scalar(
        "SELECT buyer_company FROM vendor_checks WHERE team_owner_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(owner)
    .fetch_optional(pool)
    .await?;
    let watched: Vec<Uuid> =
        query_scalar("SELECT seller_id FROM watched_suppliers WHERE team_owner_id = $1")
            .bind(owner)
            .fetch_all(pool)
            .await?;
    let now = Utc::now();
    let mut counts = BTreeMap::from([
        ("waiting", 0),
        ("to_review", 0),
        ("approved", 0),
        ("flagged", 0),
    ]);
    let items: Vec<Value> = rows
        .into_iter()
        .map(|(r, by_email, by_name)| {
            let status = shown_status(&r, now);
            match status.as_str() {
                "invited" | "opened" | "more_info" | "checking" | "no_scans" => {
                    *counts.entry("waiting").or_default() += 1
                }
                "ready" => *counts.entry("to_review").or_default() += 1,
                "approved" => *counts.entry("approved").or_default() += 1,
                _ => {}
            }
            if r.risk_level.as_deref() == Some("high") && status != "rejected" {
                *counts.entry("flagged").or_default() += 1;
            }
            let answers = answers_from(&r.answers);
            json!({
                "id": r.id,
                "vendor_name": answers.get("company_name").cloned().unwrap_or(r.vendor_name.clone()),
                "vendor_email": r.vendor_email,
                "status": status,
                "risk_level": r.risk_level,
                "risk_score": r.risk_score,
                "invited_on": r.created_at.format("%Y-%m-%d").to_string(),
                "sent_on": r.submitted_at.map(|d| d.format("%Y-%m-%d").to_string()),
                "expires_on": r.expires_at.format("%Y-%m-%d").to_string(),
                "invited_by": by_email,
                "invited_by_name": by_name,
                "invited_by_you": r.invited_by == Some(viewer),
                "watching": r.seller_id.is_some_and(|s| watched.contains(&s)),
            })
        })
        .collect();
    let who_text = match who {
        HistoryWho::AllTeam => "team".to_string(),
        HistoryWho::Person(id) => id.to_string(),
        HistoryWho::Mine => "me".to_string(),
    };
    Ok(json!({
        "paid": paid,
        "who": who_text,
        "team": in_team,
        "can_see_all": can_see_all,
        "people": people.iter().map(|p| json!({ "id": p.id, "email": p.email, "is_owner": p.is_owner })).collect::<Vec<_>>(),
        "buyer_company": buyer_company,
        "counts": counts,
        "items": items,
    }))
}

/// The status the buyer sees: an open link past its date is "expired"
/// even before the background job marks it.
fn shown_status(r: &CheckRow, now: DateTime<Utc>) -> String {
    let open = matches!(r.status.as_str(), "invited" | "opened" | "more_info");
    if open && r.expires_at < now {
        "expired".to_string()
    } else {
        r.status.clone()
    }
}

/// Everything about one vendor check, for the buyer.
pub async fn vendor_detail(
    pool: &Pool<Postgres>,
    viewer: Uuid,
    id: Uuid,
) -> Result<Value, VendorError> {
    // Free plans may still open their team's vendors, in short.
    let (owner, paid) = plan_of(pool, viewer).await?;
    let r = load(pool, id).await?;
    if r.team_owner_id != owner {
        return Err(VendorError::NotFound);
    }
    let answers = answers_from(&r.answers);
    let known: Known = serde_json::from_value(r.known.clone()).unwrap_or_default();
    let fields: Vec<Value> = FIELDS
        .iter()
        .map(|f| json!({ "field": f, "label": field_label(f), "value": answers.get(*f) }))
        .collect();
    let decided_by: Option<String> = match r.decided_by {
        Some(who) => {
            query_scalar("SELECT lower(email) FROM users WHERE id = $1")
                .bind(who)
                .fetch_optional(pool)
                .await?
        }
        None => None,
    };
    let invited_by: Option<String> = match r.invited_by {
        Some(who) => {
            query_scalar("SELECT lower(email) FROM users WHERE id = $1")
                .bind(who)
                .fetch_optional(pool)
                .await?
        }
        None => None,
    };
    // Watching, reported by this person, and all Safely reports on the
    // company (from anyone), for the page's side card.
    let (watching, reported_by_you, report_count) = match r.seller_id {
        Some(seller) => (
            query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM watched_suppliers WHERE team_owner_id = $1 AND seller_id = $2)",
            )
            .bind(r.team_owner_id)
            .bind(seller)
            .fetch_one(pool)
            .await?,
            already_reported(pool, viewer, seller).await?,
            count_fraud_reports(pool, seller).await?,
        ),
        None => (false, false, 0),
    };
    let documents: Vec<Value> = query_as::<_, (Uuid, String, String, i32, DateTime<Utc>)>(
        "SELECT id, kind, file_name, size_bytes, created_at FROM vendor_documents
         WHERE vendor_check_id = $1 ORDER BY kind",
    )
    .bind(r.id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(doc, kind, name, size, at)| {
        json!({ "id": doc, "kind": kind, "file_name": name, "size": size,
                "uploaded_on": at.format("%Y-%m-%d").to_string() })
    })
    .collect();
    let mut detail = json!({
        "id": r.id,
        "vendor_name": answers.get("company_name").cloned().unwrap_or(r.vendor_name.clone()),
        "invited_as": r.vendor_name,
        "vendor_email": r.vendor_email,
        "buyer_company": r.buyer_company,
        "status": shown_status(&r, Utc::now()),
        "language": r.language,
        "send_email": r.send_email,
        "known": known,
        "answers": fields,
        "email_confirmed": r.email_confirmed,
        "ask_fields": fields_from(&r.ask_fields),
        "ask_note": r.ask_note,
        "previous": r.previous,
        "result": r.result,
        "risk_score": r.risk_score,
        "risk_level": r.risk_level,
        "decision_note": r.decision_note,
        "decided_by": decided_by,
        "decided_on": r.decided_at.map(|d| d.format("%Y-%m-%d").to_string()),
        "invited_on": r.created_at.format("%Y-%m-%d").to_string(),
        "opened_on": r.opened_at.map(|d| d.format("%Y-%m-%d").to_string()),
        "sent_on": r.submitted_at.map(|d| d.format("%Y-%m-%d").to_string()),
        "checked_on": r.checked_at.map(|d| d.format("%Y-%m-%d").to_string()),
        "expires_on": r.expires_at.format("%Y-%m-%d").to_string(),
        "reminder_sent": r.reminder_sent_at.is_some(),
        "watching": watching,
        "reported_by_you": reported_by_you,
        "report_count": report_count,
        "invited_by": invited_by,
        "invited_by_you": r.invited_by == Some(viewer),
        "documents": documents,
        "full_result": true,
    });
    if !paid {
        free_vendor_view(&mut detail);
    }
    Ok(detail)
}

/// Cancels a vendor's link (before the vendor sends the form).
pub async fn cancel_vendor(
    pool: &Pool<Postgres>,
    viewer: Uuid,
    id: Uuid,
) -> Result<(), VendorError> {
    let r = load_for_team(pool, viewer, id).await?;
    if !matches!(
        r.status.as_str(),
        "invited" | "opened" | "more_info" | "expired"
    ) {
        return Err(VendorError::WrongStatus);
    }
    query("UPDATE vendor_check_links SET revoked_at = NOW() WHERE vendor_check_id = $1 AND revoked_at IS NULL")
        .bind(id)
        .execute(pool)
        .await?;
    // A vendor who already sent the form once keeps that result.
    let status = if r.result.is_some() {
        "ready"
    } else {
        "canceled"
    };
    query("UPDATE vendor_checks SET status = $2, ask_fields = '[]', ask_note = NULL, updated_at = NOW() WHERE id = $1")
        .bind(id)
        .bind(status)
        .execute(pool)
        .await?;
    Ok(())
}

/// A new link: the old ones stop working and the 14 days start again.
/// Also opens an expired or canceled invite again.
pub async fn new_vendor_link(
    pool: &Pool<Postgres>,
    viewer: Uuid,
    id: Uuid,
) -> Result<String, VendorError> {
    let r = load_for_team(pool, viewer, id).await?;
    let status = match r.status.as_str() {
        "invited" | "opened" | "more_info" => r.status.clone(),
        "expired" | "canceled"
            if r.submitted_at.is_some() && !fields_from(&r.ask_fields).is_empty() =>
        {
            "more_info".to_string()
        }
        "expired" | "canceled" if r.submitted_at.is_none() => "invited".to_string(),
        _ => return Err(VendorError::WrongStatus),
    };
    let link = add_link(pool, id, true).await?;
    query(
        "UPDATE vendor_checks SET status = $2, expires_at = NOW() + make_interval(days => $3),
                asked_at = NOW(), reminder_sent_at = NULL, updated_at = NOW()
         WHERE id = $1",
    )
    .bind(id)
    .bind(status)
    .bind(LINK_DAYS as i32)
    .execute(pool)
    .await?;
    Ok(link)
}

/// Emails the vendor again now (with a new link; the old one keeps
/// working).
pub async fn remind_vendor(
    pool: &Pool<Postgres>,
    viewer: Uuid,
    id: Uuid,
) -> Result<(), VendorError> {
    let r = load_for_team(pool, viewer, id).await?;
    if !matches!(r.status.as_str(), "invited" | "opened" | "more_info") || r.expires_at < Utc::now()
    {
        return Err(VendorError::WrongStatus);
    }
    send_reminder(pool, &r, Some(viewer)).await?;
    Ok(())
}

async fn send_reminder(
    pool: &Pool<Postgres>,
    r: &CheckRow,
    by: Option<Uuid>,
) -> Result<(), VendorError> {
    let link = add_link(pool, r.id, false).await?;
    let to = answers_from(&r.answers)
        .get("email")
        .cloned()
        .filter(|_| r.status == "more_info")
        .unwrap_or_else(|| r.vendor_email.clone());
    let text = if r.status == "more_info" {
        ask_text(
            &r.language,
            &r.buyer_company,
            &r.vendor_name,
            &link,
            &fields_from(&r.ask_fields),
            r.ask_note.as_deref(),
            true,
        )
    } else {
        invite_text(&r.language, &r.buyer_company, &r.vendor_name, &link, true)
    };
    let reply_to: Option<String> = query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(by.or(r.invited_by).unwrap_or(r.team_owner_id))
        .fetch_optional(pool)
        .await?;
    query("UPDATE vendor_checks SET reminder_sent_at = NOW(), updated_at = NOW() WHERE id = $1")
        .bind(r.id)
        .execute(pool)
        .await?;
    if !may_email_vendor(pool, &to).await {
        return Err(VendorError::EmailLimit);
    }
    if let Err(e) = send_vendor_email(&to, &text, reply_to.as_deref()).await {
        eprintln!("Safely: could not send a vendor reminder email: {e:?}");
    }
    Ok(())
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DecisionRequest {
    /// "approve" or "reject".
    pub decision: String,
    pub note: Option<String>,
}

/// Approve or Reject (can be changed later).
pub async fn decide(
    pool: &Pool<Postgres>,
    viewer: Uuid,
    id: Uuid,
    req: DecisionRequest,
) -> Result<(), VendorError> {
    let r = load_for_team(pool, viewer, id).await?;
    let status = match req.decision.as_str() {
        "approve" => "approved",
        "reject" => "rejected",
        _ => return Err(VendorError::BadInput("bad_decision")),
    };
    if !matches!(r.status.as_str(), "ready" | "approved" | "rejected") {
        return Err(VendorError::WrongStatus);
    }
    query(
        "UPDATE vendor_checks SET status = $2, decision_note = $3, decided_by = $4,
                decided_at = NOW(), updated_at = NOW()
         WHERE id = $1",
    )
    .bind(id)
    .bind(status)
    .bind(optional(&req.note))
    .bind(viewer)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AskRequest {
    /// The answers the vendor may change (FIELDS).
    pub fields: Vec<String>,
    /// A short note shown on the form and in the email.
    pub note: Option<String>,
    /// "more_info" (default) or "bank_change".
    pub reason: Option<String>,
}

/// Ask for more info, or for new bank details: a link that only opens
/// the chosen answers. The vendor's changes are checked again, without
/// a new scan. Returns the link.
pub async fn ask_vendor(
    pool: &Pool<Postgres>,
    viewer: Uuid,
    id: Uuid,
    req: AskRequest,
) -> Result<String, VendorError> {
    let r = load_for_team(pool, viewer, id).await?;
    if !matches!(
        r.status.as_str(),
        "ready" | "approved" | "rejected" | "more_info"
    ) || r.result.is_none()
    {
        return Err(VendorError::WrongStatus);
    }
    let bank = req.reason.as_deref() == Some("bank_change");
    let mut fields: Vec<String> = if bank {
        vec![
            "bank_holder".to_string(),
            "bank_account".to_string(),
            "bank_code".to_string(),
        ]
    } else {
        req.fields
            .iter()
            .filter(|f| FIELDS.contains(&f.as_str()))
            .cloned()
            .collect()
    };
    fields.dedup();
    if fields.is_empty() {
        return Err(VendorError::BadInput("no_fields"));
    }
    let note = optional(&req.note);
    let link = add_link(pool, id, true).await?;
    // A new email must be confirmed again.
    let reset_email = fields.iter().any(|f| f == "email");
    query(
        "UPDATE vendor_checks SET status = 'more_info', ask_fields = $2, ask_note = $3,
                expires_at = NOW() + make_interval(days => $4), asked_at = NOW(),
                reminder_sent_at = NULL, updated_at = NOW(),
                email_confirmed = CASE WHEN $5 THEN NULL ELSE email_confirmed END
         WHERE id = $1",
    )
    .bind(id)
    .bind(json!(fields))
    .bind(&note)
    .bind(LINK_DAYS as i32)
    .bind(reset_email)
    .execute(pool)
    .await?;

    let to = answers_from(&r.answers)
        .get("email")
        .cloned()
        .unwrap_or_else(|| r.vendor_email.clone());
    let text = if bank {
        bank_text(
            &r.language,
            &r.buyer_company,
            &r.vendor_name,
            &link,
            note.as_deref(),
        )
    } else {
        ask_text(
            &r.language,
            &r.buyer_company,
            &r.vendor_name,
            &link,
            &fields,
            note.as_deref(),
            false,
        )
    };
    let reply_to: Option<String> = query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(viewer)
        .fetch_optional(pool)
        .await?;
    if r.send_email
        && may_email_vendor(pool, &to).await
        && let Err(e) = send_vendor_email(&to, &text, reply_to.as_deref()).await
    {
        eprintln!("Safely: could not send a vendor update email: {e:?}");
    }
    Ok(link)
}

/// "Run the check now" (a check waiting for scans, or one that
/// stopped): Ok when it may run; the handler then runs run_check in the
/// background.
pub async fn can_retry(pool: &Pool<Postgres>, viewer: Uuid, id: Uuid) -> Result<(), VendorError> {
    let r = load_for_team(pool, viewer, id).await?;
    if !matches!(r.status.as_str(), "no_scans" | "checking") {
        return Err(VendorError::WrongStatus);
    }
    Ok(())
}

/// The vendor as a seller (platform "vendor", one per vendor check), for
/// Watched suppliers and scam reports. Its facts are saved so a watch
/// can check its outside records again.
async fn vendor_seller(pool: &Pool<Postgres>, r: &CheckRow) -> Result<Uuid, VendorError> {
    let answers = answers_from(&r.answers);
    let name = answers
        .get("company_name")
        .cloned()
        .unwrap_or(r.vendor_name.clone());
    let seller_id: Uuid = query_scalar(
        "INSERT INTO sellers (platform, platform_id, name, location, profile_url)
         VALUES ('vendor', $1, $2, $3, $4)
         ON CONFLICT (platform, platform_id) DO UPDATE
            SET name = EXCLUDED.name, location = EXCLUDED.location,
                profile_url = EXCLUDED.profile_url, updated_at = NOW()
         RETURNING id",
    )
    .bind(r.id.to_string())
    .bind(name.chars().take(200).collect::<String>())
    .bind(answers.get("country"))
    .bind(answers.get("website"))
    .fetch_one(pool)
    .await?;
    remember_seller_facts(pool, seller_id, &facts_of(&answers)).await;
    query("UPDATE vendor_checks SET seller_id = $2 WHERE id = $1")
        .bind(r.id)
        .bind(seller_id)
        .execute(pool)
        .await?;
    Ok(seller_id)
}

#[derive(Debug, Deserialize)]
pub struct ReportRequest {
    pub report_type: ReportTypes,
    pub description: Option<String>,
}

/// Reports the vendor as a scam, from the dashboard (marketplace
/// sellers are reported from the extension). The report is a normal
/// scam report: it shows in My Reports (platform "vendor"), counts in
/// "Scam reports" for any vendor or seller with this name later, and
/// one person can report a vendor once.
pub async fn report_vendor(
    pool: &Pool<Postgres>,
    viewer: Uuid,
    id: Uuid,
    req: ReportRequest,
) -> Result<(), VendorError> {
    let r = load_for_team(pool, viewer, id).await?;
    if r.submitted_at.is_none() {
        return Err(VendorError::WrongStatus);
    }
    let seller_id = vendor_seller(pool, &r).await?;
    if already_reported(pool, viewer, seller_id).await? {
        return Err(VendorError::AlreadyReported);
    }
    query(
        "INSERT INTO fraud_reports (seller_id, user_id, platform, platform_id, report_type, description, listing_url)
         VALUES ($1, $2, 'vendor', $3, $4, $5, $6)",
    )
    .bind(seller_id)
    .bind(viewer)
    .bind(r.id.to_string())
    .bind(req.report_type)
    .bind(optional(&req.description))
    .bind(format!("/vendors/{}", r.id))
    .execute(pool)
    .await?;
    Ok(())
}

/// Adds the vendor to Watched suppliers (watch.rs): Safely checks its
/// outside records, scam reports and website again later.
pub async fn watch_vendor(
    pool: &Pool<Postgres>,
    viewer: Uuid,
    id: Uuid,
) -> Result<(), VendorError> {
    let r = load_for_team(pool, viewer, id).await?;
    if r.result.is_none() {
        return Err(VendorError::WrongStatus);
    }
    let seller_id = vendor_seller(pool, &r).await?;

    let already: bool = query_scalar(
        "SELECT EXISTS (SELECT 1 FROM watched_suppliers WHERE team_owner_id = $1 AND seller_id = $2)",
    )
    .bind(r.team_owner_id)
    .bind(seller_id)
    .fetch_one(pool)
    .await?;
    if !already {
        if let Some(limit) = watch_limit(pool, r.team_owner_id).await {
            let used: i64 =
                query_scalar("SELECT COUNT(*) FROM watched_suppliers WHERE team_owner_id = $1")
                    .bind(r.team_owner_id)
                    .fetch_one(pool)
                    .await?;
            if used >= limit {
                return Err(VendorError::WatchLimit(limit));
            }
        }
        query(
            "INSERT INTO watched_suppliers (team_owner_id, seller_id, listing_url, added_by)
             VALUES ($1, $2, NULL, $3)
             ON CONFLICT (team_owner_id, seller_id) DO NOTHING",
        )
        .bind(r.team_owner_id)
        .bind(seller_id)
        .bind(viewer)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// The Watched suppliers limit (the same as watch.rs): Team 25,
/// Enterprise and test accounts none.
async fn watch_limit(pool: &Pool<Postgres>, owner: Uuid) -> Option<i64> {
    if is_unlimited_user(pool, owner).await {
        return None;
    }
    let plan: Option<String> = query_scalar(
        "SELECT plan_name FROM subscriptions WHERE user_id = $1 AND status = 'active'
         ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(owner)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    match plan {
        Some(p) if p.to_lowercase().contains("enterprise") => None,
        _ => Some(TEAM_WATCH_LIMIT),
    }
}

fn facts_of(answers: &BTreeMap<String, String>) -> SellerFacts {
    SellerFacts {
        company_name: answers.get("company_name").cloned(),
        country: answers.get("country").cloned(),
        website: answers.get("website").cloned(),
        company_numbers: answers.get("company_number").cloned().into_iter().collect(),
        about: None,
    }
}

// ───────────────────────── the vendor's form ─────────────────────────

/// The check a form link belongs to, while the link still works.
async fn check_for_link(pool: &Pool<Postgres>, token: &str) -> Result<CheckRow, VendorError> {
    let id: Option<Uuid> = query_scalar(
        "SELECT vendor_check_id FROM vendor_check_links WHERE token_hash = $1 AND revoked_at IS NULL",
    )
    .bind(hash_token(token))
    .fetch_optional(pool)
    .await?;
    let row = load(pool, id.ok_or(VendorError::LinkClosed)?).await?;
    if row.status == "canceled" || row.status == "expired" {
        return Err(VendorError::LinkClosed);
    }
    let open = matches!(row.status.as_str(), "invited" | "opened" | "more_info");
    if open && row.expires_at < Utc::now() {
        return Err(VendorError::LinkClosed);
    }
    Ok(row)
}

/// True when the vendor can still fill in the form.
fn can_edit(row: &CheckRow) -> bool {
    matches!(row.status.as_str(), "invited" | "opened" | "more_info")
}

/// The answers the vendor may change now: all of them, or only the ones
/// the buyer asked for.
fn editable(row: &CheckRow) -> Vec<String> {
    let asked = fields_from(&row.ask_fields);
    if row.status == "more_info" && !asked.is_empty() {
        asked
    } else {
        FIELDS.iter().map(|f| f.to_string()).collect()
    }
}

/// What the form page needs. Never the result, never the buyer's notes
/// about the vendor.
pub async fn open_form(pool: &Pool<Postgres>, token: &str) -> Result<Value, VendorError> {
    let row = check_for_link(pool, token).await?;
    if row.status == "invited" {
        query("UPDATE vendor_checks SET status = 'opened', opened_at = NOW(), updated_at = NOW() WHERE id = $1 AND status = 'invited'")
            .bind(row.id)
            .execute(pool)
            .await?;
    }
    let done = !can_edit(&row);
    let answers = answers_from(&row.answers);
    // Which files are uploaded (names only; the vendor can replace them).
    let uploaded: Vec<Value> = if done {
        Vec::new()
    } else {
        query_as::<_, (String, String)>(
            "SELECT kind, file_name FROM vendor_documents WHERE vendor_check_id = $1",
        )
        .bind(row.id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(kind, name)| json!({ "kind": kind, "file_name": name }))
        .collect()
    };
    let known: Known = serde_json::from_value(row.known.clone()).unwrap_or_default();
    Ok(json!({
        "buyer_company": row.buyer_company,
        "vendor_name": row.vendor_name,
        "language": row.language,
        "done": done,
        "fields": FIELDS,
        "optional": OPTIONAL_FIELDS,
        "editable": if done { Vec::new() } else { editable(&row) },
        "ask_note": if row.status == "more_info" { row.ask_note.clone() } else { None },
        "answers": if done { BTreeMap::new() } else { answers.clone() },
        // The invite's email, to fill in the first time.
        "suggested_email": known.email,
        "email_confirmed": if done { None } else { row.email_confirmed.clone() },
        "expires_on": row.expires_at.format("%Y-%m-%d").to_string(),
        "documents": uploaded,
    }))
}

/// Merges the vendor's answers into the saved ones (only the answers
/// the vendor may change). A changed email must be confirmed again.
fn merge_answers(row: &CheckRow, new: &Value) -> (BTreeMap<String, String>, bool) {
    let mut answers = answers_from(&row.answers);
    let allowed = editable(row);
    let incoming = new.as_object().cloned().unwrap_or_else(Map::new);
    for field in &allowed {
        if let Some(value) = incoming.get(field) {
            let text = value.as_str().map(clean_answer).unwrap_or_default();
            if text.is_empty() {
                answers.remove(field);
            } else {
                answers.insert(field.clone(), text);
            }
        }
    }
    let email_ok = match (answers.get("email"), row.email_confirmed.as_deref()) {
        (Some(e), Some(c)) => e.eq_ignore_ascii_case(c),
        _ => false,
    };
    (answers, email_ok)
}

/// Saves the vendor's progress (the form saves as the vendor types).
pub async fn save_draft(
    pool: &Pool<Postgres>,
    token: &str,
    answers: &Value,
) -> Result<(), VendorError> {
    let row = check_for_link(pool, token).await?;
    if !can_edit(&row) {
        return Err(VendorError::WrongStatus);
    }
    let (merged, _) = merge_answers(&row, answers);
    query("UPDATE vendor_checks SET answers = $2, updated_at = NOW() WHERE id = $1")
        .bind(row.id)
        .bind(json!(merged))
        .execute(pool)
        .await?;
    Ok(())
}

/// Sends a 6-digit code to the email the vendor typed.
pub async fn send_email_code(
    pool: &Pool<Postgres>,
    token: &str,
    email: &str,
) -> Result<(), VendorError> {
    let row = check_for_link(pool, token).await?;
    if !can_edit(&row) || !editable(&row).iter().any(|f| f == "email") {
        return Err(VendorError::WrongStatus);
    }
    let email = clean_invite_email(email).ok_or(VendorError::BadInput("bad_email"))?;
    if let Some(sent) = row.email_code_sent_at
        && (Utc::now() - sent).num_seconds() < CODE_WAIT_SECONDS
    {
        return Err(VendorError::CodeWait);
    }
    let code = new_code();
    query(
        "UPDATE vendor_checks SET email_code_hash = $2, email_code_for = $3,
                email_code_sent_at = NOW(), email_code_tries = 0, updated_at = NOW()
         WHERE id = $1",
    )
    .bind(row.id)
    .bind(hash_code(row.id, &code))
    .bind(&email)
    .execute(pool)
    .await?;
    let text = code_text(&row.language, &row.buyer_company, &code);
    if let Err(e) = send_vendor_email(&email, &text, None).await {
        eprintln!("Safely: could not send a vendor email code: {e:?}");
        return Err(VendorError::Database("email_not_sent".to_string()));
    }
    Ok(())
}

/// Checks the code the vendor typed. Returns the confirmed email.
pub async fn confirm_email_code(
    pool: &Pool<Postgres>,
    token: &str,
    code: &str,
) -> Result<String, VendorError> {
    let row = check_for_link(pool, token).await?;
    if !can_edit(&row) {
        return Err(VendorError::WrongStatus);
    }
    let (Some(hash), Some(email), Some(sent)) = (
        row.email_code_hash.as_deref(),
        row.email_code_for.as_deref(),
        row.email_code_sent_at,
    ) else {
        return Err(VendorError::CodeExpired);
    };
    if (Utc::now() - sent).num_minutes() >= CODE_MINUTES {
        return Err(VendorError::CodeExpired);
    }
    if row.email_code_tries >= CODE_TRIES {
        return Err(VendorError::CodeTooManyTries);
    }
    let digits: String = code.chars().filter(char::is_ascii_digit).collect();
    if hash_code(row.id, &digits) != hash {
        query("UPDATE vendor_checks SET email_code_tries = email_code_tries + 1 WHERE id = $1")
            .bind(row.id)
            .execute(pool)
            .await?;
        return Err(VendorError::CodeWrong);
    }
    let mut answers = answers_from(&row.answers);
    answers.insert("email".to_string(), email.to_string());
    query(
        "UPDATE vendor_checks SET email_confirmed = $2, answers = $3, email_code_hash = NULL,
                email_code_tries = 0, updated_at = NOW()
         WHERE id = $1",
    )
    .bind(row.id)
    .bind(email)
    .bind(json!(answers))
    .execute(pool)
    .await?;
    Ok(email.to_string())
}

/// Sends the form: every needed answer is there and the email is
/// confirmed. The check then runs in the background.
pub async fn submit_form(
    pool: &Pool<Postgres>,
    token: &str,
    answers: &Value,
) -> Result<Uuid, VendorError> {
    let row = check_for_link(pool, token).await?;
    if !can_edit(&row) {
        return Err(VendorError::WrongStatus);
    }
    let (merged, email_ok) = merge_answers(&row, answers);
    if !missing_fields(&merged).is_empty() {
        return Err(VendorError::BadInput("answers_missing"));
    }
    if !email_ok {
        return Err(VendorError::EmailNotConfirmed);
    }
    // A vendor's changes: the old answers are kept for the buyer.
    let mut previous = row.previous.as_array().cloned().unwrap_or_default();
    if row.status == "more_info" && row.submitted_at.is_some() {
        previous.push(json!({
            "answers": answers_from(&row.answers),
            "sent_on": row.submitted_at.map(|d| d.format("%Y-%m-%d").to_string()),
            "asked": fields_from(&row.ask_fields),
            "risk_level": row.risk_level,
        }));
    }
    let done = query(
        "UPDATE vendor_checks SET answers = $2, previous = $3, status = 'checking',
                submitted_at = NOW(), ask_fields = '[]', ask_note = NULL, updated_at = NOW()
         WHERE id = $1 AND status IN ('invited', 'opened', 'more_info')",
    )
    .bind(row.id)
    .bind(json!(merged))
    .bind(Value::Array(previous))
    .execute(pool)
    .await?;
    if done.rows_affected() == 0 {
        return Err(VendorError::WrongStatus);
    }
    Ok(row.id)
}

// ───────────────────────── the check ─────────────────────────

/// Scam reports from Safely users on sellers with the same name.
async fn reports_for_name(pool: &Pool<Postgres>, name: &str) -> Option<i64> {
    let compared = sanctions::compared_name(name)?;
    let longest = compared
        .split_whitespace()
        .max_by_key(|w| w.len())?
        .to_string();
    let rows: Vec<(String, i64)> = query_as(
        "SELECT s.name, COUNT(f.id) FROM sellers s
         JOIN fraud_reports f ON f.seller_id = s.id
         WHERE s.name IS NOT NULL AND lower(s.name) LIKE $1
         GROUP BY s.id, s.name LIMIT 200",
    )
    .bind(format!("%{longest}%"))
    .fetch_all(pool)
    .await
    .ok()?;
    Some(
        rows.into_iter()
            .filter(|(n, _)| names_match(n, name))
            .map(|(_, c)| c)
            .sum(),
    )
}

async fn domain_age_card(key: &str, label: &str, domain: &str) -> VendorCard {
    match lookup(domain).await {
        Ok(age) => age_card(
            key,
            label,
            &age.domain,
            Some((age.verdict, age.registered_on.as_deref(), age.age_days)),
        ),
        Err(_) => age_card(key, label, domain, None),
    }
}

/// Everything one check found.
#[derive(Debug, Default)]
pub struct VendorResult {
    pub cards: Vec<VendorCard>,
    /// Outside-records cards (sanctions, registers, fairs...).
    pub records: Vec<Signal>,
    /// Pages where the company was found online.
    pub online: Vec<OnlineLink>,
    /// Each site searched, found or not.
    pub presence: Vec<Presence>,
    /// The web search's rows as a scan saves them (the dashboard draws
    /// "Online presence" and "Scam warnings online" from these).
    pub social_candidates: Vec<PlatformCheckResult>,
}

/// Every check for these answers. `check_id`: this vendor check (its
/// documents, and other vendors with the same bank account).
pub async fn build_result(
    pool: &Pool<Postgres>,
    check_id: Uuid,
    known: &Known,
    answers: &BTreeMap<String, String>,
    email_confirmed: Option<&str>,
) -> VendorResult {
    let mut cards = answer_cards(known, answers, email_confirmed);
    let website = answers.get("website").and_then(|w| normalize_domain(w));
    let email_dom = answers
        .get("email")
        .and_then(|e| email_domain(e))
        .filter(|d| !is_free_email(d));
    let name = answers.get("company_name").cloned().unwrap_or_default();
    let facts = facts_of(answers);

    let website_age = async {
        match &website {
            Some(site) => Some(domain_age_card("website_age", "Website age", site).await),
            None => None,
        }
    };
    let email_age = async {
        match (&email_dom, &website) {
            (Some(dom), Some(site)) if dom == site || dom.ends_with(&format!(".{site}")) => None,
            (Some(dom), _) => {
                Some(domain_age_card("email_domain_age", "Email domain age", dom).await)
            }
            _ => None,
        }
    };
    // The same web search a scan runs (osint.rs, Serper): the company
    // on Facebook, LinkedIn, Instagram, TikTok, Reddit, Trustpilot and
    // review sites, and its name next to scam words anywhere on the web.
    // Off unless SERPER_ENABLED=true, like scans.
    let web = async {
        if name.trim().is_empty() {
            WebSearch::NoName
        } else if !serper_enabled() {
            WebSearch::Off
        } else {
            match build_social_presence_matrix(
                Some(&name),
                None,
                answers.get("country").map(String::as_str),
                answers.get("phone").map(String::as_str),
                "vendor",
            )
            .await
            {
                Ok((signal, results)) => WebSearch::Done(signal, results),
                Err(e) => {
                    eprintln!("Safely: vendor web search failed: {e}");
                    WebSearch::Failed
                }
            }
        }
    };
    let phone = async {
        match answers.get("phone") {
            Some(p) => phone_search(p, &name, answers.get("country").map(String::as_str)).await,
            None => Vec::new(),
        }
    };
    let country = answers.get("country").map(String::as_str);
    let address = async {
        match answers.get("address") {
            Some(a) => address_cards(a, country, map_search(a).await),
            None => Vec::new(),
        }
    };
    let site_content = async {
        match &website {
            Some(site) => Some(website_card(
                site,
                &name,
                fetch_home_page(site).await.as_deref(),
            )),
            None => None,
        }
    };
    let holder_sanctions = async {
        match answers.get("bank_holder") {
            Some(h) if !names_match(h, &name) => {
                sanctions::sanctions_signal(Some(
                    crate::services::sources::databases::name_for_lists(h),
                ))
                .await
            }
            _ => None,
        }
    };
    let documents = document_check(pool, check_id, answers, &name);
    let same_email = same_email_cards(pool, check_id, answers);
    let same_account = same_account_cards(pool, check_id, answers);
    let (
        website_age,
        email_age,
        web,
        phone,
        records,
        reports,
        address,
        site_content,
        holder,
        docs,
        same,
        same_mail,
    ) = tokio::join!(
        website_age,
        email_age,
        web,
        phone,
        check_outside_records(&facts),
        reports_for_name(pool, &name),
        address,
        site_content,
        holder_sanctions,
        documents,
        same_account,
        same_email,
    );

    if let Some(dom) = &email_dom {
        match &website {
            Some(site) => cards.extend(email_vs_website_card(dom, site)),
            None => {
                if let Some(d) = parse_domain(dom) {
                    let answer = check_brands(&d);
                    if answer.verdict == LookalikeVerdict::Lookalike {
                        cards.push(card(
                            "email_website",
                            "Email domain",
                            "Copies a brand",
                            "bad",
                            format!(
                                "{} looks like a copy of {}.",
                                d.site(),
                                answer.brand.unwrap_or_else(|| "a famous brand".to_string())
                            ),
                        ));
                    }
                }
            }
        }
    }
    cards.extend(website_age);
    cards.extend(email_age);
    cards.push(match reports {
        Some(0) => card(
            "scam_reports",
            "Scam reports",
            "None",
            "good",
            "No Safely user has reported a company with this name.".to_string(),
        ),
        Some(n) => card(
            "scam_reports",
            "Scam reports",
            &format!("{n} found"),
            "bad",
            format!("Safely users reported a company with this name {n} time(s)."),
        ),
        None => card(
            "scam_reports",
            "Scam reports",
            "Not checked",
            "info",
            "The name is too short to compare safely.".to_string(),
        ),
    });
    let (online, presence, social_candidates) = web_cards(&mut cards, web, &name, country);
    cards.extend(phone);
    if let Some(p) = answers.get("phone") {
        cards.extend(phone_country_card(p, country));
    }
    cards.extend(address);
    cards.extend(site_content);
    cards.extend(bank_cards(
        answers.get("bank_account").map(String::as_str),
        answers.get("bank_code").map(String::as_str),
        country,
    ));
    cards.extend(same);
    cards.extend(same_mail);
    if let Some(h) = holder {
        cards.push(if h.signal_type == "bad" {
            card(
                "holder_sanctions",
                "Bank account name on sanctions lists",
                "Listed",
                "bad",
                h.sub,
            )
        } else {
            card(
                "holder_sanctions",
                "Bank account name on sanctions lists",
                "Not on the lists",
                "good",
                "The name on the bank account is not on the sanctions lists.".to_string(),
            )
        });
    }
    cards.extend(docs);
    let records: Vec<Signal> = records
        .into_iter()
        .filter(|c| c.category == OUTSIDE_RECORDS && database_card_enabled(&c.label))
        .collect();
    // The number is checked by the outside records only for China,
    // Brazil and France. Anywhere else: at least say so, and flag a
    // number that looks made up (123456789, 000000).
    let number_checked = records
        .iter()
        .any(|r| r.label == "Company number" || r.label == "Official register");
    if !number_checked && let Some(number) = answers.get("company_number") {
        cards.push(company_number_card(number, country));
    }
    VendorResult {
        cards,
        records,
        online,
        presence,
        social_candidates,
    }
}

/// Another vendor gave the same bank account number. In the team: its
/// name; in other teams: only how many (their vendors stay private).
/// A different company with the same account is a strong warning.
async fn same_account_cards(
    pool: &Pool<Postgres>,
    check_id: Uuid,
    answers: &BTreeMap<String, String>,
) -> Vec<VendorCard> {
    let Some(account) = answers
        .get("bank_account")
        .map(|a| clean_account(a))
        .filter(|a| a.len() >= 6)
    else {
        return Vec::new();
    };
    let name = answers.get("company_name").cloned().unwrap_or_default();
    let rows: Vec<(Uuid, Option<String>, String)> = query_as(
        "SELECT o.team_owner_id, o.answers->>'company_name', o.vendor_name
         FROM vendor_checks o, vendor_checks me
         WHERE me.id = $1 AND o.id <> $1
           AND upper(regexp_replace(o.answers->>'bank_account', '[^A-Za-z0-9]', '', 'g')) = $2
         LIMIT 50",
    )
    .bind(check_id)
    .bind(&account)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let owner: Option<Uuid> = query_scalar("SELECT team_owner_id FROM vendor_checks WHERE id = $1")
        .bind(check_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    let others: Vec<(bool, String)> = rows
        .into_iter()
        .map(|(team, company, invited)| (Some(team) == owner, company.unwrap_or(invited)))
        .filter(|(_, other)| !names_match(other, &name))
        .collect();
    if others.is_empty() {
        return vec![card(
            "same_account",
            "Bank account used by another vendor",
            "No",
            "good",
            "No other vendor on Safely gave this bank account.".to_string(),
        )];
    }
    let mine: Vec<&str> = others
        .iter()
        .filter(|(m, _)| *m)
        .map(|(_, n)| n.as_str())
        .collect();
    let elsewhere = others.len() - mine.len();
    let mut parts = Vec::new();
    if !mine.is_empty() {
        parts.push(format!("your vendor(s) {}", mine.join(", ")));
    }
    if elsewhere > 0 {
        parts.push(format!("{elsewhere} vendor(s) of other Safely customers"));
    }
    vec![card(
        "same_account",
        "Bank account used by another vendor",
        "Yes",
        "bad",
        format!(
            "The same bank account was given by {}, under another company name. One account for different companies is a common sign of fraud.",
            parts.join(" and ")
        ),
    )]
}

/// An uploaded file: id, kind, type, bytes, what was read from it.
type SavedDocument = (Uuid, String, String, Vec<u8>, Option<Value>);

/// The same email address given for another company name in other
/// vendor checks. Your team: the names; other Safely customers: only
/// how many (their vendors stay private). A real supplier invited by
/// many buyers under its own name is fine.
async fn same_email_cards(
    pool: &Pool<Postgres>,
    check_id: Uuid,
    answers: &BTreeMap<String, String>,
) -> Vec<VendorCard> {
    let Some(email) = answers.get("email").map(|e| e.trim().to_lowercase()) else {
        return Vec::new();
    };
    let name = answers.get("company_name").cloned().unwrap_or_default();
    let rows: Vec<(bool, Option<String>, String)> = query_as(
        "SELECT o.team_owner_id = me.team_owner_id, o.answers->>'company_name', o.vendor_name
         FROM vendor_checks o, vendor_checks me
         WHERE me.id = $1 AND o.id <> $1
           AND (lower(o.answers->>'email') = $2 OR lower(o.vendor_email) = $2)
         LIMIT 50",
    )
    .bind(check_id)
    .bind(&email)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let others: Vec<(bool, String)> = rows
        .into_iter()
        .map(|(mine, company, invited)| (mine, company.unwrap_or(invited)))
        .filter(|(_, other)| !names_match(other, &name))
        .collect();
    if others.is_empty() {
        return vec![card(
            "same_email",
            "Email used for another company",
            "No",
            "good",
            "No other vendor check on Safely used this email for another company name.".to_string(),
        )];
    }
    let mut mine: Vec<&str> = others
        .iter()
        .filter(|(m, _)| *m)
        .map(|(_, n)| n.as_str())
        .collect();
    mine.dedup();
    let elsewhere = others.iter().filter(|(m, _)| !*m).count();
    let mut parts = Vec::new();
    if !mine.is_empty() {
        parts.push(format!("your vendor(s) {}", mine.join(", ")));
    }
    if elsewhere > 0 {
        parts.push(format!(
            "{elsewhere} vendor check(s) of other Safely customers"
        ));
    }
    vec![card(
        "same_email",
        "Email used for another company",
        "Yes",
        "bad",
        format!(
            "{email} was also used by {}, under another company name. One email for different companies is a common sign of fraud.",
            parts.join(" and ")
        ),
    )]
}

/// Reads the vendor's uploaded files (once each) and compares them with
/// the answers. No file: no card.
async fn document_check(
    pool: &Pool<Postgres>,
    check_id: Uuid,
    answers: &BTreeMap<String, String>,
    company: &str,
) -> Vec<VendorCard> {
    let docs: Vec<SavedDocument> = query_as(
        "SELECT id, kind, content_type, data, reading FROM vendor_documents
         WHERE vendor_check_id = $1 ORDER BY kind",
    )
    .bind(check_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let mut cards = Vec::new();
    for (doc_id, kind, content_type, data, saved) in docs {
        let reading: Option<DocumentReading> =
            match saved.and_then(|v| serde_json::from_value(v).ok()) {
                Some(r) => Some(r),
                None => {
                    let read = read_document(&data, &content_type).await;
                    if let Some(r) = &read {
                        let _ = query("UPDATE vendor_documents SET reading = $2 WHERE id = $1")
                            .bind(doc_id)
                            .bind(serde_json::to_value(r).unwrap_or(Value::Null))
                            .execute(pool)
                            .await;
                    }
                    read
                }
            };
        cards.extend(document_cards(
            &kind,
            reading.as_ref(),
            company,
            answers.get("company_number").map(String::as_str),
            answers.get("bank_holder").map(String::as_str),
            answers.get("bank_account").map(String::as_str),
        ));
    }
    cards
}

/// Saves a file the vendor uploaded on the form (replaces the old one
/// of the same kind). A file changed later is read again.
pub async fn upload_document(
    pool: &Pool<Postgres>,
    token: &str,
    kind: &str,
    file_name: &str,
    bytes: &[u8],
) -> Result<(), VendorError> {
    let row = check_for_link(pool, token).await?;
    if !can_edit(&row) {
        return Err(VendorError::WrongStatus);
    }
    if !DOCUMENT_KINDS.contains(&kind) {
        return Err(VendorError::BadInput("bad_kind"));
    }
    if bytes.is_empty() || bytes.len() > DOCUMENT_MAX_BYTES {
        return Err(VendorError::BadFile);
    }
    let content_type = document_type(bytes).ok_or(VendorError::BadFile)?;
    let name: String = file_name
        .chars()
        .filter(|c| !c.is_control() && *c != '/' && *c != '\\')
        .take(120)
        .collect();
    query(
        "INSERT INTO vendor_documents (vendor_check_id, kind, file_name, content_type, size_bytes, data)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (vendor_check_id, kind) DO UPDATE
            SET file_name = EXCLUDED.file_name, content_type = EXCLUDED.content_type,
                size_bytes = EXCLUDED.size_bytes, data = EXCLUDED.data, reading = NULL,
                created_at = NOW()",
    )
    .bind(row.id)
    .bind(kind)
    .bind(if name.trim().is_empty() { "document".to_string() } else { name })
    .bind(content_type)
    .bind(bytes.len() as i32)
    .bind(bytes)
    .execute(pool)
    .await?;
    Ok(())
}

/// One uploaded file, for the buyer's team: (file name, type, bytes).
pub async fn download_document(
    pool: &Pool<Postgres>,
    viewer: Uuid,
    id: Uuid,
    doc_id: Uuid,
) -> Result<(String, String, Vec<u8>), VendorError> {
    load_for_team(pool, viewer, id).await?;
    query_as("SELECT file_name, content_type, data FROM vendor_documents WHERE id = $1 AND vendor_check_id = $2")
        .bind(doc_id)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(VendorError::NotFound)
}

/// What the web search gave.
enum WebSearch {
    /// No company name to search with.
    NoName,
    /// SERPER_ENABLED is not "true".
    Off,
    /// Most searches failed (no key, no credits, Serper down).
    Failed,
    Done(Signal, Vec<PlatformCheckResult>),
}

/// One page where the company was found online.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OnlineLink {
    /// "Facebook", "LinkedIn", "Trustpilot"...
    pub platform: String,
    pub title: String,
    pub url: String,
}

/// The "Online presence" and "Scam warnings online" cards, and the
/// pages found (for the buyer to open). A search that did not run says
/// "Not checked" with the reason - never "None found".
fn web_cards(
    cards: &mut Vec<VendorCard>,
    web: WebSearch,
    company: &str,
    country: Option<&str>,
) -> (Vec<OnlineLink>, Vec<Presence>, Vec<PlatformCheckResult>) {
    let not_checked = |cards: &mut Vec<VendorCard>, why: &str| {
        for (key, label) in [
            ("social_presence", "Online presence"),
            ("web_warnings", "Scam warnings online"),
        ] {
            cards.push(card(key, label, "Not checked", "info", why.to_string()));
        }
    };
    let (signal, results) = match web {
        WebSearch::NoName => {
            not_checked(cards, "No company name to search with.");
            return (Vec::new(), Vec::new(), Vec::new());
        }
        WebSearch::Off => {
            not_checked(
                cards,
                "The web search is switched off on this server, so this was not checked.",
            );
            return (Vec::new(), Vec::new(), Vec::new());
        }
        WebSearch::Failed => {
            not_checked(
                cards,
                "The web search did not answer. Safely did not count this as clear.",
            );
            return (Vec::new(), Vec::new(), Vec::new());
        }
        WebSearch::Done(signal, results) => (signal, results),
    };

    let mut scam_pages: Vec<OnlineLink> = Vec::new();
    let mut online: Vec<OnlineLink> = Vec::new();
    for r in &results {
        let scam = r.scam_mention || is_web_scam_row(&r.platform);
        for c in &r.candidates {
            let list = if scam { &mut scam_pages } else { &mut online };
            if !list.iter().any(|l| l.url == c.url) {
                list.push(OnlineLink {
                    platform: r.platform.clone(),
                    title: c.title.clone(),
                    url: c.url.clone(),
                });
            }
        }
    }
    online.truncate(12);

    // Every site that was searched, found or not (like the extension's
    // "Online presence" list). Scam-word rows and contact rows are left
    // out: they are not the company's own pages.
    let mut presence: Vec<Presence> = Vec::new();
    for r in &results {
        if r.scam_mention || is_web_scam_row(&r.platform) || r.platform.starts_with("Contact") {
            continue;
        }
        let found = !r.candidates.is_empty();
        match presence.iter_mut().find(|p| p.platform == r.platform) {
            Some(p) => p.found |= found,
            None => presence.push(Presence {
                platform: r.platform.clone(),
                found,
            }),
        }
    }

    // Scam mentions are counted once, on their own card.
    let (value, kind) = if signal.value == SCAM_MENTIONS_FOUND {
        if online.is_empty() {
            ("No presence found".to_string(), "caution".to_string())
        } else {
            ("Found online".to_string(), "good".to_string())
        }
    } else {
        (signal.value.clone(), signal.signal_type.clone())
    };
    let detail = if online.is_empty() {
        signal.sub.clone()
    } else {
        let mut places: Vec<&str> = online.iter().map(|l| l.platform.as_str()).collect();
        places.dedup();
        format!("Found on {}.", places.join(", "))
    };
    // Pages found only for a same-name company in another country
    // ("Mirian Solutions, Inc." in the USA for a vendor in Pakistan) do
    // not count as this vendor being online.
    let titles: Vec<&str> = online.iter().map(|l| l.title.as_str()).collect();
    let (value, kind, detail) = match other_company_pages(company, country, &titles) {
        Some(note) => (
            "Only a same-name company".to_string(),
            "caution".to_string(),
            note,
        ),
        None => (value, kind, detail),
    };
    cards.push(card(
        "social_presence",
        "Online presence",
        &value,
        &kind,
        detail,
    ));

    cards.push(if scam_pages.is_empty() {
        card(
            "web_warnings",
            "Scam warnings online",
            "None found",
            "good",
            "A web search found no pages with this company's name next to scam words.".to_string(),
        )
    } else {
        card(
            "web_warnings",
            "Scam warnings online",
            &format!("{} found", scam_pages.len()),
            "bad",
            scam_pages
                .iter()
                .take(3)
                .map(|p| format!("{} ({})", p.title, p.url))
                .collect::<Vec<_>>()
                .join(" · "),
        )
    });
    (online, presence, results)
}

/// One site of the "Online presence" list: was the company found there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Presence {
    pub platform: String,
    pub found: bool,
}

// ───────────────────────── the phone number ─────────────────────────

/// Words that show a page talks about a scam (English and Portuguese).
const PHONE_SCAM_WORDS: &[&str] = &[
    "scam",
    "scammer",
    "scammers",
    "scammed",
    "fraud",
    "fraudulent",
    "fraudster",
    "fake",
    "beware",
    "ripoff",
    "rip-off",
    "golpe",
    "golpista",
    "golpistas",
    "fraude",
    "estelionato",
    "estelionatário",
    "picareta",
];

/// Sites where people report phone numbers that called or scammed them.
const PHONE_REPORT_SITES: &str = "(site:800notes.com OR site:tellows.com OR site:shouldianswer.com OR site:whocallsme.com OR site:ripoffreport.com OR site:reclameaqui.com.br)";

/// One search result.
#[derive(Debug, Clone, PartialEq)]
pub struct WebPage {
    pub url: String,
    pub title: String,
    pub text: String,
}

/// The phone number's digits.
pub fn phone_digits(phone: &str) -> String {
    phone.chars().filter(char::is_ascii_digit).collect()
}

/// How the number is searched: as the vendor wrote it, and as plain
/// digits ("+55 11 91234-5678" and "+5511912345678"), because pages
/// write numbers in different ways.
pub fn phone_variants(phone: &str) -> Vec<String> {
    let typed = phone.split_whitespace().collect::<Vec<_>>().join(" ");
    let digits = phone_digits(phone);
    let plain = if typed.starts_with('+') {
        format!("+{digits}")
    } else {
        digits
    };
    let mut out = vec![typed];
    if !out.contains(&plain) {
        out.push(plain);
    }
    out
}

/// True when the page shows this number (its last 8 digits, whatever
/// the spaces and dashes).
fn page_has_phone(page: &WebPage, last8: &str) -> bool {
    phone_digits(&format!("{} {} {}", page.title, page.text, page.url)).contains(last8)
}

fn has_scam_word(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower
        .split(|c: char| !(c.is_alphanumeric() || c == '-'))
        .any(|w| PHONE_SCAM_WORDS.contains(&w))
}

/// True when the page names the company (every word of its name, legal
/// forms left out, accents ignored).
fn page_names_company(page: &WebPage, company: &str) -> bool {
    let Some(name) = sanctions::compared_name(&fold_accents(company)) else {
        return false;
    };
    let text = fold_accents(&format!("{} {}", page.title, page.text)).to_lowercase();
    name.split_whitespace().all(|w| text.contains(w))
}

fn page_list(pages: &[&WebPage], max: usize) -> String {
    pages
        .iter()
        .take(max)
        .map(|p| format!("{} ({})", p.title, p.url))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The two phone cards, from the search results:
/// - "Phone in scam reports": pages with this number next to a scam
///   word (the whole web and phone-report sites);
/// - "Phone number online": the pages that list this number - with the
///   company's name (good), only with other names (worth a look), or
///   none (a note: common for small companies).
pub fn phone_cards(
    phone: &str,
    company: &str,
    scam_pages: &[WebPage],
    plain_pages: &[WebPage],
) -> Vec<VendorCard> {
    let digits = phone_digits(phone);
    let last8 = &digits[digits.len().saturating_sub(8)..];
    let mut seen = Vec::new();
    let reported: Vec<&WebPage> = scam_pages
        .iter()
        .filter(|p| page_has_phone(p, last8) && has_scam_word(&format!("{} {}", p.title, p.text)))
        .filter(|p| {
            let new = !seen.contains(&p.url);
            seen.push(p.url.clone());
            new
        })
        .collect();
    let mut seen = Vec::new();
    let listed: Vec<&WebPage> = plain_pages
        .iter()
        .filter(|p| page_has_phone(p, last8))
        .filter(|p| {
            let new = !seen.contains(&p.url);
            seen.push(p.url.clone());
            new
        })
        .collect();

    let mut cards = vec![if reported.is_empty() {
        card(
            "phone_scam",
            "Phone in scam reports",
            "None found",
            "good",
            format!(
                "A web search found no scam reports for {phone}, on the web or on phone-report sites."
            ),
        )
    } else {
        card(
            "phone_scam",
            "Phone in scam reports",
            &format!("{} found", reported.len()),
            "bad",
            format!(
                "Pages with {phone} next to scam words: {}",
                page_list(&reported, 3)
            ),
        )
    }];

    let with_name: Vec<&WebPage> = listed
        .iter()
        .copied()
        .filter(|p| page_names_company(p, company))
        .collect();
    cards.push(if listed.is_empty() {
        card(
            "phone_online",
            "Phone number online",
            "Not found",
            "info",
            format!("No public page lists {phone}. This is common for small companies."),
        )
    } else if !with_name.is_empty() {
        card(
            "phone_online",
            "Phone number online",
            "Matches the company",
            "good",
            format!(
                "{} page(s) list this number with the company's name: {}",
                with_name.len(),
                page_list(&with_name, 2)
            ),
        )
    } else {
        card(
            "phone_online",
            "Phone number online",
            "Other names",
            "caution",
            format!(
                "{} page(s) list this number, but none with the company's name. Check who really uses it: {}",
                listed.len(),
                page_list(&listed, 2)
            ),
        )
    });
    cards
}

/// Searches the vendor's phone number (Serper, 5 searches at most):
/// with scam words, on phone-report sites, and on its own.
async fn phone_search(phone: &str, company: &str, country: Option<&str>) -> Vec<VendorCard> {
    let not_checked = |why: &str| {
        vec![
            card(
                "phone_scam",
                "Phone in scam reports",
                "Not checked",
                "info",
                why.to_string(),
            ),
            card(
                "phone_online",
                "Phone number online",
                "Not checked",
                "info",
                why.to_string(),
            ),
        ]
    };
    if phone_digits(phone).len() < 8 {
        return not_checked("The phone number is too short to search.");
    }
    if !serper_enabled() {
        return not_checked(
            "The web search is switched off on this server, so this was not checked.",
        );
    }
    let words = "(scam OR scammer OR fraud OR fake OR golpe OR golpista OR fraude)";
    let variants = phone_variants(phone);
    let mut queries: Vec<(bool, String)> = Vec::new();
    for v in &variants {
        queries.push((true, format!("\"{v}\" {words}")));
        queries.push((true, format!("\"{v}\" {PHONE_REPORT_SITES}")));
    }
    queries.push((false, format!("\"{}\"", variants[0])));

    let country_code = match country.map(|c| countries_match(c, "Brazil")) {
        Some(true) => "br",
        _ => "us",
    };
    let mut set = tokio::task::JoinSet::new();
    for (scam, q) in queries {
        set.spawn(async move { (scam, run_serper_search(&q, Some(country_code)).await) });
    }
    let (mut scam_pages, mut plain_pages, mut answered) = (Vec::new(), Vec::new(), 0);
    while let Some(Ok((scam, answer))) = set.join_next().await {
        let Some(answer) = answer else { continue };
        answered += 1;
        for r in answer.organic.into_iter().take(8) {
            let page = WebPage {
                url: r.link,
                title: r.title,
                text: r.snippet.unwrap_or_default(),
            };
            if scam {
                scam_pages.push(page);
            } else {
                plain_pages.push(page);
            }
        }
    }
    if answered == 0 {
        return not_checked("The web search did not answer. Safely did not count this as clear.");
    }
    phone_cards(phone, company, &scam_pages, &plain_pages)
}

/// Runs the check of a sent form, counts the plan's scan (once), saves
/// the result and emails the person who invited. With no scans left the
/// check waits ("no_scans") and the scheduler tries again later.
pub async fn run_check(pool: &Pool<Postgres>, id: Uuid) {
    let Ok(row) = load(pool, id).await else {
        return;
    };
    if !matches!(row.status.as_str(), "checking" | "no_scans") {
        return;
    }
    let who = row.invited_by.unwrap_or(row.team_owner_id);
    if !row.scan_counted {
        match reserve_scan(pool, who).await {
            Ok(_) => {
                let _ = query("UPDATE vendor_checks SET scan_counted = TRUE WHERE id = $1")
                    .bind(id)
                    .execute(pool)
                    .await;
            }
            Err(_) => {
                let _ = query("UPDATE vendor_checks SET status = 'no_scans', updated_at = NOW() WHERE id = $1")
                    .bind(id)
                    .execute(pool)
                    .await;
                if row.status == "checking" {
                    email_buyer(pool, &row, None).await;
                }
                return;
            }
        }
    }
    let known: Known = serde_json::from_value(row.known.clone()).unwrap_or_default();
    let answers = answers_from(&row.answers);
    let found = build_result(pool, id, &known, &answers, row.email_confirmed.as_deref()).await;
    let (points, level) = score(&found.cards, &found.records);
    let result = json!({
        "cards": found.cards,
        "records": found.records,
        "online": found.online,
        "presence": found.presence,
        "social_candidates": found.social_candidates,
        "checked_at": Utc::now().to_rfc3339(),
    });
    let saved = query(
        "UPDATE vendor_checks SET result = $2, risk_score = $3, risk_level = $4, status = 'ready',
                checked_at = NOW(), updated_at = NOW()
         WHERE id = $1 AND status IN ('checking', 'no_scans')",
    )
    .bind(id)
    .bind(result)
    .bind(points)
    .bind(level)
    .execute(pool)
    .await;
    match saved {
        Ok(done) if done.rows_affected() > 0 => email_buyer(pool, &row, Some(level)).await,
        Ok(_) => {}
        Err(e) => eprintln!("Safely: could not save a vendor check: {e}"),
    }
}

/// Emails the person who invited: the result is ready (Some(level)), or
/// the check waits for scans (None).
async fn email_buyer(pool: &Pool<Postgres>, row: &CheckRow, level: Option<&str>) {
    let who = row.invited_by.unwrap_or(row.team_owner_id);
    let Ok(Some(email)) = query_scalar::<_, String>("SELECT email FROM users WHERE id = $1")
        .bind(who)
        .fetch_optional(pool)
        .await
    else {
        return;
    };
    let base = std::env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| "https://safely.sh".to_string());
    let open = format!("{}/vendors/{}", base.trim_end_matches('/'), row.id);
    let name = answers_from(&row.answers)
        .get("company_name")
        .cloned()
        .unwrap_or(row.vendor_name.clone());
    let text = ready_text(&name, level, &open);
    if let Err(e) = send_vendor_email(&email, &text, None).await {
        eprintln!("Safely: could not send a vendor result email: {e:?}");
    }
}

// ───────────────────────── emails ─────────────────────────

fn safety_footer(pt: bool, buyer: &str) -> String {
    if pt {
        format!(
            "Se você não trabalha com {buyer}, pode ignorar este e-mail. O Safely nunca pede senhas ou números de cartão."
        )
    } else {
        format!(
            "If you don't work with {buyer}, you can ignore this email. Safely never asks for passwords or card numbers."
        )
    }
}

/// The vendor's invite (or its reminder).
pub fn invite_text(
    language: &str,
    buyer: &str,
    vendor: &str,
    link: &str,
    reminder: bool,
) -> VendorEmailText {
    let pt = language == "pt";
    let (subject, title, first) = match (pt, reminder) {
        (false, false) => (
            format!("{buyer} asks you to fill in a short supplier form"),
            format!("Supplier form from {buyer}"),
            format!(
                "{buyer} uses Safely to check new suppliers. They asked you to fill in a short form about your company."
            ),
        ),
        (false, true) => (
            format!("Reminder: {buyer}'s supplier form"),
            "A quick reminder".to_string(),
            format!("{buyer} is still waiting for your supplier form."),
        ),
        (true, false) => (
            format!("{buyer} pede que você preencha um formulário rápido de fornecedor"),
            format!("Formulário de fornecedor de {buyer}"),
            format!(
                "{buyer} usa o Safely para verificar novos fornecedores. Eles pediram que você preencha um formulário rápido sobre a sua empresa."
            ),
        ),
        (true, true) => (
            format!("Lembrete: formulário de fornecedor de {buyer}"),
            "Um lembrete rápido".to_string(),
            format!("{buyer} ainda está esperando o seu formulário de fornecedor."),
        ),
    };
    VendorEmailText {
        preheader: first.clone(),
        subject,
        title,
        subtitle: if pt {
            "Leva cerca de 3 minutos".into()
        } else {
            "It takes about 3 minutes".into()
        },
        lines: vec![
            if pt {
                format!("Olá, {vendor},")
            } else {
                format!("Hi {vendor},")
            },
            first,
            if pt {
                "Tenha em mãos o número de registro da empresa (CNPJ ou outro), o nome do titular da conta bancária e acesso a este e-mail.".into()
            } else {
                "You will need your company number, the name on your bank account and access to this email address.".into()
            },
        ],
        code: String::new(),
        button_text: if pt {
            "Abrir o formulário".into()
        } else {
            "Open the form".into()
        },
        button_url: link.to_string(),
        small_note: if pt {
            "O link funciona por 14 dias.".into()
        } else {
            "The link works for 14 days.".into()
        },
        footer_note: safety_footer(pt, buyer),
    }
}

/// The email code.
pub fn code_text(language: &str, buyer: &str, code: &str) -> VendorEmailText {
    let pt = language == "pt";
    VendorEmailText {
        subject: if pt {
            format!("Seu código: {code}")
        } else {
            format!("Your code: {code}")
        },
        preheader: if pt {
            format!("Seu código é {code}")
        } else {
            format!("Your code is {code}")
        },
        title: if pt {
            "Seu código de e-mail".into()
        } else {
            "Your email code".into()
        },
        subtitle: if pt {
            format!("Para o formulário de fornecedor de {buyer}")
        } else {
            format!("For {buyer}'s supplier form")
        },
        lines: vec![if pt {
            "Digite este código no formulário para confirmar o seu e-mail.".into()
        } else {
            "Type this code in the form to confirm your email address.".into()
        }],
        code: code.to_string(),
        button_text: String::new(),
        button_url: String::new(),
        small_note: if pt {
            "O código funciona por 15 minutos.".into()
        } else {
            "The code works for 15 minutes.".into()
        },
        footer_note: if pt {
            "Se você não pediu este código, pode ignorar este e-mail.".into()
        } else {
            "If you did not ask for this code, you can ignore this email.".into()
        },
    }
}

/// "Please update a few answers" (or its reminder).
pub fn ask_text(
    language: &str,
    buyer: &str,
    vendor: &str,
    link: &str,
    fields: &[String],
    note: Option<&str>,
    reminder: bool,
) -> VendorEmailText {
    let pt = language == "pt";
    let names: Vec<&str> = fields
        .iter()
        .map(|f| {
            if pt {
                field_label_pt(f)
            } else {
                field_label(f)
            }
        })
        .collect();
    let list = names.join(", ");
    let mut lines = vec![
        if pt {
            format!("Olá, {vendor},")
        } else {
            format!("Hi {vendor},")
        },
        if pt {
            format!("{buyer} pediu que você atualize: {list}.")
        } else {
            format!("{buyer} asked you to update: {list}.")
        },
    ];
    if let Some(note) = note {
        lines.push(if pt {
            format!("Recado: \"{note}\"")
        } else {
            format!("Their note: \"{note}\"")
        });
    }
    let prefix = match (pt, reminder) {
        (false, true) => "Reminder: ",
        (true, true) => "Lembrete: ",
        _ => "",
    };
    VendorEmailText {
        subject: if pt {
            format!("{prefix}{buyer} pede que você atualize o formulário de fornecedor")
        } else {
            format!("{prefix}{buyer} asks you to update your supplier form")
        },
        preheader: lines[1].clone(),
        title: if pt {
            "Atualize algumas respostas".into()
        } else {
            "Please update a few answers".into()
        },
        subtitle: if pt {
            "Só as respostas pedidas".into()
        } else {
            "Only the answers they asked for".into()
        },
        lines,
        code: String::new(),
        button_text: if pt {
            "Atualizar o formulário".into()
        } else {
            "Update the form".into()
        },
        button_url: link.to_string(),
        small_note: if pt {
            "O link funciona por 14 dias.".into()
        } else {
            "The link works for 14 days.".into()
        },
        footer_note: safety_footer(pt, buyer),
    }
}

/// "Confirm your new bank details".
pub fn bank_text(
    language: &str,
    buyer: &str,
    vendor: &str,
    link: &str,
    note: Option<&str>,
) -> VendorEmailText {
    let pt = language == "pt";
    let mut lines = vec![
        if pt {
            format!("Olá, {vendor},")
        } else {
            format!("Hi {vendor},")
        },
        if pt {
            format!(
                "{buyer} precisa que você confirme os seus dados bancários (titular, número da conta e código do banco) antes do próximo pagamento."
            )
        } else {
            format!(
                "{buyer} needs you to confirm your bank details (account name, account number and bank code) before the next payment."
            )
        },
        if pt {
            "Se você não mudou de banco, ou se não esperava este e-mail, fale com eles por telefone antes de responder.".into()
        } else {
            "If you did not change banks, or did not expect this email, call them before you answer.".into()
        },
    ];
    if let Some(note) = note {
        lines.push(if pt {
            format!("Recado: \"{note}\"")
        } else {
            format!("Their note: \"{note}\"")
        });
    }
    VendorEmailText {
        subject: if pt {
            format!("{buyer} pede que você confirme os dados bancários")
        } else {
            format!("{buyer} asks you to confirm your bank details")
        },
        preheader: lines[1].clone(),
        title: if pt {
            "Confirme os dados bancários".into()
        } else {
            "Confirm your bank details".into()
        },
        subtitle: if pt {
            "Leva menos de 1 minuto".into()
        } else {
            "It takes less than a minute".into()
        },
        lines,
        code: String::new(),
        button_text: if pt {
            "Confirmar".into()
        } else {
            "Confirm".into()
        },
        button_url: link.to_string(),
        small_note: if pt {
            "O link funciona por 14 dias.".into()
        } else {
            "The link works for 14 days.".into()
        },
        footer_note: safety_footer(pt, buyer),
    }
}

/// The buyer's email: the result is ready, or the check waits for scans.
pub fn ready_text(vendor: &str, level: Option<&str>, open: &str) -> VendorEmailText {
    let (subject, title, lines) = match level {
        Some(level) => {
            let words = match level {
                "high" => "High risk. Look at the warnings before you pay.",
                "caution" => "Caution. A few things are worth a second look.",
                _ => "Low risk. Nothing serious was found.",
            };
            (
                format!("Vendor check ready: {vendor}"),
                "Vendor check ready".to_string(),
                vec![
                    format!("{vendor} sent their supplier form and Safely checked the answers."),
                    format!("Result: {words}"),
                ],
            )
        }
        None => (
            format!("Vendor check waiting: {vendor}"),
            "Your plan has no scans left".to_string(),
            vec![
                format!("{vendor} sent their supplier form, but your plan has no scans left this month."),
                "The check runs by itself when your scans renew. You can also run it from the Vendors page.".to_string(),
            ],
        ),
    };
    VendorEmailText {
        preheader: lines[0].clone(),
        subject,
        title,
        subtitle: vendor.to_string(),
        lines,
        code: String::new(),
        button_text: "See the result".to_string(),
        button_url: open.to_string(),
        small_note: "Only your team can see this result. The vendor can't.".to_string(),
        footer_note: "You get this email because you invited this vendor on Safely.".to_string(),
    }
}

// ───────────────────────── background jobs ─────────────────────────

/// One round of the background jobs: close old links, send the 3-day
/// reminders, run checks that wait for scans (every 6 hours) or that
/// stopped (a restart while checking). Returns how many checks ran.
pub async fn run_vendor_jobs(pool: &Pool<Postgres>) -> usize {
    let _ = query(
        "UPDATE vendor_checks SET status = CASE WHEN result IS NOT NULL THEN 'ready' ELSE 'expired' END,
                ask_fields = CASE WHEN result IS NOT NULL THEN '[]'::jsonb ELSE ask_fields END,
                updated_at = NOW()
         WHERE status IN ('invited', 'opened', 'more_info') AND expires_at < NOW()",
    )
    .execute(pool)
    .await;

    let due: Vec<CheckRow> = query_as(
        "UPDATE vendor_checks SET reminder_sent_at = NOW()
         WHERE id IN (
            SELECT id FROM vendor_checks
            WHERE send_email AND reminder_sent_at IS NULL
              AND status IN ('invited', 'opened', 'more_info')
              AND asked_at < NOW() - make_interval(days => $1)
              AND expires_at > NOW()
            ORDER BY asked_at LIMIT 20 FOR UPDATE SKIP LOCKED)
         RETURNING *",
    )
    .bind(REMIND_AFTER_DAYS as i32)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for row in &due {
        if let Err(e) = send_reminder(pool, row, None).await {
            eprintln!("Safely: could not send a vendor reminder: {e:?}");
        }
    }

    let waiting: Vec<Uuid> = query_scalar(
        "UPDATE vendor_checks SET updated_at = NOW()
         WHERE id IN (
            SELECT id FROM vendor_checks
            WHERE (status = 'no_scans' AND updated_at < NOW() - INTERVAL '6 hours')
               OR (status = 'checking' AND updated_at < NOW() - INTERVAL '30 minutes')
            ORDER BY updated_at LIMIT 10 FOR UPDATE SKIP LOCKED)
         RETURNING id",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for id in &waiting {
        run_check(pool, *id).await;
    }
    waiting.len()
}

/// Runs run_vendor_jobs every 15 minutes, in the background. Off when
/// SAFELY_VENDOR_JOBS_DISABLED is "true" (for example on a second
/// server).
pub fn start_vendor_scheduler(pool: Pool<Postgres>) {
    if std::env::var("SAFELY_VENDOR_JOBS_DISABLED").is_ok_and(|v| v.trim() == "true") {
        println!("Safely: vendor check jobs are off (SAFELY_VENDOR_JOBS_DISABLED)");
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(90)).await;
        loop {
            run_vendor_jobs(&pool).await;
            tokio::time::sleep(Duration::from_secs(900)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answers(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn a_link_is_saved_only_as_its_hash() {
        let token = new_token();
        assert_eq!(token.len(), 64);
        assert_ne!(new_token(), token);
        assert_eq!(hash_token(&token).len(), 64);
        assert_ne!(hash_token(&token), token);
        assert_eq!(hash_token(&token), hash_token(&format!(" {token} ")));
    }

    #[test]
    fn codes_have_six_digits() {
        for _ in 0..50 {
            let code = new_code();
            assert_eq!(code.len(), 6);
            assert!(code.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn company_names_match_without_legal_forms() {
        assert!(names_match("ACME Comércio Ltda.", "Acme Comercio LTDA"));
        assert!(names_match("Acme Trading Co., Ltd.", "ACME TRADING"));
        assert!(names_match("Acme Trading", "Acme Trading Import Export"));
        assert!(!names_match("Acme Trading", "John Smith"));
        assert!(!names_match("Acme Trading", "Acmex Trading"));
    }

    #[test]
    fn phones_match_on_the_last_eight_digits() {
        assert!(phones_match("+55 11 91234-5678", "(11) 91234-5678"));
        assert!(!phones_match("+55 11 91234-5678", "+55 11 98888-7777"));
    }

    #[test]
    fn free_email_is_a_caution_and_a_different_bank_name_is_bad() {
        let known = Known {
            name: Some("Acme Trading".into()),
            email: Some("sales@acme.com".into()),
            country: Some("Brazil".into()),
            ..Known::default()
        };
        let a = answers(&[
            ("company_name", "Acme Trading Ltda"),
            ("country", "Brazil"),
            ("email", "acme.sales@gmail.com"),
            ("bank_holder", "Carlos Pereira"),
        ]);
        let cards = answer_cards(&known, &a, Some("acme.sales@gmail.com"));
        let find = |k: &str| cards.iter().find(|c| c.key == k).cloned();
        assert_eq!(find("email_code").unwrap().kind, "good");
        assert_eq!(find("free_email").unwrap().kind, "caution");
        assert_eq!(find("bank_name").unwrap().kind, "bad");
        // A company address invited, a gmail answered: red.
        assert_eq!(find("known_email").unwrap().kind, "bad");
        assert!(find("known_country").is_none());
        assert!(find("known_all").is_none());
        assert_eq!(find("website_missing").unwrap().kind, "info");
    }

    #[test]
    fn matching_answers_give_one_good_card() {
        let known = Known {
            name: Some("Acme".into()),
            phone: Some("+1 415 555 0100".into()),
            website: Some("https://www.acme.com/".into()),
            ..Known::default()
        };
        let a = answers(&[
            ("company_name", "Acme Inc."),
            ("phone", "(415) 555-0100"),
            ("website", "acme.com"),
            ("email", "ana@acme.com"),
            ("bank_holder", "ACME INC"),
        ]);
        let cards = answer_cards(&known, &a, None);
        let find = |k: &str| cards.iter().find(|c| c.key == k).cloned();
        assert_eq!(find("known_all").unwrap().kind, "good");
        assert_eq!(find("bank_name").unwrap().kind, "good");
        assert_eq!(find("email_code").unwrap().kind, "bad");
        assert_eq!(find("free_email").unwrap().kind, "good");
    }

    #[test]
    fn a_lookalike_email_domain_is_bad() {
        let c = email_vs_website_card("acrne.com", "acme.com").unwrap();
        assert_eq!(c.kind, "bad");
        let c = email_vs_website_card("acme.com", "www.acme.com").unwrap();
        assert_eq!(c.kind, "good");
        let c = email_vs_website_card("otherbiz.net", "acme.com").unwrap();
        assert_eq!(c.kind, "caution");
    }

    #[test]
    fn new_domains_are_bad() {
        assert_eq!(
            age_card(
                "k",
                "l",
                "x.com",
                Some((Verdict::New, Some("2026-10-01"), Some(40)))
            )
            .kind,
            "bad"
        );
        assert_eq!(
            age_card(
                "k",
                "l",
                "x.com",
                Some((Verdict::Established, Some("2010-01-01"), Some(6000)))
            )
            .kind,
            "good"
        );
        assert_eq!(age_card("k", "l", "x.com", None).value, "Not checked");
    }

    #[test]
    fn the_score_and_level() {
        let c = |kind: &str| card("k", "l", "v", kind, String::new());
        assert_eq!(score(&[c("good"), c("info")], &[]), (0, "low"));
        assert_eq!(
            score(&[c("caution"), c("caution"), c("caution")], &[]).1,
            "caution"
        );
        assert_eq!(score(&[c("bad"), c("bad")], &[]).1, "high");
        let sanctioned = Signal {
            label: sanctions::LABEL.to_string(),
            sub: String::new(),
            value: "Listed".into(),
            signal_type: "bad".into(),
            category: OUTSIDE_RECORDS.into(),
            check_type: "existence".into(),
        };
        assert_eq!(score(&[c("good")], &[sanctioned]), (100, "high"));
    }

    #[test]
    fn missing_answers_are_listed_and_website_is_optional() {
        let a = answers(&[("company_name", "Acme"), ("country", "Brazil")]);
        let missing = missing_fields(&a);
        assert!(missing.contains(&"company_number"));
        assert!(!missing.contains(&"website"));
    }

    #[test]
    fn unknown_fields_are_left_out() {
        let a =
            answers_from(&json!({"company_name": "  Acme   Ltd ", "password": "x", "phone": 5}));
        assert_eq!(a.get("company_name").unwrap(), "Acme Ltd");
        assert!(!a.contains_key("password"));
        assert!(!a.contains_key("phone"));
    }

    #[test]
    fn the_emails_render_in_both_languages() {
        for lang in ["en", "pt"] {
            let html = crate::services::features::email::render_vendor_email(
                &invite_text(lang, "Buyer <Co>", "Acme", "https://safely.sh/v/abc", false),
                "https://safely.sh/images/white_logo.png",
            )
            .unwrap();
            assert!(html.contains("https://safely.sh/v/abc"));
            assert!(html.contains("Buyer &lt;Co&gt;"));
            let code = crate::services::features::email::render_vendor_email(
                &code_text(lang, "Buyer", "123456"),
                "https://safely.sh/images/white_logo.png",
            )
            .unwrap();
            assert!(code.contains("123456"));
            assert!(!code.contains("&rarr;"));
        }
    }

    #[test]
    fn a_web_search_that_did_not_run_is_not_checked() {
        let mut cards = Vec::new();
        assert!(
            web_cards(&mut cards, WebSearch::Off, "Acme", None)
                .0
                .is_empty()
        );
        assert_eq!(cards.len(), 2);
        assert!(
            cards
                .iter()
                .all(|c| c.value == "Not checked" && c.kind == "info")
        );
    }

    #[test]
    fn online_pages_and_scam_pages_are_kept_apart() {
        use crate::services::features::osint::SocialCandidateLink;
        let link = |url: &str| SocialCandidateLink {
            platform: "x".into(),
            title: "Page".into(),
            url: url.into(),
        };
        let row = |platform: &str, scam: bool, url: &str| PlatformCheckResult {
            platform: platform.into(),
            variant_searched: "Acme".into(),
            found: true,
            candidates: vec![link(url)],
            scam_mention: scam,
        };
        let signal = Signal {
            label: "Social presence check".into(),
            sub: "Found.".into(),
            value: SCAM_MENTIONS_FOUND.into(),
            signal_type: "bad".into(),
            category: "external_intelligence".into(),
            check_type: "existence".into(),
        };
        let mut cards = Vec::new();
        let (online, presence, _) = web_cards(
            &mut cards,
            WebSearch::Done(
                signal,
                vec![
                    row("LinkedIn", false, "https://linkedin.com/company/acme"),
                    row("Facebook", true, "https://facebook.com/acme-scam"),
                    row(
                        crate::services::features::osint::WEB_SCAM_SEARCH,
                        false,
                        "https://blog.example/acme-scam",
                    ),
                ],
            ),
            "Acme",
            None,
        );
        assert_eq!(online.len(), 1);
        assert_eq!(online[0].platform, "LinkedIn");
        assert_eq!(
            presence,
            vec![Presence {
                platform: "LinkedIn".into(),
                found: true
            }]
        );
        let find = |k: &str| cards.iter().find(|c| c.key == k).unwrap().clone();
        // Scam mentions count once, on their own card.
        assert_eq!(find("social_presence").kind, "good");
        assert_eq!(find("web_warnings").kind, "bad");
        assert_eq!(find("web_warnings").value, "2 found");
    }

    fn page(url: &str, title: &str, text: &str) -> WebPage {
        WebPage {
            url: url.into(),
            title: title.into(),
            text: text.into(),
        }
    }

    #[test]
    fn the_phone_is_searched_as_written_and_as_digits() {
        assert_eq!(
            phone_variants("+55 11  91234-5678"),
            vec![
                "+55 11 91234-5678".to_string(),
                "+5511912345678".to_string()
            ]
        );
        assert_eq!(
            phone_variants("11912345678"),
            vec!["11912345678".to_string()]
        );
    }

    #[test]
    fn a_phone_next_to_scam_words_is_reported() {
        let scam = vec![
            page(
                "https://800notes.com/x",
                "(11) 91234-5678 - scam caller",
                "Fake supplier, took my money",
            ),
            // Same scam words, but another number: not this phone.
            page("https://800notes.com/y", "(11) 95555-0000 - scam", "scam"),
            // This number, but no scam word: not a report.
            page(
                "https://example.com/z",
                "Acme contact",
                "Call +55 11 91234-5678",
            ),
        ];
        let cards = phone_cards("+55 11 91234-5678", "Acme Ltda", &scam, &[]);
        assert_eq!(cards[0].key, "phone_scam");
        assert_eq!(cards[0].kind, "bad");
        assert_eq!(cards[0].value, "1 found");
        assert_eq!(cards[1].kind, "info");
    }

    #[test]
    fn who_uses_the_phone_online() {
        let with_name = vec![page(
            "https://acme.com.br",
            "Acme Comércio",
            "Fale conosco: (11) 91234-5678",
        )];
        let cards = phone_cards("+55 11 91234-5678", "ACME Comercio Ltda", &[], &with_name);
        assert_eq!(cards[0].kind, "good");
        assert_eq!(cards[1].value, "Matches the company");

        let other = vec![page(
            "https://shop.example",
            "Bob's Phones",
            "WhatsApp 11 91234 5678",
        )];
        let cards = phone_cards("+55 11 91234-5678", "Acme Comercio Ltda", &[], &other);
        assert_eq!(cards[1].value, "Other names");
        assert_eq!(cards[1].kind, "caution");
    }

    /// Bilal's test vendor (Nov 19): a person's bank account, gmail, no
    /// website, country changed from USA to Pakistan, a made-up company
    /// number and bank code, and pages only for a US "Inc." company.
    #[test]
    fn the_mirian_solutions_vendor_is_high_risk() {
        let known = Known {
            name: Some("Mirian Solutions".into()),
            email: Some("veinstree@gmail.com".into()),
            country: Some("USA".into()),
            ..Known::default()
        };
        let a = answers(&[
            ("company_name", "Mirian Solutions"),
            ("country", "Pakistan"),
            ("company_number", "123456789"),
            ("email", "veinstree@gmail.com"),
            ("phone", "+923158105028"),
            ("bank_holder", "Bilal Khan"),
            ("bank_account", "12343567"),
            ("bank_code", "5423fd"),
        ]);
        let mut cards = answer_cards(&known, &a, Some("veinstree@gmail.com"));
        cards.extend(crate::services::features::vendor_checks::bank_cards(
            Some("12343567"),
            Some("5423fd"),
            Some("Pakistan"),
        ));
        cards.push(
            crate::services::features::vendor_checks::company_number_card(
                "123456789",
                Some("Pakistan"),
            ),
        );
        let note = crate::services::features::vendor_checks::other_company_pages(
            "Mirian Solutions",
            Some("Pakistan"),
            &[
                "Mirian Solutions, Inc. Careers and Employment",
                "Working at Mirian Solutions, Inc.: Employee Reviews",
            ],
        );
        assert!(note.is_some());
        let find = |k: &str| cards.iter().find(|c| c.key == k).cloned().unwrap();
        assert_eq!(find("known_country").kind, "bad");
        assert_eq!(find("no_website_free_email").kind, "caution");
        assert_eq!(find("bank_code").value, "Not a bank code");
        assert_eq!(find("company_number").value, "Looks made up");
        let (points, level) = score(&cards, &[]);
        // bank name 25 + country 25 + free email 8 + no website 8 +
        // bank code 8 + number 8 = 82 (+8 more with the online-presence card).
        assert_eq!(points, 82);
        assert_eq!(level, "high");
    }

    #[test]
    fn another_email_at_the_same_company_is_fine() {
        let known = Known {
            email: Some("ali@acme.com".into()),
            ..Known::default()
        };
        let colleague = answer_cards(&known, &answers(&[("email", "sara@acme.com")]), None);
        assert!(colleague.iter().all(|c| c.key != "known_email"));
        let other = answer_cards(&known, &answers(&[("email", "sara@other.com")]), None);
        assert_eq!(
            other.iter().find(|c| c.key == "known_email").unwrap().kind,
            "caution"
        );
        let free = answer_cards(&known, &answers(&[("email", "acme.sales@gmail.com")]), None);
        assert_eq!(
            free.iter().find(|c| c.key == "known_email").unwrap().kind,
            "bad"
        );
        // Invited a gmail, answered from another gmail: just different.
        let known = Known {
            email: Some("a@gmail.com".into()),
            ..Known::default()
        };
        let c = answer_cards(&known, &answers(&[("email", "b@gmail.com")]), None);
        assert_eq!(
            c.iter().find(|c| c.key == "known_email").unwrap().kind,
            "caution"
        );
    }
}
