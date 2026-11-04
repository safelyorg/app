//! Chambers of commerce: is the seller a member? Being a member is a
//! small good sign (the chamber knows the company). Not being a member
//! says nothing, so then no card is shown.
//!
//! Two kinds of chamber can be searched, both through one Google search
//! (Serper, SERPER_API_KEY) that finds the member's own page, which is
//! then opened to read the exact name:
//! - ChamberMaster / GrowthZone: the directory software of thousands of
//!   local chambers (USA, Canada, UK...). Every member page is
//!   ".../list/member/<name>-<number>". Asked for US sellers and sellers
//!   whose country Safely has no file for.
//! - HKGCC, the Hong Kong General Chamber of Commerce: member pages are
//!   chamber.org.hk/en/membership/directory_detail.aspx?...&id=<number>,
//!   open to everyone. Asked for Hong Kong and China sellers.
//!
//! China's CCPIT and Brazil's ACSP have no public member list; every
//! French company belongs to its CCI by law, so a CCI says nothing more
//! than the register.
//!
//! A member counts only when its WHOLE name is the seller's name (legal
//! forms and punctuation left out). When both the seller and the member
//! page give a website and they differ, it may be another company with
//! the same name: information only.
//!
//! Off unless CHAMBERS_CHECK_ENABLED is "true" in .env (each check is
//! one Serper search).

use crate::{
    models::analysis::Signal,
    services::{
        features::{domain_age::normalize_domain, osint::run_serper_search},
        shared::standards::databases_for_country,
        sources::databases::{OUTSIDE_RECORDS, sanctions::compared_name},
    },
};
use regex::Regex;
use scraper::{Html, Selector};
use std::{sync::LazyLock, time::Duration};

/// The card's name, as in standards/databases/global.json.
pub const LABEL: &str = "Chamber of commerce";
pub const MEMBER: &str = "Chamber member";
pub const OTHER_WEBSITE: &str = "Same name, other website";

/// Most member pages opened for one seller.
const MAX_PAGES: usize = 3;

/// Which chamber directory to search.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Directory {
    ChamberMaster,
    Hkgcc,
}

/// The directory for a seller's country, or None (no chamber to search).
pub fn directory_for(country: Option<&str>) -> Option<Directory> {
    match country
        .and_then(databases_for_country)
        .map(|c| c.country.as_str())
    {
        Some("hong_kong") | Some("china") => Some(Directory::Hkgcc),
        Some("usa") | None => Some(Directory::ChamberMaster),
        Some(_) => None,
    }
}

/// The Google search that finds the member's page.
pub fn search_query(directory: Directory, company_name: &str) -> String {
    let name = company_name.replace('"', " ");
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    match directory {
        Directory::ChamberMaster => format!("inurl:\"/list/member/\" \"{name}\""),
        Directory::Hkgcc => format!("site:chamber.org.hk inurl:directory_detail \"{name}\""),
    }
}

/// true when `link` is a member page of that directory.
pub fn is_member_page(directory: Directory, link: &str) -> bool {
    let lower = link.to_ascii_lowercase();
    match directory {
        Directory::ChamberMaster => lower.starts_with("http") && lower.contains("/list/member/"),
        Directory::Hkgcc => {
            normalize_domain(&lower).as_deref() == Some("chamber.org.hk")
                && lower.contains("directory_detail.aspx")
                && lower.contains("id=")
        }
    }
}

/// What a member page says.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MemberPage {
    pub url: String,
    /// "London Chamber of Commerce Ontario, Canada", "Hong Kong General
    /// Chamber of Commerce (HKGCC)".
    pub chamber: String,
    /// Every name the page gives (English, Chinese).
    pub names: Vec<String>,
    pub place: String,
    /// The member's own website, as the page links it.
    pub website: String,
    /// "1903" (HKGCC only).
    pub joined: String,
    /// "HKJ0037" (HKGCC only).
    pub member_number: String,
}

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("valid selector")
}

fn clean(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

static COPYRIGHT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"©\s*(?:\d{4}\s*)?([^<|©\n]{3,90}?)\s*(?:\||<|$)").unwrap());

