//! More vendor checks (used by vendors.rs run_check), none of which
//! needs a paid service or a new key:
//!
//! - bank account: IBAN check digits (mod 97) and its country; SWIFT/BIC
//!   format and country; US routing number check digits;
//! - the same bank account given by another vendor;
//! - the phone's country code against the company's country;
//! - the address: mailbox words (PO box, virtual office) and the free
//!   OpenStreetMap address search (Nominatim, at most 1 a second);
//! - the website: does it show the company's name, or is it parked / for
//!   sale;
//! - documents the vendor uploads (company registration, bank letter):
//!   read with Claude (ANTHROPIC_API_KEY, already used by scans) and
//!   compared with the answers.

use crate::services::features::vendors::{VendorCard, fold_accents, names_match};
use crate::services::sources::databases::sanctions;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

fn card(key: &str, label: &str, value: &str, kind: &str, detail: String) -> VendorCard {
    VendorCard {
        key: key.to_string(),
        label: label.to_string(),
        value: value.to_string(),
        kind: kind.to_string(),
        detail,
    }
}

// ───────────────────────── countries ─────────────────────────

/// (ISO code, phone code, names in English and Portuguese, lower case).
const COUNTRIES: &[(&str, &str, &[&str])] = &[
    (
        "AE",
        "971",
        &[
            "united arab emirates",
            "uae",
            "emirados árabes unidos",
            "dubai",
        ],
    ),
    ("AR", "54", &["argentina"]),
    ("AT", "43", &["austria", "áustria"]),
    ("AU", "61", &["australia", "austrália"]),
    ("BD", "880", &["bangladesh"]),
    ("BE", "32", &["belgium", "bélgica"]),
    ("BR", "55", &["brazil", "brasil"]),
    ("CA", "1", &["canada", "canadá"]),
    ("CH", "41", &["switzerland", "suíça"]),
    ("CL", "56", &["chile"]),
    ("CN", "86", &["china", "people's republic of china", "prc"]),
    ("CO", "57", &["colombia", "colômbia"]),
    (
        "CZ",
        "420",
        &["czech republic", "czechia", "república tcheca"],
    ),
    ("DE", "49", &["germany", "alemanha", "deutschland"]),
    ("DK", "45", &["denmark", "dinamarca"]),
    ("EG", "20", &["egypt", "egito"]),
    ("ES", "34", &["spain", "espanha", "españa"]),
    ("FI", "358", &["finland", "finlândia"]),
    ("FR", "33", &["france", "frança"]),
    (
        "GB",
        "44",
        &[
            "united kingdom",
            "uk",
            "great britain",
            "england",
            "reino unido",
            "inglaterra",
        ],
    ),
    ("GR", "30", &["greece", "grécia"]),
    ("HK", "852", &["hong kong"]),
    ("HU", "36", &["hungary", "hungria"]),
    ("ID", "62", &["indonesia", "indonésia"]),
    ("IE", "353", &["ireland", "irlanda"]),
    ("IL", "972", &["israel"]),
    ("IN", "91", &["india", "índia"]),
    ("IT", "39", &["italy", "itália"]),
    ("JP", "81", &["japan", "japão"]),
    ("KE", "254", &["kenya", "quênia"]),
    (
        "KR",
        "82",
        &["south korea", "korea", "republic of korea", "coreia do sul"],
    ),
    ("LK", "94", &["sri lanka"]),
    ("MA", "212", &["morocco", "marrocos"]),
    ("MX", "52", &["mexico", "méxico"]),
    ("MY", "60", &["malaysia", "malásia"]),
    ("NG", "234", &["nigeria", "nigéria"]),
    (
        "NL",
        "31",
        &["netherlands", "holland", "holanda", "países baixos"],
    ),
    ("NO", "47", &["norway", "noruega"]),
    ("NZ", "64", &["new zealand", "nova zelândia"]),
    ("PE", "51", &["peru"]),
    ("PH", "63", &["philippines", "filipinas"]),
    ("PK", "92", &["pakistan", "paquistão"]),
    ("PL", "48", &["poland", "polônia"]),
    ("PT", "351", &["portugal"]),
    ("RO", "40", &["romania", "romênia"]),
    ("RU", "7", &["russia", "rússia"]),
    ("SA", "966", &["saudi arabia", "arábia saudita"]),
    ("SE", "46", &["sweden", "suécia"]),
    ("SG", "65", &["singapore", "singapura"]),
    ("TH", "66", &["thailand", "tailândia"]),
    ("TR", "90", &["turkey", "türkiye", "turquia"]),
    ("TW", "886", &["taiwan"]),
    ("UA", "380", &["ukraine", "ucrânia"]),
    (
        "US",
        "1",
        &[
            "united states",
            "usa",
            "us",
            "u.s.",
            "u.s.a.",
            "united states of america",
            "estados unidos",
            "eua",
        ],
    ),
    ("UY", "598", &["uruguay", "uruguai"]),
    ("VN", "84", &["vietnam", "viet nam", "vietnã"]),
    ("ZA", "27", &["south africa", "áfrica do sul"]),
];

