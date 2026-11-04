//! Public records about the seller's own website, for every scan that
//! knows it (a website check, or a marketplace supplier whose website
//! was found):
//!
//! - "Website history": since when the Wayback Machine (web.archive.org)
//!   has saved the website.
//! - "Website traffic": whether it is among the world's 1 million most
//!   visited websites (the Tranco list, tranco-list.eu).
//! - "Security certificate": when it got its first certificate (crt.sh,
//!   the public log of certificates).
//!
//! All three are free and need no key. A long record is a good sign; a
//! short one or none is information only, never a warning: many real
//! small companies have little traffic and a young website, and a
//! brand-new domain is already warned about by "Website age".
//!
//! Off unless WEBSITE_RECORDS_CHECK_ENABLED is "true" in .env. Each
//! source is asked once per scan with a short time limit; one that does
//! not answer in time gives no card, and the scan goes on.

use crate::{
    models::analysis::Signal,
    services::{
        features::domain_age::normalize_domain, shared::platform_config::get_all_platform_domains,
        sources::databases::OUTSIDE_RECORDS,
    },
};
use chrono::{NaiveDate, Utc};
use serde::Deserialize;
use std::time::Duration;

/// The cards' names, as in standards/databases/global.json.
pub const HISTORY_LABEL: &str = "Website history";
pub const TRAFFIC_LABEL: &str = "Website traffic";
pub const CERTIFICATE_LABEL: &str = "Security certificate";

pub const HISTORY_OLD: &str = "Online for years";
pub const HISTORY_NEW: &str = "New online";
pub const HISTORY_NONE: &str = "Not in the archive";
pub const TRAFFIC_TOP: &str = "Top 1 million site";
pub const TRAFFIC_NONE: &str = "Not in the top 1 million";
pub const CERTIFICATE_OLD: &str = "Certificate for years";
pub const CERTIFICATE_NEW: &str = "First certificate recent";
pub const CERTIFICATE_NONE: &str = "No certificate found";

/// A record at least this old (in days) is a good sign.
const OLD_ENOUGH_DAYS: i64 = 365;

/// Longest wait for each source. crt.sh is often slow.
const WAYBACK_WAIT: Duration = Duration::from_secs(8);
const TRANCO_WAIT: Duration = Duration::from_secs(5);
const CRTSH_WAIT: Duration = Duration::from_secs(12);

/// Largest crt.sh answer read (a very big website has thousands of
/// certificates; it is old anyway).
const MAX_CRTSH_BYTES: usize = 20_000_000;

pub fn enabled() -> bool {
    std::env::var("WEBSITE_RECORDS_CHECK_ENABLED")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(false)
}

/// The domain to look up ("https://www.shop.com/a" -> "shop.com"), or
/// None when it is not a website address or it is a marketplace Safely
/// reads (a supplier's page on Alibaba is not the supplier's own site).
pub fn site_domain(website: &str) -> Option<String> {
    let domain = normalize_domain(website)?;
    let on_a_marketplace = get_all_platform_domains().values().any(|platform| {
        let platform = platform.trim_start_matches("www.");
        domain == platform || domain.ends_with(&format!(".{platform}"))
    });
    (!on_a_marketplace).then_some(domain)
}

/// The three cards for the seller's website. Nothing when the check is
/// off or there is no website to look up.
pub async fn website_record_cards(website: Option<&str>) -> Vec<Signal> {
    if !enabled() {
        return Vec::new();
    }
    let Some(domain) = website.and_then(site_domain) else {
        return Vec::new();
    };
    let Ok(client) = reqwest::Client::builder()
        .user_agent("Safely website check (safely.sh)")
        .build()
    else {
        return Vec::new();
    };
    let today = Utc::now().date_naive();
    let (history, traffic, certificate) = tokio::join!(
        first_archived(&client, &domain),
        tranco_rank(&client, &domain),
        first_certificate(&client, &domain),
    );
    let mut cards = Vec::new();
    cards.extend(history.map(|first| history_card(&domain, first, today)));
    cards.extend(traffic.map(|rank| traffic_card(&domain, rank)));
    cards.extend(certificate.map(|first| certificate_card(&domain, first, today)));
    cards
}

