//! The Domain Age Checker (safely.sh/tools/domain-age-checker).
//!
//! 1. The visitor's text is cleaned into a plain domain:
//!    "https://www.Supplier.com/about" -> "supplier.com".
//! 2. The registration record is read with RDAP, the free and official
//!    successor of WHOIS (rdap.org sends each request to the right
//!    registry). A subdomain that is not found is tried again without
//!    its first part: "shop.supplier.com" -> "supplier.com".
//! 3. Some registries have no RDAP yet. For those, the WhoisJSON lookup
//!    the extension already uses (services/whois.rs) is tried.
//!
//! Answers are kept for 12 hours, so the same domain is never looked up
//! again and again, and each visitor can run 30 checks an hour, so a bot
//! cannot flood the registries from our server.

use crate::models::domain_age::{DomainAge, Verdict};
use crate::services::shared::whois::check_domain_whois;
use chrono::{NaiveDate, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const RDAP_URL: &str = "https://rdap.org/domain/";
const USER_AGENT: &str = "Safely Domain Age Checker (+https://safely.sh/tools/domain-age-checker)";
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(8);

/// Under 6 months is "new", under 2 years "young", older "established".
const NEW_DAYS: i64 = 183;
const YOUNG_DAYS: i64 = 730;
/// "Expires soon" means in the next 3 months.
const EXPIRES_SOON_DAYS: i64 = 92;

const CACHE_FOR: Duration = Duration::from_secs(12 * 60 * 60);
const CACHE_MAX: usize = 5_000;
pub const CHECKS_PER_HOUR: usize = 30;
const LIMIT_WINDOW: Duration = Duration::from_secs(60 * 60);

/// Why a lookup gave no answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupError {
    /// No registry knows this domain.
    NotFound,
    /// The registries could not be reached this time.
    Failed,
}

/// Turns what the visitor typed into a plain domain, or None when it
/// is not a domain at all.
///
/// "https://www.Supplier.com:443/about?x=1" -> "supplier.com"
pub fn normalize_domain(input: &str) -> Option<String> {
    let mut text = input.trim().to_ascii_lowercase();
    if let Some((_, rest)) = text.split_once("://") {
        text = rest.to_string();
    }
    // Cut the path, the query, the #part, a user@ and the :port.
    let host = text.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.rsplit('@').next().unwrap_or_default();
    let host = host.split(':').next().unwrap_or_default();
    let host = host.trim_end_matches('.');
    let host = host.strip_prefix("www.").unwrap_or(host);

    let labels: Vec<&str> = host.split('.').collect();
    let tld = labels.last()?;
    let valid = host.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                && !l.starts_with('-')
                && !l.ends_with('-')
        })
        && tld.len() >= 2
        && (tld.chars().all(|c| c.is_ascii_alphabetic()) || tld.starts_with("xn--"));
    valid.then(|| host.to_string())
}

/// The same domain without its first part, while at least two parts
/// are left: "shop.supplier.com" -> "supplier.com".
pub fn parent_domain(domain: &str) -> Option<String> {
    let (_, rest) = domain.split_once('.')?;
    rest.contains('.').then(|| rest.to_string())
}

/// "2015-03-02T18:20:21Z" or "2015-03-02 18:20:21" -> 2015-03-02
pub fn parse_day(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.get(..10)?, "%Y-%m-%d").ok()
}

pub fn verdict_for(age_days: Option<i64>) -> Verdict {
    match age_days {
        None => Verdict::Unknown,
        Some(d) if d < NEW_DAYS => Verdict::New,
        Some(d) if d < YOUNG_DAYS => Verdict::Young,
        Some(_) => Verdict::Established,
    }
}

/// Builds the answer from the three dates and the registrar.
pub fn build_answer(
    domain: &str,
    registered: Option<NaiveDate>,
    expires: Option<NaiveDate>,
    updated: Option<NaiveDate>,
    registrar: Option<String>,
    today: NaiveDate,
) -> DomainAge {
    let age_days = registered.map(|d| (today - d).num_days().max(0));
    let expires_soon = expires
        .map(|d| (d - today).num_days() <= EXPIRES_SOON_DAYS)
        .unwrap_or(false);
    DomainAge {
        domain: domain.to_string(),
        registered_on: registered.map(|d| d.to_string()),
        expires_on: expires.map(|d| d.to_string()),
        updated_on: updated.map(|d| d.to_string()),
        registrar,
        age_days,
        verdict: verdict_for(age_days),
        expires_soon,
    }
}