/// A ChamberMaster / GrowthZone member page.
pub fn read_chambermaster(url: &str, page: &str) -> Option<MemberPage> {
    let doc = Html::parse_document(page);
    let name = doc
        .select(&selector(r#"meta[itemprop="name"]"#))
        .find_map(|m| m.value().attr("content"))
        .map(clean)
        .filter(|n| !n.is_empty())?;
    let first_text = |css: &str| {
        doc.select(&selector(css))
            .next()
            .map(|e| clean(&e.text().collect::<String>()))
            .unwrap_or_default()
    };
    let place = [
        first_text(".gz-card-address .gz-address-city"),
        first_text(r#".gz-card-address [itemprop="addressRegion"]"#),
    ]
    .into_iter()
    .filter(|p| !p.is_empty())
    .collect::<Vec<_>>()
    .join(", ");
    let website = doc
        .select(&selector(".gz-card-website a"))
        .find_map(|a| a.value().attr("href"))
        .unwrap_or("")
        .trim()
        .to_string();
    // The chamber's name is in the page's footer ("© 2026 London Chamber
    // of Commerce Ontario, Canada"); else the directory's address.
    let chamber = COPYRIGHT
        .captures_iter(page)
        .map(|c| clean(&c[1]))
        .find(|c| c.to_ascii_lowercase().contains("chamber"))
        .or_else(|| normalize_domain(url))
        .unwrap_or_default();
    Some(MemberPage {
        url: url.to_string(),
        chamber,
        names: vec![name],
        place,
        website,
        joined: String::new(),
        member_number: String::new(),
    })
}

/// An HKGCC member page ("Hong Kong Business Directory").
pub fn read_hkgcc(url: &str, page: &str) -> Option<MemberPage> {
    let doc = Html::parse_document(page);
    let name_box = doc.select(&selector(".directory_CompanyName")).next()?;
    // English name, then the Chinese one after the <br>.
    let names: Vec<String> = name_box
        .text()
        .map(clean)
        .filter(|n| !n.is_empty())
        .collect();
    if names.is_empty() {
        return None;
    }
    let row = |label: &str| -> (String, String) {
        for tr in doc.select(&selector("#mainTable tr")) {
            let cells: Vec<_> = tr.select(&selector("td")).collect();
            if cells.len() >= 2
                && clean(&cells[0].text().collect::<String>()).eq_ignore_ascii_case(label)
            {
                let text = clean(&cells[1].text().collect::<String>());
                let link = cells[1]
                    .select(&selector("a"))
                    .find_map(|a| a.value().attr("href"))
                    .unwrap_or("")
                    .to_string();
                return (text, link);
            }
        }
        (String::new(), String::new())
    };
    let (member_number, _) = row("Membership No.");
    let (website_text, website_link) = row("Web Site");
    let (joined, _) = row("Year Joined");
    Some(MemberPage {
        url: url.to_string(),
        chamber: "Hong Kong General Chamber of Commerce (HKGCC)".to_string(),
        names,
        place: "Hong Kong".to_string(),
        website: if website_link.is_empty() {
            website_text
        } else {
            website_link
        },
        joined,
        member_number,
    })
}

/// true when one of the page's names is the seller's whole name.
pub fn same_name(company_name: &str, page: &MemberPage) -> bool {
    let Some(wanted) = compared_name(company_name) else {
        return false;
    };
    page.names
        .iter()
        .any(|n| compared_name(n).as_deref() == Some(wanted.as_str()))
}

/// The card for a member page with the seller's name.
pub fn chamber_card(seller_website: Option<&str>, page: &MemberPage) -> Signal {
    let seller_domain = seller_website.and_then(normalize_domain);
    let member_domain = normalize_domain(&page.website);
    let other_website = matches!((&seller_domain, &member_domain), (Some(a), Some(b)) if a != b);
    let mut details = Vec::new();
    if !page.place.is_empty() {
        details.push(page.place.clone());
    }
    if !page.member_number.is_empty() {
        details.push(format!("member no. {}", page.member_number));
    }
    if !page.joined.is_empty() {
        details.push(format!("member since {}", page.joined));
    }
    let details = if details.is_empty() {
        String::new()
    } else {
        format!(" ({})", details.join(", "))
    };
    let shown_name = page.names.first().cloned().unwrap_or_default();
    let (value, signal_type, sub) = if other_website {
        (
            OTHER_WEBSITE,
            "info",
            format!(
                "\"{shown_name}\"{details} is a member of the {}, but its website there is {}, not {}. It may be another company with the same name. Member page: {}",
                page.chamber,
                member_domain.unwrap_or_default(),
                seller_domain.unwrap_or_default(),
                page.url
            ),
        )
    } else {
        (
            MEMBER,
            "good",
            format!(
                "\"{shown_name}\"{details} is a member of the {}. A chamber knows its members, so this is a good sign - but it does not prove that this listing is the company's own. Member page: {}",
                page.chamber, page.url
            ),
        )
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

fn enabled() -> bool {
    std::env::var("CHAMBERS_CHECK_ENABLED")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(false)
}

async fn open_page(url: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (compatible; Safely company check; +https://safely.sh)")
        .build()
        .ok()?;
    match client.get(url).send().await {
        Ok(r) if r.status().is_success() => r.text().await.ok(),
        Ok(r) => {
            eprintln!("Safely: chamber page answered {}: {}", r.status(), url);
            None
        }
        Err(e) => {
            eprintln!("Safely: chamber page could not be opened: {}", e);
            None
        }
    }
}

/// The "Chamber of commerce" card, or None when the check is off, there
/// is no chamber to search for the seller's country, or the seller is
/// not found as a member.
pub async fn chamber_signal(
    company_name: Option<&str>,
    country: Option<&str>,
    seller_website: Option<&str>,
) -> Option<Signal> {
    let name = company_name.map(str::trim).filter(|n| !n.is_empty())?;
    if !enabled() {
        return None;
    }
    compared_name(name)?;
    let directory = directory_for(country)?;
    let found = run_serper_search(&search_query(directory, name), None).await?;
    let links: Vec<String> = found
        .organic
        .iter()
        .map(|r| r.link.clone())
        .filter(|l| is_member_page(directory, l))
        .take(MAX_PAGES)
        .collect();
    for link in links {
        let Some(page) = open_page(&link).await else {
            continue;
        };
        let read = match directory {
            Directory::ChamberMaster => read_chambermaster(&link, &page),
            Directory::Hkgcc => read_hkgcc(&link, &page),
        };
        if let Some(member) = read.filter(|m| same_name(name, m)) {
            return Some(chamber_card(seller_website, &member));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // The parts of real pages Bilal saved on Nov 4 that Safely reads.
    const CHAMBERMASTER: &str = r##"<html><head><title>Qdoba Mexican Grill | Restaurant</title></head><body>
      <div class="d-flex gz-details-head"><meta itemprop="name" content="Qdoba Mexican Grill"></div>
      <ul class="list-group gz-list-group">
        <li class="list-group-item gz-card-address"><a href="#" itemprop="address">
          <span class="gz-street-address" itemprop="streetAddress">841 Wellington Rd.</span>
          <span class="gz-address-city" itemprop="addressLocality">London</span>
          <span itemprop="addressRegion">ON</span>
          <span itemprop="postalCode">N6E 3R5</span></a></li>
        <li class="list-group-item gz-card-website">
          <a href="http://www.qdoba.com/" target="_blank" itemprop="url" class="card-link"><span itemprop="sameAs">Visit Website</span></a></li>
      </ul>
      <div class="fl-page-footer-text"> <span>© 2026 London Chamber of Commerce Ontario, Canada</span> <span> | </span> <span id="fl-site-credits">Site by <a href="http://www.growthzone.com/">GrowthZone</a></span></div>
    </body></html>"##;

    const HKGCC: &str = r#"<html><body><div id="content">
      <h1><span class="nav_page_sub_title">Hong Kong Business Directory</span></h1>
      <div class="directory_CompanyName">John Swire &amp; Sons (HK) Ltd<br>香港太古集團有限公司</div>
      <table id="mainTable" class="genTableNoTitle"><tbody>
        <tr><td class="header_Generic"><strong>Membership No.</strong></td><td>HKJ0037</td></tr>
        <tr class="even"><td><strong>Web Site</strong></td><td><a target="_blank" href="http://www.swire.com/">http://www.swire.com</a></td></tr>
        <tr><td><strong>Year Joined</strong></td><td>1903</td></tr>
        <tr class="even"><td><strong>Nature of Business</strong></td><td><ul><li>Service</li></ul></td></tr>
      </tbody></table></div></body></html>"#;

    const QDOBA_URL: &str =
        "https://business.londonchamber.com/list/member/qdoba-mexican-grill-23256";
    const SWIRE_URL: &str =
        "https://www.chamber.org.hk/en/membership/directory_detail.aspx?member_cate=HKM&id=HKJ0037";

    #[test]
    fn the_right_chamber_for_each_country() {
        assert_eq!(
            directory_for(Some("Guangdong, China")),
            Some(Directory::Hkgcc)
        );
        assert_eq!(directory_for(Some("Hong Kong")), Some(Directory::Hkgcc));
        assert_eq!(
            directory_for(Some("Texas, USA")),
            Some(Directory::ChamberMaster)
        );
        assert_eq!(
            directory_for(Some("Ontario, Canada")),
            Some(Directory::ChamberMaster)
        );
        assert_eq!(directory_for(None), Some(Directory::ChamberMaster));
        assert_eq!(directory_for(Some("Paris, France")), None);
        assert_eq!(directory_for(Some("São Paulo, Brazil")), None);
    }

    #[test]
    fn searches_and_member_pages() {
        assert_eq!(
            search_query(Directory::ChamberMaster, "Qdoba \"Mexican\" Grill"),
            "inurl:\"/list/member/\" \"Qdoba Mexican Grill\""
        );
        assert!(search_query(Directory::Hkgcc, "Swire").starts_with("site:chamber.org.hk"));
        assert!(is_member_page(Directory::ChamberMaster, QDOBA_URL));
        assert!(!is_member_page(
            Directory::ChamberMaster,
            "https://business.londonchamber.com/list/searchalpha/q"
        ));
        assert!(is_member_page(Directory::Hkgcc, SWIRE_URL));
        assert!(!is_member_page(
            Directory::Hkgcc,
            "https://www.chamber.org.hk/en/events/whatson_detail.aspx?id=1"
        ));
        assert!(!is_member_page(
            Directory::Hkgcc,
            "https://fake-chamber.org.hk.example.com/directory_detail.aspx?id=1"
        ));
    }

    #[test]
    fn a_chambermaster_page_is_read() {
        let m = read_chambermaster(QDOBA_URL, CHAMBERMASTER).unwrap();
        assert_eq!(m.names, vec!["Qdoba Mexican Grill"]);
        assert_eq!(m.place, "London, ON");
        assert_eq!(m.website, "http://www.qdoba.com/");
        assert_eq!(m.chamber, "London Chamber of Commerce Ontario, Canada");
        assert!(read_chambermaster(QDOBA_URL, "<html><body>No member</body></html>").is_none());
    }

    #[test]
    fn an_hkgcc_page_is_read() {
        let m = read_hkgcc(SWIRE_URL, HKGCC).unwrap();
        assert_eq!(
            m.names,
            vec!["John Swire & Sons (HK) Ltd", "香港太古集團有限公司"]
        );
        assert_eq!(m.member_number, "HKJ0037");
        assert_eq!(m.website, "http://www.swire.com/");
        assert_eq!(m.joined, "1903");
        assert!(read_hkgcc(SWIRE_URL, "<html><body>Search</body></html>").is_none());
    }

    #[test]
    fn whole_names_only() {
        let m = read_hkgcc(SWIRE_URL, HKGCC).unwrap();
        assert!(same_name("John Swire & Sons (HK) Limited", &m));
        assert!(same_name("香港太古集團有限公司", &m));
        assert!(!same_name("Swire", &m));
        assert!(!same_name("Swire Trading Co., Ltd.", &m));
    }

    #[test]
    fn chamber_cards() {
        let m = read_hkgcc(SWIRE_URL, HKGCC).unwrap();
        let card = chamber_card(Some("https://swire.com/about"), &m);
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (MEMBER, "good")
        );
        assert_eq!(card.category, OUTSIDE_RECORDS);
        assert!(card.sub.contains("member no. HKJ0037, member since 1903"));
        assert!(card.sub.contains("Hong Kong General Chamber of Commerce"));
        assert!(card.sub.contains(SWIRE_URL));

        // No website from the seller: still a member.
        let card = chamber_card(None, &m);
        assert_eq!(card.value, MEMBER);

        // The seller's website is another one: maybe another company.
        let card = chamber_card(Some("swire-deals.shop"), &m);
        assert_eq!(
            (card.value.as_str(), card.signal_type.as_str()),
            (OTHER_WEBSITE, "info")
        );
        assert!(card.sub.contains("swire.com, not swire-deals.shop"));
    }

    #[tokio::test]
    async fn the_check_is_off_unless_switched_on() {
        if enabled() {
            return;
        }
        assert!(
            chamber_signal(Some("Qdoba Mexican Grill"), None, None)
                .await
                .is_none()
        );
    }
}