fn card(label: &str, value: &str, good: bool, sub: String) -> Signal {
    Signal {
        label: label.to_string(),
        sub,
        value: value.to_string(),
        signal_type: if good { "good" } else { "info" }.to_string(),
        category: OUTSIDE_RECORDS.to_string(),
        check_type: "existence".to_string(),
    }
}

/// "3 months", "1 year and 2 months", "9 years".
fn age_in_words(days: i64) -> String {
    let plural = |n: i64, one: &str, many: &str| {
        if n == 1 {
            format!("1 {one}")
        } else {
            format!("{n} {many}")
        }
    };
    if days < 31 {
        return plural(days.max(0), "day", "days");
    }
    let months = days / 30;
    if months < 12 {
        return plural(months, "month", "months");
    }
    plural(days / 365, "year", "years")
}

// ---- Website history (Wayback Machine) ----------------------------

/// The date of the first saved copy: Some(Some(date)), Some(None) when
/// the archive has none, None when the archive did not answer.
async fn first_archived(client: &reqwest::Client, domain: &str) -> Option<Option<NaiveDate>> {
    let answer = client
        .get(format!(
            "https://web.archive.org/cdx/search/cdx?url={}&output=json&limit=1&fl=timestamp&filter=statuscode:200",
            urlencoding::encode(domain)
        ))
        .timeout(WAYBACK_WAIT)
        .send()
        .await
        .ok()?;
    if !answer.status().is_success() {
        eprintln!(
            "Safely: Wayback Machine answered {} for {domain}",
            answer.status()
        );
        return None;
    }
    let text = answer.text().await.ok()?;
    Some(first_capture(&text))
}

/// The first capture's date from the archive's answer: a header row,
/// then one row per capture (`[["timestamp"],["20140512083000"]]`), or
/// an empty answer when nothing is saved.
pub fn first_capture(text: &str) -> Option<NaiveDate> {
    let rows: Vec<Vec<String>> = serde_json::from_str(text.trim()).ok()?;
    let stamp = rows.get(1)?.first()?;
    NaiveDate::parse_from_str(stamp.get(..8)?, "%Y%m%d").ok()
}

pub fn history_card(domain: &str, first: Option<NaiveDate>, today: NaiveDate) -> Signal {
    match first {
        Some(date) => {
            let days = (today - date).num_days();
            let since = format!(
                "The Wayback Machine (web.archive.org) first saved {domain} on {date}, {} ago.",
                age_in_words(days)
            );
            if days >= OLD_ENOUGH_DAYS {
                card(
                    HISTORY_LABEL,
                    HISTORY_OLD,
                    true,
                    format!("{since} The website has a long public record."),
                )
            } else {
                card(
                    HISTORY_LABEL,
                    HISTORY_NEW,
                    false,
                    format!(
                        "{since} A young website is not proof of a scam, but it has no long record yet."
                    ),
                )
            }
        }
        None => card(
            HISTORY_LABEL,
            HISTORY_NONE,
            false,
            format!(
                "The Wayback Machine (web.archive.org) has no saved copy of {domain}. Many small company websites are never saved, so this is not a warning."
            ),
        ),
    }
}

// ---- Website traffic (Tranco list) --------------------------------

#[derive(Deserialize)]
struct TrancoRank {
    date: String,
    rank: u64,
}

#[derive(Deserialize)]
struct TrancoAnswer {
    #[serde(default)]
    ranks: Vec<TrancoRank>,
}

/// The latest rank: Some(Some((rank, list date))), Some(None) when the
/// website is not in the list, None when Tranco did not answer (it
/// allows one question a second).
async fn tranco_rank(client: &reqwest::Client, domain: &str) -> Option<Option<(u64, String)>> {
    let answer = client
        .get(format!(
            "https://tranco-list.eu/api/ranks/domain/{}",
            urlencoding::encode(domain)
        ))
        .timeout(TRANCO_WAIT)
        .send()
        .await
        .ok()?;
    if !answer.status().is_success() {
        eprintln!("Safely: Tranco answered {} for {domain}", answer.status());
        return None;
    }
    let text = answer.text().await.ok()?;
    latest_rank(&text)
}

