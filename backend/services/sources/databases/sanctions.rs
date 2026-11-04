//! Sanctions lists: is the company on a government "banned" list?
//!
//! Lists read: the US OFAC list of blocked companies (SDN) and its other
//! lists (consolidated, non-SDN), the UN Security Council list, and the
//! EU's consolidated financial sanctions list ("sanctions_us_un" and
//! "sanctions_eu" in standards/databases/global.json).
//!
//! The lists are downloaded into SANCTIONS_DIR (default
//! "data/sanctions") at most once a day, in the background, and kept in
//! memory. A scan never waits for a download: while no list has been
//! downloaded yet, the card is simply not shown.
//!
//! Only companies are compared - never people, ships or planes - and
//! only the WHOLE name, after legal forms ("Co., Ltd.", "LLC") and
//! punctuation are removed. A part of a name never matches.
//!
//! Off unless SANCTIONS_CHECK_ENABLED is "true" in .env, so tests and
//! local runs never download anything.

use crate::{models::analysis::Signal, services::sources::databases::OUTSIDE_RECORDS};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime},
};

/// The card's name, as in standards/databases/global.json.
pub const LABEL: &str = "Sanctions lists";
pub const MATCH_VALUE: &str = "Possible match";
pub const NOT_LISTED_VALUE: &str = "Not on the lists";

/// How old the downloaded lists may get before they are downloaded again.
const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// After a download, the next one waits at least this long, even when a
/// list failed (a list that keeps failing is not asked for on every scan).
const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);

const OFAC: &str = "https://sanctionslistservice.ofac.treas.gov/api/PublicationPreview/exports";

/// The EU's consolidated financial sanctions list (XML, version 1.1), as
/// the EU's own download feed gives it
/// (webgate.ec.europa.eu/fsd/fsf/public/rss). The token is the public
/// one written in that feed, not a key.
const EU: &str = "https://webgate.ec.europa.eu/fsd/fsf/public/files/xmlFullSanctionsList_1_1/content?token=dG9rZW4tMjAxNw";

/// (file saved in SANCTIONS_DIR, where it is downloaded from)
const LIST_FILES: [(&str, &str); 6] = [
    ("ofac_sdn.csv", "SDN.CSV"),
    ("ofac_sdn_alt.csv", "ALT.CSV"),
    ("ofac_cons_prim.csv", "CONS_PRIM.CSV"),
    ("ofac_cons_alt.csv", "CONS_ALT.CSV"),
    (
        "un.xml",
        "https://scsanctions.un.org/resources/xml/en/consolidated.xml",
    ),
    ("eu.xml", EU),
];

const OFAC_SDN_LIST: &str = "US OFAC list of blocked companies (SDN)";
const OFAC_OTHER_LIST: &str = "US OFAC sanctions lists (non-SDN)";
const UN_LIST: &str = "UN Security Council sanctions list";
const EU_LIST: &str = "EU sanctions list";

/// Legal forms left out before names are compared ("Co., Ltd.", "LLC",
/// "S.A.", Russian "OOO"...), so "Acme Trading Co., Ltd." and "ACME
/// TRADING LIMITED" are the same name.
const LEGAL_FORMS: [&str; 40] = [
    "co",
    "company",
    "ltd",
    "limited",
    "llc",
    "inc",
    "incorporated",
    "corp",
    "corporation",
    "plc",
    "gmbh",
    "sa",
    "sarl",
    "sas",
    "ltda",
    "eireli",
    "jsc",
    "ojsc",
    "pjsc",
    "cjsc",
    "llp",
    "lp",
    "pte",
    "pty",
    "bv",
    "nv",
    "ag",
    "srl",
    "spa",
    "fze",
    "fzco",
    "fzc",
    "ooo",
    "oao",
    "zao",
    "pao",
    "tov",
    "kft",
    "sro",
    "the",
];

/// One company on a list.
#[derive(Clone, Debug, PartialEq)]
pub struct ListedCompany {
    /// The name as the list writes it (its main name, even when an
    /// alias matched).
    pub name: String,
    pub list: &'static str,
    /// The sanctions programme ("IRAN", "RUSSIA-EO14024", "DPRK"...).
    pub program: String,
}