/// The ISO code of a country written in words ("Brasil" -> "BR"), or of
/// the last part of a place ("São Paulo, Brazil" -> "BR").
pub fn country_code(country: &str) -> Option<&'static str> {
    let lower = country.trim().to_lowercase();
    let last = lower
        .rsplit([',', '|', '/', ';', '-'])
        .map(str::trim)
        .find(|p| !p.is_empty())
        .unwrap_or("");
    COUNTRIES
        .iter()
        .find(|(iso, _, names)| {
            names.contains(&lower.as_str())
                || names.contains(&last)
                || lower.eq_ignore_ascii_case(iso)
        })
        .map(|(iso, _, _)| *iso)
}

/// A country's name for a card ("BR" -> "Brazil").
pub fn country_name(iso: &str) -> String {
    COUNTRIES
        .iter()
        .find(|(code, _, _)| code.eq_ignore_ascii_case(iso))
        .map(|(_, _, names)| {
            let n = names[0];
            n.split(' ')
                .map(|w| {
                    let mut c = w.chars();
                    c.next()
                        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_else(|| iso.to_uppercase())
}

// ───────────────────────── bank ─────────────────────────

/// The account written without spaces or dashes, in capitals.
pub fn clean_account(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_uppercase()
}

/// True when the text has the shape of an IBAN (2 letters, 2 digits,
/// then 11 to 30 letters or digits).
pub fn looks_like_iban(account: &str) -> bool {
    let b = account.as_bytes();
    (15..=34).contains(&b.len())
        && b[0].is_ascii_uppercase()
        && b[1].is_ascii_uppercase()
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit()
}

/// The IBAN check digits (ISO 13616, mod 97 = 1).
pub fn iban_is_valid(account: &str) -> bool {
    if !looks_like_iban(account) {
        return false;
    }
    let moved = format!("{}{}", &account[4..], &account[..4]);
    let mut rest: u32 = 0;
    for ch in moved.chars() {
        let digits = if ch.is_ascii_digit() {
            ch.to_string()
        } else {
            ((ch as u32) - ('A' as u32) + 10).to_string()
        };
        for d in digits.chars() {
            rest = (rest * 10 + d.to_digit(10).unwrap_or(0)) % 97;
        }
    }
    rest == 1
}

/// True for the shape of a SWIFT/BIC code: 4 letters (bank), 2 letters
/// (country), 2 letters or digits (place), and 3 more for a branch.
pub fn looks_like_bic(code: &str) -> bool {
    let b = code.as_bytes();
    (b.len() == 8 || b.len() == 11)
        && b[..6].iter().all(u8::is_ascii_uppercase)
        && b[6..].iter().all(u8::is_ascii_alphanumeric)
}

/// The US bank routing number check digits (3, 7, 1 weights).
pub fn routing_is_valid(code: &str) -> bool {
    let d: Vec<u32> = code.chars().filter_map(|c| c.to_digit(10)).collect();
    if d.len() != 9 || code.chars().count() != 9 {
        return false;
    }
    let sum = 3 * (d[0] + d[3] + d[6]) + 7 * (d[1] + d[4] + d[7]) + (d[2] + d[5] + d[8]);
    sum.is_multiple_of(10) && sum != 0
}

fn place_note(account_country: &str, company: Option<&str>) -> Option<String> {
    let company = company?;
    if account_country.eq_ignore_ascii_case(company) {
        None
    } else {
        Some(format!(
            "The account is in {}, but the company is in {}. Ask why before you pay.",
            country_name(account_country),
            country_name(company)
        ))
    }
}

/// The bank account number and the bank code (SWIFT/BIC or US routing
/// number), against each other and the company's country.
pub fn bank_cards(
    account: Option<&str>,
    code: Option<&str>,
    country: Option<&str>,
) -> Vec<VendorCard> {
    let company = country.and_then(country_code);
    let mut cards = Vec::new();
    let account = account.map(clean_account).filter(|a| !a.is_empty());
    let code = code.map(clean_account).filter(|c| !c.is_empty());

    let mut iban_country: Option<String> = None;
    if let Some(acc) = &account {
        if looks_like_iban(acc) {
            if iban_is_valid(acc) {
                let cc = acc[..2].to_string();
                iban_country = Some(cc.clone());
                cards.push(match place_note(&cc, company) {
                    None => card(
                        "bank_account",
                        "Bank account number",
                        "Valid IBAN",
                        "good",
                        format!(
                            "The IBAN's check digits are right. Bank account in {}.",
                            country_name(&cc)
                        ),
                    ),
                    Some(note) => card(
                        "bank_account",
                        "Bank account number",
                        "Account abroad",
                        "caution",
                        format!("The IBAN is valid. {note}"),
                    ),
                });
            } else {
                cards.push(card(
                    "bank_account",
                    "Bank account number",
                    "Not a valid IBAN",
                    "bad",
                    "The IBAN's check digits are wrong: a typo, or a made-up number. Ask the vendor to send it again and confirm it by phone.".to_string(),
                ));
            }
        } else {
            cards.push(card(
                "bank_account",
                "Bank account number",
                "Given",
                "info",
                "This is not an IBAN, so its format can't be checked. Confirm it by phone with someone you know at the company.".to_string(),
            ));
        }
    }

    if let Some(c) = &code {
        if looks_like_bic(c) {
            let cc = c[4..6].to_string();
            let iban_differs = iban_country
                .as_deref()
                .is_some_and(|i| !i.eq_ignore_ascii_case(&cc));
            cards.push(if iban_differs {
                card(
                    "bank_code",
                    "Bank code (SWIFT/BIC)",
                    "Doesn't match the IBAN",
                    "bad",
                    format!(
                        "The SWIFT/BIC is for a bank in {}, but the IBAN is from {}. One of them is wrong.",
                        country_name(&cc),
                        country_name(iban_country.as_deref().unwrap_or(""))
                    ),
                )
            } else {
                match place_note(&cc, company) {
                    None => card(
                        "bank_code",
                        "Bank code (SWIFT/BIC)",
                        "Valid format",
                        "good",
                        format!("A SWIFT/BIC code for a bank in {}.", country_name(&cc)),
                    ),
                    Some(note) => card(
                        "bank_code",
                        "Bank code (SWIFT/BIC)",
                        "Bank abroad",
                        "caution",
                        format!("The SWIFT/BIC is for a bank in {}. {note}", country_name(&cc)),
                    ),
                }
            })
        } else if c.len() == 9 && c.chars().all(|ch| ch.is_ascii_digit()) {
            cards.push(if routing_is_valid(c) {
                card(
                    "bank_code",
                    "Bank code (US routing number)",
                    "Valid",
                    "good",
                    "The routing number's check digits are right.".to_string(),
                )
            } else {
                card(
                    "bank_code",
                    "Bank code (US routing number)",
                    "Not valid",
                    "bad",
                    "The routing number's check digits are wrong: a typo, or a made-up number."
                        .to_string(),
                )
            });
        } else {
            cards.push(card(
                "bank_code",
                "Bank code",
                "Not a bank code",
                "caution",
                "This is not a SWIFT/BIC code (8 or 11 letters and digits, like COBADEFFXXX) or a US routing number (9 digits). It may be mistyped or made up. Ask the vendor for the bank's SWIFT/BIC.".to_string(),
            ));
        }
    }
    cards
}

// ───────────────────────── company number ─────────────────────────

/// True for a number nobody's company really has: all one digit
/// ("000000"), counting up or down ("123456789", "987654"), or too
/// short to be a real number (under 4 digits).
pub fn looks_made_up(number: &str) -> bool {
    let d: Vec<u32> = number.chars().filter_map(|c| c.to_digit(10)).collect();
    if d.len() < 4 {
        return number.chars().filter(|c| c.is_alphanumeric()).count() < 4;
    }
    let same = d.iter().all(|x| *x == d[0]);
    let up = d.windows(2).all(|w| w[1] == (w[0] + 1) % 10);
    let down = d.windows(2).all(|w| w[0] == (w[1] + 1) % 10);
    same || up || down
}

/// The company number card where Safely has no number check (all
/// countries but China, Brazil and France).
pub fn company_number_card(number: &str, country: Option<&str>) -> VendorCard {
    if looks_made_up(number) {
        return card(
            "company_number",
            "Company number",
            "Looks made up",
            "caution",
            format!(
                "\"{number}\" does not look like a real company number (a run of digits or the same digit). Ask for the registration document."
            ),
        );
    }
    let place = country
        .and_then(country_code)
        .map(country_name)
        .unwrap_or_else(|| "this country".to_string());
    card(
        "company_number",
        "Company number",
        "Not checked",
        "info",
        format!(
            "Safely has no check for company numbers in {place} yet. Ask for the registration document and compare the number."
        ),
    )
}

// ───────────────────────── same-name company ─────────────────────────

/// Legal forms that belong to one country, written in a company's name:
/// (form, countries where it is used).
const LEGAL_FORMS: &[(&str, &[&str])] = &[
    ("inc", &["US", "CA"]),
    ("llc", &["US"]),
    ("corp", &["US", "CA"]),
    ("ltda", &["BR", "PT", "CO", "CL"]),
    ("eireli", &["BR"]),
    ("gmbh", &["DE", "AT", "CH"]),
    ("sarl", &["FR", "MA", "BE", "CH"]),
    ("sas", &["FR"]),
    ("pvt", &["PK", "IN", "LK", "BD"]),
    ("bv", &["NL", "BE"]),
    ("sdn bhd", &["MY"]),
    ("pte", &["SG"]),
    ("kk", &["JP"]),
];

fn legal_forms_in(text: &str) -> Vec<&'static (&'static str, &'static [&'static str])> {
    let words: Vec<String> = text
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    let joined = format!(" {} ", words.join(" "));
    LEGAL_FORMS
        .iter()
        .filter(|(form, _)| joined.contains(&format!(" {form} ")))
        .collect()
}

/// When every page found online is for a same-name company of another
/// country (its legal form, like "Inc.", is used in another country
/// than the vendor's, and the vendor did not write it), a note saying
/// so; None otherwise.
pub fn other_company_pages(
    company: &str,
    country: Option<&str>,
    titles: &[&str],
) -> Option<String> {
    let mine = country.and_then(country_code)?;
    if titles.is_empty() {
        return None;
    }
    let own_forms = legal_forms_in(company);
    let mut seen: Option<(&str, &[&str])> = None;
    for title in titles {
        let foreign = legal_forms_in(title)
            .into_iter()
            .find(|f| !f.1.contains(&mine) && !own_forms.iter().any(|o| o.0 == f.0));
        let f = foreign?;
        seen = Some((f.0, f.1));
    }
    let (form, places) = seen?;
    Some(format!(
        "The pages found are for a company named with \"{}\", a form used in {}, but this vendor says it is in {}. They are probably another company with the same name, so they don't show this vendor online.",
        form,
        places
            .iter()
            .map(|p| country_name(p))
            .collect::<Vec<_>>()
            .join(" / "),
        country_name(mine)
    ))
}

// ───────────────────────── phone ─────────────────────────

/// The phone's country from its "+" code ("+55 11..." -> "BR"). The
/// longest code wins ("+852" Hong Kong before "+85"). "+1" is the USA
/// and Canada, so it gives "US".
pub fn phone_country(phone: &str) -> Option<&'static str> {
    let t = phone.trim();
    let rest = t.strip_prefix('+').or_else(|| t.strip_prefix("00"))?;
    let digits: String = rest.chars().filter(char::is_ascii_digit).collect();
    COUNTRIES
        .iter()
        .filter(|(_, code, _)| digits.starts_with(code))
        .max_by_key(|(iso, code, _)| (code.len(), *iso == "US"))
        .map(|(iso, _, _)| *iso)
}

/// The phone's "+" code against the company's country.
pub fn phone_country_card(phone: &str, country: Option<&str>) -> Option<VendorCard> {
    let from = phone_country(phone)?;
    let company = country.and_then(country_code)?;
    let same = from == company || (from == "US" && company == "CA");
    Some(if same {
        card(
            "phone_country",
            "Phone country",
            "Matches",
            "good",
            format!(
                "The phone number is from {}, like the company.",
                country_name(from)
            ),
        )
    } else {
        card(
            "phone_country",
            "Phone country",
            "Another country",
            "caution",
            format!(
                "The phone number is from {}, but the company is in {}. Ask why.",
                country_name(from),
                country_name(company)
            ),
        )
    })
}

// ───────────────────────── address ─────────────────────────

/// Words of a mailbox or rented address, not a place where a company
/// works.
const MAILBOX_WORDS: &[&str] = &[
    "po box",
    "p.o. box",
    "p o box",
    "pobox",
    "post office box",
    "caixa postal",
    "postfach",
    "virtual office",
    "escritório virtual",
    "escritorio virtual",
    "mailbox",
    "mail drop",
    "regus",
    "registered agent",
];

pub fn is_mailbox_address(address: &str) -> bool {
    let lower = address.to_lowercase();
    MAILBOX_WORDS.iter().any(|w| lower.contains(w))
}

/// What the map search found: its country and the address it shows.
#[derive(Debug, Clone, PartialEq)]
pub struct MapAnswer {
    pub country: Option<String>,
    pub shown: String,
}

/// The address cards: a mailbox, and what the map search found (None
/// when the search did not answer).
pub fn address_cards(
    address: &str,
    country: Option<&str>,
    map: Option<Option<MapAnswer>>,
) -> Vec<VendorCard> {
    let mut cards = Vec::new();
    if is_mailbox_address(address) {
        cards.push(card(
            "address_mailbox",
            "Address type",
            "Mailbox",
            "caution",
            "The address looks like a PO box or a virtual office, not a place where the company works. Ask for the real address.".to_string(),
        ));
    }
    let company = country.and_then(country_code);
    cards.push(match map {
        None => card(
            "address_map",
            "Address on the map",
            "Not checked",
            "info",
            "The map search did not answer. Try again later.".to_string(),
        ),
        Some(None) => card(
            "address_map",
            "Address on the map",
            "Not found",
            "info",
            "OpenStreetMap could not find this address. Check it yourself: it may be new, or written in an unusual way.".to_string(),
        ),
        Some(Some(found)) => {
            let other = match (found.country.as_deref(), company) {
                (Some(f), Some(c)) if !f.eq_ignore_ascii_case(c) => Some(f.to_uppercase()),
                _ => None,
            };
            match other {
                Some(f) => card(
                    "address_map",
                    "Address on the map",
                    "Another country",
                    "caution",
                    format!(
                        "The address is in {}, but the company says it is in {}. Map: {}",
                        country_name(&f),
                        country_name(company.unwrap_or("")),
                        found.shown
                    ),
                ),
                None => card(
                    "address_map",
                    "Address on the map",
                    "Found",
                    "good",
                    format!("OpenStreetMap found it: {}", found.shown),
                ),
            }
        }
    });
    cards
}

/// Nominatim allows 1 search a second: the time of the last one.
static LAST_MAP_SEARCH: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));