/// Reads an RDAP answer (RFC 9083): the dates are in "events", the
/// registrar's name is in the "fn" line of its contact card.
pub fn parse_rdap(body: &str, domain: &str, today: NaiveDate) -> Option<DomainAge> {
    let json: Value = serde_json::from_str(body).ok()?;
    let event = |action: &str| {
        json["events"].as_array().and_then(|events| {
            events
                .iter()
                .find(|e| e["eventAction"].as_str() == Some(action))
                .and_then(|e| e["eventDate"].as_str())
                .and_then(parse_day)
        })
    };
    let registrar = json["entities"].as_array().and_then(|entities| {
        entities
            .iter()
            .find(|e| {
                e["roles"]
                    .as_array()
                    .is_some_and(|roles| roles.iter().any(|r| r.as_str() == Some("registrar")))
            })
            .and_then(|e| e["vcardArray"][1].as_array())
            .and_then(|lines| {
                lines
                    .iter()
                    .find(|line| line[0].as_str() == Some("fn"))
                    .and_then(|line| line[3].as_str())
            })
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
    });
    Some(build_answer(
        domain,
        event("registration"),
        event("expiration"),
        event("last changed"),
        registrar,
        today,
    ))
}

/// Remembers recent answers so the same domain is not looked up again.
pub struct Cache {
    entries: HashMap<String, (Instant, Result<DomainAge, LookupError>)>,
}

impl Cache {
    pub fn new() -> Cache {
        Cache {
            entries: HashMap::new(),
        }
    }

    pub fn get(&self, domain: &str, now: Instant) -> Option<Result<DomainAge, LookupError>> {
        self.entries
            .get(domain)
            .filter(|(saved, _)| now.duration_since(*saved) < CACHE_FOR)
            .map(|(_, answer)| answer.clone())
    }

    /// "Failed" is never kept, so the next visitor tries again.
    pub fn put(&mut self, domain: &str, answer: &Result<DomainAge, LookupError>, now: Instant) {
        if answer == &Err(LookupError::Failed) {
            return;
        }
        if self.entries.len() >= CACHE_MAX {
            self.entries
                .retain(|_, (saved, _)| now.duration_since(*saved) < CACHE_FOR);
            if self.entries.len() >= CACHE_MAX {
                self.entries.clear();
            }
        }
        self.entries
            .insert(domain.to_string(), (now, answer.clone()));
    }
}

impl Default for Cache {
    fn default() -> Self {
        Cache::new()
    }
}

/// Counts each visitor's checks in the last hour.
pub struct RateLimiter {
    per_visitor: usize,
    seen: HashMap<String, Vec<Instant>>,
}

impl RateLimiter {
    pub fn new(per_visitor: usize) -> RateLimiter {
        RateLimiter {
            per_visitor,
            seen: HashMap::new(),
        }
    }

    /// True (and counted) when this visitor may run one more check.
    pub fn allow(&mut self, visitor: &str, now: Instant) -> bool {
        if self.seen.len() > 10_000 {
            self.seen.retain(|_, times| {
                times
                    .last()
                    .is_some_and(|t| now.duration_since(*t) < LIMIT_WINDOW)
            });
        }
        let times = self.seen.entry(visitor.to_string()).or_default();
        times.retain(|t| now.duration_since(*t) < LIMIT_WINDOW);
        if times.len() >= self.per_visitor {
            return false;
        }
        times.push(now);
        true
    }
}

static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(|| Mutex::new(Cache::new()));
static LIMITER: LazyLock<Mutex<RateLimiter>> =
    LazyLock::new(|| Mutex::new(RateLimiter::new(CHECKS_PER_HOUR)));

/// True when this visitor may run one more check this hour.
pub fn allow_visitor(visitor: &str) -> bool {
    LIMITER
        .lock()
        .map(|mut limiter| limiter.allow(visitor, Instant::now()))
        .unwrap_or(true)
}