/// Every listed company, by its compared name.
#[derive(Default)]
pub struct SanctionsIndex {
    by_name: HashMap<String, Vec<ListedCompany>>,
    /// When the oldest list file was downloaded.
    pub updated: Option<SystemTime>,
    /// The lists read, as the card names them ("US (OFAC)", "UN", "EU").
    pub sources: Vec<&'static str>,
}

impl SanctionsIndex {
    fn add(&mut self, name: &str, company: ListedCompany) {
        let Some(key) = compared_name(name) else {
            return;
        };
        let found = self.by_name.entry(key).or_default();
        if !found.contains(&company) {
            found.push(company);
        }
    }

    /// The listed companies with exactly this name (after legal forms
    /// and punctuation are removed).
    pub fn find(&self, company_name: &str) -> Vec<&ListedCompany> {
        compared_name(company_name)
            .and_then(|key| self.by_name.get(&key))
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

/// The name as it is compared: lower case, no punctuation, no legal
/// forms. None when too little is left to compare safely (under 4
/// letters or digits).
pub fn compared_name(name: &str) -> Option<String> {
    let lower = name.to_lowercase().replace(['.', '\''], "");
    let spaced: String = lower
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let words: Vec<&str> = spaced
        .split_whitespace()
        .filter(|w| !LEGAL_FORMS.contains(w))
        .collect();
    let joined = words.join(" ");
    (joined.chars().filter(|c| c.is_alphanumeric()).count() >= 4).then_some(joined)
}

/// Splits a CSV file into rows (quoted fields, "" inside quotes, and
/// line breaks inside quotes all handled).
fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => row.push(std::mem::take(&mut field)),
            '\n' if !quoted => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            '\r' if !quoted => {}
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// OFAC writes "-0-" for an empty field.
fn ofac_value(field: &str) -> &str {
    let v = field.trim();
    if v == "-0-" { "" } else { v }
}

/// Adds the companies of one OFAC list: the main file (ent_num, name,
/// type, programme...) and its file of other names (ent_num, alt_num,
/// type, name...). People, ships and planes are left out: OFAC gives
/// them a type; companies have none ("-0-").
fn add_ofac(index: &mut SanctionsIndex, main: &str, other_names: &str, list: &'static str) {
    let mut companies: HashMap<String, ListedCompany> = HashMap::new();
    for row in csv_rows(main) {
        if row.len() < 4 || !ofac_value(&row[2]).is_empty() {
            continue;
        }
        let (id, name) = (row[0].trim(), ofac_value(&row[1]));
        if id.is_empty() || name.is_empty() {
            continue;
        }
        let company = ListedCompany {
            name: name.to_string(),
            list,
            program: ofac_value(&row[3]).to_string(),
        };
        index.add(name, company.clone());
        companies.insert(id.to_string(), company);
    }
    for row in csv_rows(other_names) {
        if row.len() < 4 {
            continue;
        }
        if let Some(company) = companies.get(row[0].trim()) {
            let other = ofac_value(&row[3]);
            if !other.is_empty() {
                index.add(other, company.clone());
            }
        }
    }
}

fn xml_text(raw: &str) -> String {
    raw.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .trim()
        .to_string()
}

/// Every value of <tag>...</tag> in `xml`.
fn xml_values<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let mut values = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start + open.len()..];
        let Some(end) = after.find(&close) else {
            break;
        };
        values.push(&after[..end]);
        rest = &after[end + close.len()..];
    }
    values
}

/// Adds the companies (<ENTITY>) of the UN list: the name is in
/// <FIRST_NAME>, other names in <ALIAS_NAME>, the programme in
/// <UN_LIST_TYPE>. People (<INDIVIDUAL>) are left out.
fn add_un(index: &mut SanctionsIndex, xml: &str) {
    for entity in xml_values(xml, "ENTITY") {
        let Some(name) = xml_values(entity, "FIRST_NAME")
            .first()
            .map(|n| xml_text(n))
        else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let company = ListedCompany {
            name: name.clone(),
            list: UN_LIST,
            program: xml_values(entity, "UN_LIST_TYPE")
                .first()
                .map(|p| xml_text(p))
                .unwrap_or_default(),
        };
        index.add(&name, company.clone());
        for alias in xml_values(entity, "ALIAS_NAME") {
            index.add(&xml_text(alias), company.clone());
        }
    }
}