/// Searches the address on OpenStreetMap (Nominatim, free, no key).
/// Some(None) = not found; None = the search did not answer.
pub async fn map_search(address: &str) -> Option<Option<MapAnswer>> {
    {
        let mut last = LAST_MAP_SEARCH.lock().await;
        if let Some(at) = *last {
            let wait = Duration::from_millis(1100).saturating_sub(at.elapsed());
            tokio::time::sleep(wait).await;
        }
        *last = Some(Instant::now());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("Safely vendor check (+https://safely.sh)")
        .build()
        .ok()?;
    let url = format!(
        "https://nominatim.openstreetmap.org/search?format=jsonv2&limit=1&addressdetails=1&q={}",
        urlencoding::encode(address)
    );
    let answer: Value = client.get(url).send().await.ok()?.json().await.ok()?;
    let first = answer.as_array()?.first().cloned();
    Some(first.map(|place| {
        MapAnswer {
            country: place["address"]["country_code"]
                .as_str()
                .map(str::to_uppercase),
            shown: place["display_name"].as_str().unwrap_or("").to_string(),
        }
    }))
}

// ───────────────────────── website ─────────────────────────

/// Words of a parked domain or one for sale.
const PARKED_WORDS: &[&str] = &[
    "domain is for sale",
    "this domain is for sale",
    "buy this domain",
    "domain may be for sale",
    "parked free",
    "parkingcrew",
    "sedoparking",
    "domínio está à venda",
    "este domínio está à venda",
];

/// The page's text without tags, scripts or styles, lower case.
pub fn page_text(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    let lower_tags = ["<script", "<style"];
    loop {
        let next = lower_tags
            .iter()
            .filter_map(|t| rest.to_lowercase().find(t).map(|i| (i, *t)))
            .min();
        match next {
            Some((i, tag)) => {
                out.push_str(&rest[..i]);
                let close = if tag == "<script" {
                    "</script>"
                } else {
                    "</style>"
                };
                match rest[i..].to_lowercase().find(close) {
                    Some(j) => rest = &rest[i + j + close.len()..],
                    None => break,
                }
            }
            None => {
                out.push_str(rest);
                break;
            }
        }
    }
    let mut text = String::new();
    let mut in_tag = false;
    for ch in out.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    fold_accents(&text.replace("&amp;", "&")).to_lowercase()
}

/// The website card from its home page (None = it did not answer).
pub fn website_card(site: &str, company: &str, html: Option<&str>) -> VendorCard {
    let Some(html) = html else {
        return card(
            "website_content",
            "Website content",
            "Not checked",
            "info",
            format!("{site} did not answer, so its pages were not read."),
        );
    };
    let text = page_text(html);
    if PARKED_WORDS.iter().any(|w| text.contains(w)) {
        return card(
            "website_content",
            "Website content",
            "Parked or for sale",
            "bad",
            format!(
                "{site} is an empty, parked page or is for sale. A real company's website shows its business."
            ),
        );
    }
    let shown = sanctions::compared_name(&fold_accents(company))
        .is_some_and(|name| name.split_whitespace().all(|w| text.contains(w)));
    if shown {
        card(
            "website_content",
            "Website content",
            "Names the company",
            "good",
            format!("{site}'s home page shows the company's name."),
        )
    } else {
        card(
            "website_content",
            "Website content",
            "Name not shown",
            "caution",
            format!(
                "{site}'s home page does not show \"{company}\". Check that it is really this company's website."
            ),
        )
    }
}

/// Reads the website's home page (at most 1 MB, 12 seconds).
pub async fn fetch_home_page(site: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36")
        .build()
        .ok()?;
    for scheme in ["https", "http"] {
        if let Ok(r) = client.get(format!("{scheme}://{site}")).send().await
            && r.status().is_success()
            && let Ok(bytes) = r.bytes().await
        {
            let cut = &bytes[..bytes.len().min(1_000_000)];
            return Some(String::from_utf8_lossy(cut).to_string());
        }
    }
    None
}

// ───────────────────────── documents ─────────────────────────

/// The two kinds of file the vendor can upload.
pub const DOCUMENT_KINDS: &[&str] = &["registration", "bank_letter"];
/// The biggest file allowed (5 MB).
pub const DOCUMENT_MAX_BYTES: usize = 5 * 1024 * 1024;

/// The file's real type from its first bytes (not its name): PDF, JPEG,
/// PNG or WebP. None for anything else.
pub fn document_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"%PDF") {
        Some("application/pdf")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"\x89PNG") {
        Some("image/png")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// What Claude read from a document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentReading {
    /// "company registration", "business licence", "bank letter"...
    pub document_type: Option<String>,
    pub company_name: Option<String>,
    pub company_number: Option<String>,
    pub account_holder: Option<String>,
    pub account_number: Option<String>,
    /// Signs of editing (different fonts, pasted text, cut-off edges).
    pub looks_edited: bool,
    pub notes: Option<String>,
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Asks Claude to read a document. None when there is no key or Claude
/// did not answer.
pub async fn read_document(bytes: &[u8], content_type: &str) -> Option<DocumentReading> {
    let key = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())?;
    let source = json!({ "type": "base64", "media_type": content_type, "data": base64(bytes) });
    let file = if content_type == "application/pdf" {
        json!({ "type": "document", "source": source })
    } else {
        json!({ "type": "image", "source": source })
    };
    let ask = "This file was sent by a company to a buyer who is checking it before paying. \
Read it and answer with JSON only, no other text: \
{\"document_type\": string or null (e.g. \"company registration\", \"business licence\", \"bank letter\", \"bank statement\", \"other\"), \
\"company_name\": string or null, \"company_number\": string or null (registration, tax or licence number), \
\"account_holder\": string or null (name on the bank account), \"account_number\": string or null (IBAN or account number), \
\"looks_edited\": true or false (clear signs the file was edited: mismatched fonts, pasted or covered text, cut edges), \
\"notes\": string or null (one short sentence, only if something looks wrong)}. \
Write names and numbers exactly as the document shows them.";
    let body = json!({
        "model": "claude-sonnet-4-6",
        "max_tokens": 600,
        "temperature": 0,
        "messages": [{ "role": "user", "content": [file, { "type": "text", "text": ask }] }],
    });
    let response = reqwest::Client::new()
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .timeout(Duration::from_secs(90))
        .json(&body)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        eprintln!(
            "Safely: Claude could not read a vendor document ({})",
            response.status()
        );
        return None;
    }
    let answer: Value = response.json().await.ok()?;
    let text = answer["content"][0]["text"].as_str()?;
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    serde_json::from_str(&text[start..=end]).ok()
}