/// Asks rdap.org about one domain. Ok(None) means "not found".
async fn rdap_lookup(
    client: &reqwest::Client,
    domain: &str,
    today: NaiveDate,
) -> Result<Option<DomainAge>, LookupError> {
    let response = client
        .get(format!("{}{}", RDAP_URL, domain))
        .header("Accept", "application/rdap+json, application/json")
        .send()
        .await
        .map_err(|e| {
            eprintln!("Safely: RDAP lookup for {} failed: {}", domain, e);
            LookupError::Failed
        })?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !status.is_success() {
        eprintln!("Safely: RDAP lookup for {} gave status {}", domain, status);
        return Err(LookupError::Failed);
    }
    let body = response.text().await.map_err(|_| LookupError::Failed)?;
    match parse_rdap(&body, domain, today) {
        Some(answer) => Ok(Some(answer)),
        None => {
            eprintln!("Safely: RDAP answer for {} could not be read", domain);
            Err(LookupError::Failed)
        }
    }
}

/// RDAP first (the domain, then without its first part), then WhoisJSON.
async fn lookup_now(domain: &str) -> Result<DomainAge, LookupError> {
    let today = Utc::now().date_naive();
    let client = reqwest::Client::builder()
        .timeout(LOOKUP_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|_| LookupError::Failed)?;

    let mut failed = false;
    let mut name = Some(domain.to_string());
    while let Some(current) = name {
        match rdap_lookup(&client, &current, today).await {
            Ok(Some(answer)) => return Ok(answer),
            Ok(None) => name = parent_domain(&current),
            Err(_) => {
                failed = true;
                break;
            }
        }
    }

    // A registry without RDAP: try the WHOIS service the extension uses.
    match check_domain_whois(domain).await {
        Some(whois) if whois.registered => Ok(build_answer(
            domain,
            whois.created.as_deref().and_then(parse_day),
            None,
            None,
            whois.registrar,
            today,
        )),
        Some(_) => Err(LookupError::NotFound),
        None if failed => Err(LookupError::Failed),
        None => Err(LookupError::NotFound),
    }
}