/// The value of `attribute="..."` inside one tag's text, or "".
fn xml_attribute<'a>(tag: &'a str, attribute: &str) -> &'a str {
    let key = format!(" {attribute}=\"");
    tag.find(&key)
        .map(|start| &tag[start + key.len()..])
        .and_then(|rest| rest.find('"').map(|end| &rest[..end]))
        .unwrap_or("")
}

/// The text of every <tag ...> opening tag (up to its ">") in `xml`.
fn xml_tags<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}");
    let mut tags = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start + open.len()..];
        // "<nameAliasX" is another tag.
        if !after.starts_with([' ', '>', '/', '\n', '\r', '\t']) {
            rest = after;
            continue;
        }
        let end = after.find('>').unwrap_or(after.len());
        tags.push(&after[..end]);
        rest = &after[end..];
    }
    tags
}

/// Adds the companies of the EU list. Each <sanctionEntity> is one
/// listed person or company; <subjectType code="enterprise"> marks a
/// company (people are "person"). Its names are the wholeName of its
/// <nameAlias> tags (the first is the main name), its programme the
/// programme of its <regulation> ("RUS", "IRN", "DPRK"...).
fn add_eu(index: &mut SanctionsIndex, xml: &str) {
    let mut rest = xml;
    while let Some(start) = rest.find("<sanctionEntity") {
        let after = &rest[start..];
        let end = after
            .find("</sanctionEntity>")
            .map(|e| e + "</sanctionEntity>".len())
            .unwrap_or(after.len());
        let entity = &after[..end];
        rest = &after[end..];
        let is_company = xml_tags(entity, "subjectType")
            .iter()
            .any(|t| xml_attribute(t, "code") == "enterprise");
        if !is_company {
            continue;
        }
        let names: Vec<String> = xml_tags(entity, "nameAlias")
            .iter()
            .map(|t| xml_text(xml_attribute(t, "wholeName")))
            .filter(|n| !n.is_empty())
            .collect();
        let Some(main) = names.first() else {
            continue;
        };
        let company = ListedCompany {
            name: main.clone(),
            list: EU_LIST,
            program: xml_tags(entity, "regulation")
                .iter()
                .map(|t| xml_attribute(t, "programme"))
                .find(|p| !p.is_empty())
                .unwrap_or("")
                .to_string(),
        };
        for name in &names {
            index.add(name, company.clone());
        }
    }
}

/// Reads every downloaded list in `dir`. None when none is there yet.
fn load(dir: &Path) -> Option<SanctionsIndex> {
    let read = |file: &str| std::fs::read_to_string(dir.join(file)).ok();
    let mut index = SanctionsIndex::default();
    let mut found = false;
    for (main, other, list) in [
        ("ofac_sdn.csv", "ofac_sdn_alt.csv", OFAC_SDN_LIST),
        ("ofac_cons_prim.csv", "ofac_cons_alt.csv", OFAC_OTHER_LIST),
    ] {
        if let Some(main) = read(main) {
            add_ofac(&mut index, &main, &read(other).unwrap_or_default(), list);
            found = true;
            if !index.sources.contains(&"US (OFAC)") {
                index.sources.push("US (OFAC)");
            }
        }
    }
    if let Some(xml) = read("un.xml") {
        add_un(&mut index, &xml);
        found = true;
        index.sources.push("UN");
    }
    if let Some(xml) = read("eu.xml") {
        let before = index.len();
        add_eu(&mut index, &xml);
        // Logged so the first real download shows the EU file was read
        // (0 means the EU changed its file's layout).
        eprintln!(
            "Safely: EU sanctions list read ({} company names)",
            index.len() - before
        );
        if index.len() > before {
            found = true;
            index.sources.push("EU");
        }
    }
    index.updated = LIST_FILES
        .iter()
        .filter_map(|(file, _)| std::fs::metadata(dir.join(file)).ok()?.modified().ok())
        .min();
    (found && !index.is_empty()).then_some(index)
}

/// true when every list file is there and less than a day old.
fn lists_are_fresh(dir: &Path) -> bool {
    LIST_FILES.iter().all(|(file, _)| {
        std::fs::metadata(dir.join(file))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age < MAX_AGE)
    })
}