fn same_number(a: &str, b: &str) -> bool {
    let (x, y) = (clean_account(a), clean_account(b));
    !x.is_empty()
        && (x == y || (x.len() >= 6 && y.len() >= 6 && (x.ends_with(&y) || y.ends_with(&x))))
}

/// What a document says, against the answers. `reading` None = it could
/// not be read.
pub fn document_cards(
    kind: &str,
    reading: Option<&DocumentReading>,
    company: &str,
    company_number: Option<&str>,
    holder: Option<&str>,
    account: Option<&str>,
) -> Vec<VendorCard> {
    let (key, label) = if kind == "bank_letter" {
        ("doc_bank", "Bank document")
    } else {
        ("doc_registration", "Registration document")
    };
    let Some(r) = reading else {
        return vec![card(
            key,
            label,
            "Not read",
            "info",
            "The file was uploaded but Safely could not read it. Open it and check it yourself."
                .to_string(),
        )];
    };
    let mut problems: Vec<String> = Vec::new();
    let mut good: Vec<String> = Vec::new();
    if kind == "bank_letter" {
        let want = holder.unwrap_or(company);
        match r.account_holder.as_deref().or(r.company_name.as_deref()) {
            Some(h) if names_match(h, want) || names_match(h, company) => {
                good.push(format!("account holder \"{h}\""))
            }
            Some(h) => problems.push(format!("the account holder is \"{h}\", not \"{want}\"")),
            None => {}
        }
        if let (Some(doc), Some(given)) = (r.account_number.as_deref(), account) {
            if same_number(doc, given) {
                good.push("the same account number".to_string());
            } else {
                problems.push(format!(
                    "the account number on it ({doc}) is not the one in the form"
                ));
            }
        }
    } else {
        match r.company_name.as_deref() {
            Some(n) if names_match(n, company) => good.push(format!("company \"{n}\"")),
            Some(n) => problems.push(format!("it is for \"{n}\", not \"{company}\"")),
            None => {}
        }
        if let (Some(doc), Some(given)) = (r.company_number.as_deref(), company_number) {
            if same_number(doc, given) {
                good.push("the same company number".to_string());
            } else {
                problems.push(format!(
                    "the number on it ({doc}) is not the one in the form ({given})"
                ));
            }
        }
    }
    if r.looks_edited {
        problems.push("it shows signs of editing".to_string());
    }
    let what = r
        .document_type
        .clone()
        .unwrap_or_else(|| "document".to_string());
    if !problems.is_empty() {
        let mut detail = format!("The {what} doesn't match: {}.", problems.join("; "));
        if let Some(n) = &r.notes {
            detail.push(' ');
            detail.push_str(n);
        }
        vec![card(key, label, "Doesn't match", "bad", detail)]
    } else if !good.is_empty() {
        vec![card(
            key,
            label,
            "Matches",
            "good",
            format!("The {what} shows {}.", good.join(" and ")),
        )]
    } else {
        vec![card(
            key,
            label,
            "Nothing to compare",
            "info",
            format!(
                "Safely read the {what} but found no name or number to compare. Open it and check it yourself."
            ),
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iban_check_digits() {
        assert!(iban_is_valid(&clean_account("GB82 WEST 1234 5698 7654 32")));
        assert!(iban_is_valid("DE89370400440532013000"));
        assert!(!iban_is_valid("DE89370400440532013001"));
        assert!(!looks_like_iban("123456789"));
    }

    #[test]
    fn bank_cards_compare_countries() {
        let c = bank_cards(
            Some("DE89 3704 0044 0532 0130 00"),
            Some("COBADEFFXXX"),
            Some("Brazil"),
        );
        assert_eq!(c[0].value, "Account abroad");
        assert_eq!(c[1].value, "Bank abroad");
        let c = bank_cards(
            Some("DE89 3704 0044 0532 0130 00"),
            Some("COBADEFF"),
            Some("Germany"),
        );
        assert_eq!(c[0].kind, "good");
        assert_eq!(c[1].kind, "good");
        let c = bank_cards(Some("DE89370400440532013000"), Some("BOFAUS3N"), None);
        assert_eq!(c[1].value, "Doesn't match the IBAN");
        let c = bank_cards(Some("GB00WEST12345698765432"), None, None);
        assert_eq!(c[0].kind, "bad");
        let c = bank_cards(Some("000123456789"), Some("021000021"), Some("USA"));
        assert_eq!(c[0].kind, "info");
        assert_eq!(c[1].value, "Valid");
        assert_eq!(bank_cards(None, Some("021000022"), None)[0].kind, "bad");
    }

    #[test]
    fn phone_country_codes() {
        assert_eq!(phone_country("+55 11 91234-5678"), Some("BR"));
        assert_eq!(phone_country("+852 2345 6789"), Some("HK"));
        assert_eq!(phone_country("+1 312 555 0148"), Some("US"));
        assert_eq!(phone_country("0086 138 0000 0000"), Some("CN"));
        assert_eq!(phone_country("(11) 91234-5678"), None);
        assert_eq!(
            phone_country_card("+86 138 0000 0000", Some("Brasil"))
                .unwrap()
                .kind,
            "caution"
        );
        assert_eq!(
            phone_country_card("+55 11 91234-5678", Some("São Paulo, Brazil"))
                .unwrap()
                .kind,
            "good"
        );
        assert_eq!(
            phone_country_card("+1 416 555 0100", Some("Canada"))
                .unwrap()
                .kind,
            "good"
        );
    }

    #[test]
    fn mailbox_addresses_and_the_map() {
        let c = address_cards("PO Box 1234, Miami", Some("USA"), Some(None));
        assert_eq!(c[0].key, "address_mailbox");
        let found = MapAnswer {
            country: Some("CN".into()),
            shown: "Shenzhen, China".into(),
        };
        let c = address_cards("Rua X 1, São Paulo", Some("Brazil"), Some(Some(found)));
        assert_eq!(c[0].value, "Another country");
        assert_eq!(address_cards("x", None, None)[0].value, "Not checked");
    }

    #[test]
    fn website_shows_the_name_or_is_parked() {
        let html = "<html><head><style>.a{}</style><script>var x='Acme'</script></head><body><h1>ACME Comércio</h1></body></html>";
        assert_eq!(
            website_card("acme.com", "Acme Comercio Ltda", Some(html)).kind,
            "good"
        );
        assert_eq!(
            website_card(
                "acme.com",
                "Acme Comercio Ltda",
                Some("<p>This domain is for sale</p>")
            )
            .kind,
            "bad"
        );
        // The name only inside a script does not count.
        assert_eq!(
            website_card(
                "acme.com",
                "Acme Trading",
                Some("<script>Acme Trading</script><p>Hello</p>")
            )
            .kind,
            "caution"
        );
        assert_eq!(website_card("acme.com", "Acme", None).kind, "info");
    }

    #[test]
    fn document_types_and_base64() {
        assert_eq!(document_type(b"%PDF-1.7"), Some("application/pdf"));
        assert_eq!(document_type(b"<html>"), None);
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(b"fo"), "Zm8=");
    }

    #[test]
    fn documents_are_compared_with_the_answers() {
        let reading = DocumentReading {
            document_type: Some("bank letter".into()),
            account_holder: Some("Carlos Pereira".into()),
            account_number: Some("DE89 3704 0044 0532 0130 00".into()),
            ..DocumentReading::default()
        };
        let c = document_cards(
            "bank_letter",
            Some(&reading),
            "Acme Ltda",
            None,
            Some("Acme Ltda"),
            Some("DE89370400440532013000"),
        );
        assert_eq!(c[0].kind, "bad");
        let reading = DocumentReading {
            company_name: Some("ACME LTDA".into()),
            company_number: Some("12.345.678/0001-95".into()),
            ..DocumentReading::default()
        };
        let c = document_cards(
            "registration",
            Some(&reading),
            "Acme Ltda",
            Some("12345678000195"),
            None,
            None,
        );
        assert_eq!(c[0].kind, "good");
        assert_eq!(
            document_cards("registration", None, "Acme", None, None, None)[0].value,
            "Not read"
        );
    }

    #[test]
    fn made_up_company_numbers() {
        assert!(looks_made_up("123456789"));
        assert!(looks_made_up("000000"));
        assert!(looks_made_up("987654"));
        assert!(looks_made_up("12"));
        assert!(!looks_made_up("36-4829177"));
        assert_eq!(
            company_number_card("36-4829177", Some("Pakistan")).value,
            "Not checked"
        );
    }

    #[test]
    fn a_same_name_company_abroad_is_not_the_vendor() {
        // US "Inc." pages for a vendor in Pakistan.
        assert!(
            other_company_pages(
                "Mirian Solutions",
                Some("Pakistan"),
                &["Mirian Solutions, Inc. Reviews"]
            )
            .is_some()
        );
        // The vendor is in the USA: the same pages are fine.
        assert!(
            other_company_pages(
                "Mirian Solutions",
                Some("USA"),
                &["Mirian Solutions, Inc. Reviews"]
            )
            .is_none()
        );
        // The vendor wrote "Inc." itself.
        assert!(
            other_company_pages(
                "Mirian Solutions Inc",
                Some("Pakistan"),
                &["Mirian Solutions, Inc."]
            )
            .is_none()
        );
        // One page without a foreign form: it may be the vendor.
        assert!(
            other_company_pages(
                "Mirian Solutions",
                Some("Pakistan"),
                &["Mirian Solutions, Inc.", "Mirian Solutions Lahore"]
            )
            .is_none()
        );
    }
}