/// The newest rank in Tranco's answer (`{"ranks":[{"date":"2026-11-02",
/// "rank":1234}]}`). Some(None) for no ranks; None for an answer that
/// can't be read.
pub fn latest_rank(text: &str) -> Option<Option<(u64, String)>> {
    let answer: TrancoAnswer = serde_json::from_str(text).ok()?;
    Some(
        answer
            .ranks
            .into_iter()
            .max_by(|a, b| a.date.cmp(&b.date))
            .map(|r| (r.rank, r.date)),
    )
}

pub fn traffic_card(domain: &str, rank: Option<(u64, String)>) -> Signal {
    match rank {
        Some((rank, date)) => card(
            TRAFFIC_LABEL,
            TRAFFIC_TOP,
            true,
            format!(
                "{domain} is number {rank} in the Tranco list of the world's 1 million most visited websites (list of {date}). Scam shops rarely get this much traffic."
            ),
        ),
        None => card(
            TRAFFIC_LABEL,
            TRAFFIC_NONE,
            false,
            format!(
                "{domain} is not among the world's 1 million most visited websites (Tranco list). That is normal for most small companies and new shops, so it is not a warning."
            ),
        ),
    }
}

// ---- Security certificate (crt.sh) --------------------------------

#[derive(Deserialize)]
struct Certificate {
    not_before: String,
}

/// The date of the first certificate: Some(Some(date)), Some(None) when
/// the log has none, None when crt.sh did not answer in time.
async fn first_certificate(client: &reqwest::Client, domain: &str) -> Option<Option<NaiveDate>> {
    let answer = client
        .get(format!(
            "https://crt.sh/?q={}&output=json",
            urlencoding::encode(domain)
        ))
        .timeout(CRTSH_WAIT)
        .send()
        .await
        .ok()?;
    if !answer.status().is_success() {
        eprintln!("Safely: crt.sh answered {} for {domain}", answer.status());
        return None;
    }
    let bytes = answer.bytes().await.ok()?;
    if bytes.len() > MAX_CRTSH_BYTES {
        return None;
    }
    earliest_certificate(&String::from_utf8_lossy(&bytes))
}

/// The earliest "not_before" date in crt.sh's answer
/// (`[{"not_before":"2016-03-01T00:00:00", ...}]`). Some(None) for no
/// certificates; None for an answer that can't be read.
pub fn earliest_certificate(text: &str) -> Option<Option<NaiveDate>> {
    let certificates: Vec<Certificate> = serde_json::from_str(text.trim()).ok()?;
    Some(
        certificates
            .iter()
            .filter_map(|c| NaiveDate::parse_from_str(c.not_before.get(..10)?, "%Y-%m-%d").ok())
            .min(),
    )
}