fn list_dir() -> PathBuf {
    std::env::var("SANCTIONS_DIR")
        .ok()
        .filter(|d| !d.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data/sanctions"))
}

fn enabled() -> bool {
    std::env::var("SANCTIONS_CHECK_ENABLED")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(false)
}

static INDEX: RwLock<Option<Arc<SanctionsIndex>>> = RwLock::new(None);
static DOWNLOADING: AtomicBool = AtomicBool::new(false);
/// When the last download started (see RETRY_AFTER).
static LAST_DOWNLOAD: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// true when no download started in the last RETRY_AFTER; marks now.
fn may_download_now() -> bool {
    let Ok(mut last) = LAST_DOWNLOAD.lock() else {
        return false;
    };
    if last.is_some_and(|t| t.elapsed() < RETRY_AFTER) {
        return false;
    }
    *last = Some(std::time::Instant::now());
    true
}

/// Downloads every list again. A list that fails keeps its last copy.
async fn download(dir: PathBuf) {
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        eprintln!(
            "Safely: sanctions lists: cannot create {}: {}",
            dir.display(),
            e
        );
        return;
    }
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("Safely sanctions check (safely.sh)")
        // The EU's download page sets a cookie and sends the browser to
        // the same address again; without cookies that never ends.
        .cookie_store(true)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Safely: sanctions lists: {}", e);
            return;
        }
    };
    for (file, source) in LIST_FILES {
        let url = if source.starts_with("https://") {
            source.to_string()
        } else {
            format!("{OFAC}/{source}")
        };
        let body = match client.get(&url).send().await {
            Ok(r) if r.status().is_success() => r.bytes().await.ok(),
            Ok(r) => {
                eprintln!("Safely: sanctions list {} answered {}", url, r.status());
                None
            }
            Err(e) => {
                eprintln!(
                    "Safely: sanctions list {} could not be downloaded: {}",
                    url, e
                );
                None
            }
        };
        // A real list is large; a tiny answer is an error page.
        let Some(body) = body.filter(|b| b.len() > 10_000) else {
            continue;
        };
        let part = dir.join(format!("{file}.part"));
        if tokio::fs::write(&part, &body).await.is_ok() {
            let _ = tokio::fs::rename(&part, dir.join(file)).await;
        }
    }
    let loaded = tokio::task::spawn_blocking(move || load(&dir))
        .await
        .ok()
        .flatten();
    if let Some(index) = loaded {
        eprintln!(
            "Safely: sanctions lists loaded ({} company names)",
            index.len()
        );
        if let Ok(mut current) = INDEX.write() {
            *current = Some(Arc::new(index));
        }
    }
}

/// The lists in memory, read from disk the first time. Starts a
/// background download when they are missing or older than a day. Never
/// waits for the download.
async fn current_index() -> Option<Arc<SanctionsIndex>> {
    let dir = list_dir();
    let in_memory = INDEX.read().ok().and_then(|i| i.clone());
    let index = match in_memory {
        Some(index) => Some(index),
        None => {
            let from_disk = {
                let dir = dir.clone();
                tokio::task::spawn_blocking(move || load(&dir))
                    .await
                    .ok()
                    .flatten()
            };
            from_disk.map(|index| {
                let index = Arc::new(index);
                if let Ok(mut current) = INDEX.write() {
                    *current = Some(index.clone());
                }
                index
            })
        }
    };
    let fresh = {
        let dir = dir.clone();
        tokio::task::spawn_blocking(move || lists_are_fresh(&dir))
            .await
            .unwrap_or(false)
    };
    if !fresh && !DOWNLOADING.swap(true, Ordering::SeqCst) && may_download_now() {
        tokio::spawn(async move {
            download(dir).await;
            DOWNLOADING.store(false, Ordering::SeqCst);
        });
    }
    index
}

/// The "Sanctions lists" card for a company, or None when there is
/// nothing to check: the check is off, there is no usable name, or no
/// list has been downloaded yet.
pub async fn sanctions_signal(company_name: Option<&str>) -> Option<Signal> {
    if !enabled() {
        return None;
    }
    let name = company_name?.trim();
    compared_name(name)?;
    let Some(index) = current_index().await else {
        eprintln!("Safely: sanctions lists not downloaded yet - sanctions card skipped");
        return None;
    };
    let matches = index.find(name);
    Some(sanctions_card(
        name,
        &matches,
        index.updated,
        &index.sources,
    ))
}