/// The answer for a clean domain (from normalize_domain), from the
/// cache when it was looked up in the last 12 hours.
pub async fn lookup(domain: &str) -> Result<DomainAge, LookupError> {
    if let Some(answer) = CACHE
        .lock()
        .ok()
        .and_then(|cache| cache.get(domain, Instant::now()))
    {
        return answer;
    }
    let answer = lookup_now(domain).await;
    if let Ok(mut cache) = CACHE.lock() {
        cache.put(domain, &answer, Instant::now());
    }
    answer
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn cleans_what_people_type() {
        for (typed, clean) in [
            ("supplier.com", "supplier.com"),
            ("  Supplier.COM  ", "supplier.com"),
            ("https://www.supplier.com/about?x=1#top", "supplier.com"),
            ("http://shop.supplier.co.uk:8080/", "shop.supplier.co.uk"),
            ("www.supplier.com.", "supplier.com"),
            ("user@supplier.com", "supplier.com"),
            ("xn--80ak6aa92e.com", "xn--80ak6aa92e.com"),
            ("supplier.xn--p1ai", "supplier.xn--p1ai"),
        ] {
            assert_eq!(normalize_domain(typed).as_deref(), Some(clean), "{typed}");
        }
        for bad in [
            "",
            "supplier",
            "localhost",
            "supplier.c",
            "supplier.123",
            "-bad.com",
            "bad-.com",
            "sup plier.com",
            "sup_plier.com",
            "münchen.de",
            "..com",
            "192.168.1.1",
            &format!("{}.com", "a".repeat(64)),
        ] {
            assert_eq!(normalize_domain(bad), None, "{bad}");
        }
    }

    #[test]
    fn parent_domains() {
        assert_eq!(
            parent_domain("shop.supplier.com").as_deref(),
            Some("supplier.com")
        );
        assert_eq!(
            parent_domain("a.supplier.co.uk").as_deref(),
            Some("supplier.co.uk")
        );
        assert_eq!(parent_domain("supplier.com"), None);
    }

    #[test]
    fn verdicts_and_dates() {
        assert_eq!(verdict_for(None), Verdict::Unknown);
        assert_eq!(verdict_for(Some(10)), Verdict::New);
        assert_eq!(verdict_for(Some(400)), Verdict::Young);
        assert_eq!(verdict_for(Some(4000)), Verdict::Established);
        assert_eq!(parse_day("2015-03-02T18:20:21Z"), Some(day(2015, 3, 2)));
        assert_eq!(parse_day("2015-03-02 18:20:21"), Some(day(2015, 3, 2)));
        assert_eq!(parse_day("02/03/2015"), None);
        assert_eq!(parse_day(""), None);
    }

    const RDAP: &str = r#"{
        "objectClassName": "domain",
        "ldhName": "SUPPLIER.COM",
        "status": ["client transfer prohibited"],
        "events": [
            {"eventAction": "registration", "eventDate": "2026-08-01T10:00:00Z"},
            {"eventAction": "expiration", "eventDate": "2026-12-01T10:00:00Z"},
            {"eventAction": "last changed", "eventDate": "2026-09-15T08:00:00Z"},
            {"eventAction": "last update of RDAP database", "eventDate": "2026-10-18T00:00:00Z"}
        ],
        "entities": [
            {"roles": ["registrant"], "vcardArray": ["vcard", [["fn", {}, "text", "Hidden"]]]},
            {"roles": ["registrar"], "vcardArray": ["vcard", [
                ["version", {}, "text", "4.0"],
                ["fn", {}, "text", " Example Registrar, Inc. "]
            ]]}
        ]
    }"#;

    #[test]
    fn reads_an_rdap_answer() {
        let a = parse_rdap(RDAP, "supplier.com", day(2026, 10, 18)).unwrap();
        assert_eq!(a.registered_on.as_deref(), Some("2026-08-01"));
        assert_eq!(a.expires_on.as_deref(), Some("2026-12-01"));
        assert_eq!(a.updated_on.as_deref(), Some("2026-09-15"));
        assert_eq!(a.registrar.as_deref(), Some("Example Registrar, Inc."));
        assert_eq!(a.age_days, Some(78));
        assert_eq!(a.verdict, Verdict::New);
        assert!(a.expires_soon, "expires in 44 days");
    }

    #[test]
    fn rdap_without_dates_or_registrar() {
        let a = parse_rdap(r#"{"ldhName": "x.br"}"#, "x.br", day(2026, 10, 18)).unwrap();
        assert_eq!(a.verdict, Verdict::Unknown);
        assert_eq!(a.registrar, None);
        assert!(!a.expires_soon);
        assert_eq!(parse_rdap("not json", "x.com", day(2026, 10, 18)), None);
    }

    #[test]
    fn answer_as_json_for_the_page() {
        let a = build_answer(
            "old.com",
            Some(day(2001, 1, 1)),
            Some(day(2030, 1, 1)),
            None,
            None,
            day(2026, 10, 18),
        );
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"verdict\":\"established\""));
        assert!(json.contains("\"registered_on\":\"2001-01-01\""));
        assert!(json.contains("\"expires_soon\":false"));
    }

    #[test]
    fn cache_keeps_answers_but_not_failures() {
        let mut cache = Cache::new();
        let now = Instant::now();
        let found = Ok(build_answer(
            "a.com",
            None,
            None,
            None,
            None,
            day(2026, 1, 1),
        ));
        cache.put("a.com", &found, now);
        cache.put("b.com", &Err(LookupError::NotFound), now);
        cache.put("c.com", &Err(LookupError::Failed), now);
        assert_eq!(cache.get("a.com", now), Some(found));
        assert_eq!(cache.get("b.com", now), Some(Err(LookupError::NotFound)));
        assert_eq!(cache.get("c.com", now), None, "failures are tried again");
        let later = now + CACHE_FOR + Duration::from_secs(1);
        assert_eq!(cache.get("a.com", later), None, "old answers expire");
    }

    #[test]
    fn each_visitor_has_a_limit() {
        let mut limiter = RateLimiter::new(3);
        let now = Instant::now();
        assert!(limiter.allow("1.1.1.1", now));
        assert!(limiter.allow("1.1.1.1", now));
        assert!(limiter.allow("1.1.1.1", now));
        assert!(!limiter.allow("1.1.1.1", now), "4th check in the hour");
        assert!(limiter.allow("2.2.2.2", now), "another visitor is fine");
        let later = now + LIMIT_WINDOW + Duration::from_secs(1);
        assert!(limiter.allow("1.1.1.1", later), "a new hour starts over");
    }
}