pub fn certificate_card(domain: &str, first: Option<NaiveDate>, today: NaiveDate) -> Signal {
    match first {
        Some(date) => {
            let days = (today - date).num_days();
            let since = format!(
                "The first security certificate for {domain} was issued on {date}, {} ago (crt.sh, the public log of certificates).",
                age_in_words(days)
            );
            if days >= OLD_ENOUGH_DAYS {
                card(CERTIFICATE_LABEL, CERTIFICATE_OLD, true, since)
            } else {
                card(
                    CERTIFICATE_LABEL,
                    CERTIFICATE_NEW,
                    false,
                    format!(
                        "{since} A website gets its first certificate when it starts, so this shows the site is new - not that it is a scam."
                    ),
                )
            }
        }
        None => card(
            CERTIFICATE_LABEL,
            CERTIFICATE_NONE,
            false,
            format!(
                "No security certificate for {domain} was found in the public log (crt.sh). Check that the address starts with https:// and the browser shows no warning before you pay."
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn only_the_sellers_own_website_is_looked_up() {
        assert_eq!(
            site_domain("https://www.xtoinc.com/about-us/").as_deref(),
            Some("xtoinc.com")
        );
        assert_eq!(site_domain("gloflux.com").as_deref(), Some("gloflux.com"));
        // A supplier's page on a marketplace is not its own website.
        assert_eq!(site_domain("https://boyue.en.alibaba.com/"), None);
        assert_eq!(site_domain("not a website"), None);
    }

    #[test]
    fn the_first_saved_copy_is_read_from_the_archive() {
        assert_eq!(
            first_capture(r#"[["timestamp"],["20140512083000"]]"#),
            Some(day("2014-05-12"))
        );
        assert_eq!(first_capture("[]"), None);
        assert_eq!(first_capture(""), None);
        let old = history_card("xtoinc.com", Some(day("2014-05-12")), day("2026-11-04"));
        assert_eq!(
            (old.value.as_str(), old.signal_type.as_str()),
            (HISTORY_OLD, "good")
        );
        assert!(
            old.sub
                .contains("first saved xtoinc.com on 2014-05-12, 12 years ago")
        );
        assert_eq!(old.category, OUTSIDE_RECORDS);
        let new = history_card("shop.com", Some(day("2026-08-01")), day("2026-11-04"));
        assert_eq!(
            (new.value.as_str(), new.signal_type.as_str()),
            (HISTORY_NEW, "info")
        );
        assert!(new.sub.contains("3 months ago"));
        let none = history_card("shop.com", None, day("2026-11-04"));
        assert_eq!(
            (none.value.as_str(), none.signal_type.as_str()),
            (HISTORY_NONE, "info")
        );
    }

    #[test]
    fn the_newest_tranco_rank_is_used() {
        let rank = latest_rank(
            r#"{"ranks":[{"date":"2026-11-01","rank":2100},{"date":"2026-11-02","rank":1999}]}"#,
        );
        assert_eq!(rank, Some(Some((1999, "2026-11-02".to_string()))));
        assert_eq!(latest_rank(r#"{"ranks":[]}"#), Some(None));
        assert_eq!(latest_rank(r#"{"domain":"x.com"}"#), Some(None));
        assert_eq!(latest_rank("<html>Too many requests</html>"), None);
        let top = traffic_card("shop.com", Some((1999, "2026-11-02".to_string())));
        assert_eq!(
            (top.value.as_str(), top.signal_type.as_str()),
            (TRAFFIC_TOP, "good")
        );
        assert!(top.sub.contains("number 1999"));
        let none = traffic_card("shop.com", None);
        assert_eq!(
            (none.value.as_str(), none.signal_type.as_str()),
            (TRAFFIC_NONE, "info")
        );
    }

    #[test]
    fn the_first_certificate_is_the_earliest_one() {
        let first = earliest_certificate(
            r#"[{"id":1,"not_before":"2024-03-01T00:00:00","name_value":"shop.com"},{"id":2,"not_before":"2016-07-15T12:00:00"},{"id":3,"not_before":"bad"}]"#,
        );
        assert_eq!(first, Some(Some(day("2016-07-15"))));
        assert_eq!(earliest_certificate("[]"), Some(None));
        assert_eq!(earliest_certificate("<html>busy</html>"), None);
        let old = certificate_card("shop.com", Some(day("2016-07-15")), day("2026-11-04"));
        assert_eq!(
            (old.value.as_str(), old.signal_type.as_str()),
            (CERTIFICATE_OLD, "good")
        );
        let new = certificate_card("shop.com", Some(day("2026-10-20")), day("2026-11-04"));
        assert_eq!(
            (new.value.as_str(), new.signal_type.as_str()),
            (CERTIFICATE_NEW, "info")
        );
        assert!(new.sub.contains("15 days ago"));
        let none = certificate_card("shop.com", None, day("2026-11-04"));
        assert_eq!(none.value, CERTIFICATE_NONE);
    }

    #[test]
    fn these_cards_never_warn() {
        let today = day("2026-11-04");
        for c in [
            history_card("a.com", None, today),
            history_card("a.com", Some(today), today),
            traffic_card("a.com", None),
            certificate_card("a.com", None, today),
            certificate_card("a.com", Some(today), today),
        ] {
            assert_ne!(c.signal_type, "caution", "{}", c.label);
        }
    }

    #[tokio::test]
    async fn the_check_is_off_unless_switched_on() {
        if !enabled() {
            assert!(website_record_cards(Some("xtoinc.com")).await.is_empty());
        }
    }
}