/// The card itself. A match is a warning; no match is a good sign.
pub fn sanctions_card(
    company_name: &str,
    matches: &[&ListedCompany],
    updated: Option<SystemTime>,
    sources: &[&str],
) -> Signal {
    let date = updated.map(|t| {
        chrono::DateTime::<chrono::Utc>::from(t)
            .format("%Y-%m-%d")
            .to_string()
    });
    let (value, signal_type, sub) = match matches.first() {
        Some(first) => {
            let mut lists: Vec<String> = Vec::new();
            for m in matches {
                let entry = if m.program.is_empty() {
                    m.list.to_string()
                } else {
                    format!("{} ({})", m.list, m.program)
                };
                if !lists.contains(&entry) {
                    lists.push(entry);
                }
            }
            (
                MATCH_VALUE,
                "caution",
                format!(
                    "A company named \"{}\" is on the {}. It may be a different company with the same name, but do not pay or ship anything until you have checked it on the official list ({}). Paying a sanctioned company can be against the law.",
                    first.name,
                    lists.join(", "),
                    official_search(first.list),
                ),
            )
        }
        None => (
            NOT_LISTED_VALUE,
            "good",
            format!(
                "\"{}\" is not on the {} sanctions lists{}. This only shows the company is not banned - not that it is real or safe.",
                company_name,
                lists_in_words(sources),
                date.map(|d| format!(" (lists of {d})")).unwrap_or_default()
            ),
        ),
    };
    Signal {
        label: LABEL.to_string(),
        sub,
        value: value.to_string(),
        signal_type: signal_type.to_string(),
        category: OUTSIDE_RECORDS.to_string(),
        check_type: "existence".to_string(),
    }
}

/// Where a buyer can check a match on the official list.
fn official_search(list: &str) -> &'static str {
    match list {
        EU_LIST => "the EU Sanctions Map, sanctionsmap.eu",
        UN_LIST => "the UN Security Council Consolidated List, un.org",
        _ => "sanctionssearch.ofac.treas.gov",
    }
}

/// "US (OFAC) or UN", "US (OFAC), UN or EU".
fn lists_in_words(sources: &[&str]) -> String {
    let sources: Vec<&str> = if sources.is_empty() {
        vec!["US (OFAC)", "UN"]
    } else {
        sources.to_vec()
    };
    match sources.split_last() {
        Some((last, [])) => last.to_string(),
        Some((last, rest)) => format!("{} or {}", rest.join(", "), last),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from the real OFAC and UN files (made-up companies).
    const SDN: &str = "36,\"AEROCARIBBEAN AIRLINES\",-0- ,\"CUBA\",-0- ,-0- ,-0- ,-0- ,-0- ,-0- ,-0- ,-0- \r\n\
173,\"ANGLO-CARIBBEAN CO., LTD.\",-0- ,\"CUBA\",-0- ,-0- ,-0- ,-0- ,-0- ,-0- ,-0- ,-0- \r\n\
306,\"BANCO NACIONAL DE CUBA\",-0- ,\"CUBA\",-0- ,-0- ,-0- ,-0- ,-0- ,-0- ,-0- ,\"a.k.a. \"\"BNC\"\".\"\r\n\
2674,\"ABBAS, Abu\",\"individual\",\"SDGT\",-0- ,-0- ,-0- ,-0- ,-0- ,-0- ,-0- ,-0- \r\n\
9999,\"SEA STAR\",\"vessel\",\"IRAN\",-0- ,-0- ,-0- ,-0- ,-0- ,-0- ,-0- ,-0- \r\n\x1A";
    const SDN_ALT: &str = "306,220,\"aka\",\"NATIONAL BANK OF CUBA\",-0- \r\n\
2674,221,\"aka\",\"ABU ABBAS TRADING\",-0- \r\n";
    const UN: &str = r#"<CONSOLIDATED_LIST><INDIVIDUALS><INDIVIDUAL><FIRST_NAME>JOHN</FIRST_NAME></INDIVIDUAL></INDIVIDUALS><ENTITIES><ENTITY><DATAID>1</DATAID><FIRST_NAME>KOREA MINING DEVELOPMENT TRADING CORPORATION</FIRST_NAME><UN_LIST_TYPE>DPRK</UN_LIST_TYPE><ENTITY_ALIAS><QUALITY>Good</QUALITY><ALIAS_NAME>CHANGGWANG SINYONG CORPORATION</ALIAS_NAME></ENTITY_ALIAS></ENTITY><ENTITY><DATAID>2</DATAID><FIRST_NAME>AL &amp; NOOR EXCHANGE</FIRST_NAME><UN_LIST_TYPE>Al-Qaida</UN_LIST_TYPE></ENTITY></ENTITIES></CONSOLIDATED_LIST>"#;

    // The EU file's layout (XML version 1.1), trimmed; made-up records.
    const EU_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<export xmlns="http://eu.europa.ec/fpi/fsd/export" generationDate="2026-11-03T10:00:00.000+01:00">
<sanctionEntity designationDetails="" unitedNationId="" euReferenceNumber="EU.1.1" logicalId="101">
<regulation regulationType="regulation" programme="RUS" logicalId="1"/>
<subjectType code="enterprise" classificationCode="E"/>
<nameAlias firstName="" middleName="" lastName="" wholeName="Rosoboronexport OJSC" function="" gender="" title="" nameLanguage="" strong="true" logicalId="11"/>
<nameAlias wholeName="ROE" strong="false" logicalId="12"/>
<nameAlias wholeName="Russian Defence Export &amp; Import Company" logicalId="13"/>
</sanctionEntity>
<sanctionEntity euReferenceNumber="EU.2.2" logicalId="102">
<regulation programme="IRN" logicalId="2"/>
<subjectType code="person" classificationCode="P"/>
<nameAlias firstName="Ali" lastName="Example" wholeName="Ali Example Trading" logicalId="21"/>
</sanctionEntity>
</export>"#;

    fn index() -> SanctionsIndex {
        let mut i = SanctionsIndex::default();
        add_ofac(&mut i, SDN, SDN_ALT, OFAC_SDN_LIST);
        add_un(&mut i, UN);
        add_eu(&mut i, EU_XML);
        i
    }

    #[test]
    fn eu_companies_and_their_other_names_are_read_people_are_not() {
        let i = index();
        let m = i.find("Rosoboronexport");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].name, "Rosoboronexport OJSC");
        assert_eq!(m[0].list, EU_LIST);
        assert_eq!(m[0].program, "RUS");
        assert_eq!(
            i.find("Russian Defence Export & Import Company")[0].name,
            "Rosoboronexport OJSC"
        );
        // "ROE" is too short to compare safely; people never match.
        assert!(i.find("ROE").is_empty());
        assert!(i.find("Ali Example Trading").is_empty());
    }

    #[test]
    fn an_eu_match_points_to_the_eu_list_and_the_lists_read_are_named() {
        let i = index();
        let m = i.find("Rosoboronexport");
        let card = sanctions_card("Rosoboronexport", &m, None, &["US (OFAC)", "UN", "EU"]);
        assert_eq!(card.value, MATCH_VALUE);
        assert!(card.sub.contains("is on the EU sanctions list (RUS)"));
        assert!(card.sub.contains("sanctionsmap.eu"));
        let clean = sanctions_card("Acme Widgets", &[], None, &["US (OFAC)", "UN", "EU"]);
        assert!(
            clean
                .sub
                .contains("is not on the US (OFAC), UN or EU sanctions lists")
        );
        assert_eq!(lists_in_words(&["UN"]), "UN");
    }

    #[test]
    fn a_failing_list_is_not_asked_for_again_within_the_hour() {
        assert!(may_download_now());
        assert!(!may_download_now());
    }

    #[test]
    fn names_are_compared_without_legal_forms_and_punctuation() {
        assert_eq!(
            compared_name("Anglo-Caribbean Co., Ltd.").as_deref(),
            Some("anglo caribbean")
        );
        assert_eq!(
            compared_name("ANGLO CARIBBEAN LIMITED"),
            compared_name("Anglo-Caribbean Co., Ltd.")
        );
        assert_eq!(compared_name("Acme S.A.").as_deref(), Some("acme"));
        assert_eq!(compared_name("OOO Romashka").as_deref(), Some("romashka"));
        // Too little left to compare safely.
        assert_eq!(compared_name("AB Co., Ltd."), None);
        assert_eq!(compared_name("Co., Ltd."), None);
        assert_eq!(compared_name(""), None);
    }

    #[test]
    fn csv_handles_quotes_commas_and_line_breaks() {
        let rows = csv_rows("1,\"A, B\",\"say \"\"hi\"\"\"\r\n2,\"two\nlines\",x\n");
        assert_eq!(rows[0], ["1", "A, B", "say \"hi\""]);
        assert_eq!(rows[1], ["2", "two\nlines", "x"]);
    }

    #[test]
    fn only_listed_companies_match_whole_names() {
        let i = index();
        let m = i.find("Anglo Caribbean Company Limited");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].name, "ANGLO-CARIBBEAN CO., LTD.");
        assert_eq!(m[0].program, "CUBA");
        // Another name of a listed company points to its main name.
        assert_eq!(
            i.find("National Bank of Cuba")[0].name,
            "BANCO NACIONAL DE CUBA"
        );
        // UN companies and their other names; &amp; is read as &.
        assert_eq!(i.find("Changgwang Sinyong Corp.")[0].list, UN_LIST);
        assert_eq!(i.find("Al & Noor Exchange")[0].program, "Al-Qaida");
        // People, ships and their other names are never compared.
        assert!(i.find("Abbas, Abu").is_empty());
        assert!(i.find("Abu Abbas Trading").is_empty());
        assert!(i.find("Sea Star").is_empty());
        assert!(i.find("John").is_empty());
        // A part of a name never matches.
        assert!(i.find("Anglo Caribbean Shipping").is_empty());
        assert!(i.find("Caribbean").is_empty());
        assert!(i.find("Guangdong Boyue Trading Co., Ltd.").is_empty());
    }

    #[test]
    fn a_match_is_a_warning_and_no_match_a_good_sign() {
        let i = index();
        let m = i.find("Anglo-Caribbean Co Ltd");
        let card = sanctions_card("Anglo-Caribbean Co Ltd", &m, None, &["US (OFAC)", "UN"]);
        assert_eq!(card.label, LABEL);
        assert_eq!(card.value, MATCH_VALUE);
        assert_eq!(card.signal_type, "caution");
        assert_eq!(card.category, OUTSIDE_RECORDS);
        assert!(card.sub.contains(
            "A company named \"ANGLO-CARIBBEAN CO., LTD.\" is on the US OFAC list of blocked companies (SDN) (CUBA)."
        ));

        let card = sanctions_card(
            "Guangdong Boyue Trading Co., Ltd.",
            &[],
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_793_448_000)),
            &["US (OFAC)", "UN"],
        );
        assert_eq!(card.value, NOT_LISTED_VALUE);
        assert_eq!(card.signal_type, "good");
        assert!(
            card.sub
                .contains("is not on the US (OFAC) or UN sanctions lists (lists of 2026-10-31).")
        );
    }

    #[test]
    fn lists_load_from_the_folder_and_are_fresh_only_when_all_are_there() {
        let dir =
            std::env::temp_dir().join(format!("safely-sanctions-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(load(&dir).is_none());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ofac_sdn.csv"), SDN).unwrap();
        std::fs::write(dir.join("ofac_sdn_alt.csv"), SDN_ALT).unwrap();
        std::fs::write(dir.join("un.xml"), UN).unwrap();
        let i = load(&dir).unwrap();
        assert_eq!(i.find("Banco Nacional de Cuba").len(), 1);
        assert_eq!(i.find("Korea Mining Development Trading").len(), 1);
        assert!(i.updated.is_some());
        assert_eq!(i.sources, ["US (OFAC)", "UN"]);
        // Three files are still missing.
        assert!(!lists_are_fresh(&dir));
        std::fs::write(dir.join("ofac_cons_prim.csv"), "").unwrap();
        std::fs::write(dir.join("ofac_cons_alt.csv"), "").unwrap();
        assert!(!lists_are_fresh(&dir));
        std::fs::write(dir.join("eu.xml"), EU_XML).unwrap();
        assert!(lists_are_fresh(&dir));
        let i = load(&dir).unwrap();
        assert_eq!(i.sources, ["US (OFAC)", "UN", "EU"]);
        assert_eq!(i.find("Rosoboronexport")[0].list, EU_LIST);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_check_is_off_unless_switched_on() {
        // SANCTIONS_CHECK_ENABLED is not set in tests: nothing is
        // downloaded and no card is made.
        if !enabled() {
            assert!(
                sanctions_signal(Some("Banco Nacional de Cuba"))
                    .await
                    .is_none()
            );
        }
    }
}
