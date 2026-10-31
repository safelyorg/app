//! What Safely reads from a shop's own pages without asking the AI:
//! emails, phone links, company numbers, which shop policies exist, and
//! whether the domain copies a famous brand. Each is checked against
//! real page text in the tests below.

use crate::services::sources::websites::WebsitePage;
use regex::Regex;
use std::sync::LazyLock;

static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b[a-z0-9][a-z0-9._%+-]{0,63}@([a-z0-9-]+(?:\.[a-z0-9-]+)*\.[a-z]{2,})\b")
        .unwrap()
});

/// Email providers anyone can sign up to: an address there is a note,
/// not proof of a scam (standards/websites.json "rules").
const FREE_EMAIL_DOMAINS: &[&str] = &[
    "gmail.com",
    "googlemail.com",
    "hotmail.com",
    "outlook.com",
    "live.com",
    "yahoo.com",
    "yahoo.com.br",
    "icloud.com",
    "aol.com",
    "proton.me",
    "protonmail.com",
    "gmx.com",
    "mail.com",
    "yandex.com",
    "qq.com",
    "163.com",
    "126.com",
    "uol.com.br",
    "bol.com.br",
    "orange.fr",
    "free.fr",
];

/// The email addresses on the pages (text and mailto links), lower case,
/// without picture names like "logo@2x.png".
pub fn emails(pages: &[WebsitePage]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut add = |text: &str| {
        for m in EMAIL.captures_iter(text) {
            let email = m[0].to_lowercase();
            let domain = m[1].to_lowercase();
            let picture = [".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg"]
                .iter()
                .any(|p| domain.ends_with(p));
            if !picture && !found.contains(&email) {
                found.push(email);
            }
        }
    };
    for page in pages {
        add(&page.text);
        for link in &page.links {
            if let Some(mail) = link.href.strip_prefix("mailto:") {
                add(mail.split('?').next().unwrap_or(mail));
            }
        }
    }
    found
}

/// The domain of an email ("info@shop.com" -> "shop.com").
pub fn email_domain(email: &str) -> &str {
    email.rsplit('@').next().unwrap_or("")
}

pub fn is_free_email(email: &str) -> bool {
    FREE_EMAIL_DOMAINS.contains(&email_domain(email))
}

/// true when the email is on the shop's own domain (or a part of it:
/// "sales@mail.shop.com" for "shop.com").
pub fn is_own_domain_email(email: &str, site_domain: &str) -> bool {
    let d = email_domain(email);
    d == site_domain || d.ends_with(&format!(".{site_domain}"))
}

/// Phone numbers from tel: and WhatsApp links (text phone numbers are
/// left to the AI: too many other numbers look like phones).
pub fn phone_links(pages: &[WebsitePage]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for link in pages.iter().flat_map(|p| p.links.iter()) {
        let href = link.href.trim();
        let number = if let Some(n) = href.strip_prefix("tel:") {
            n.to_string()
        } else if href.contains("wa.me/") {
            href.split("wa.me/")
                .nth(1)
                .unwrap_or("")
                .split(['?', '/'])
                .next()
                .unwrap_or("")
                .to_string()
        } else {
            continue;
        };
        let digits = number.chars().filter(char::is_ascii_digit).count();
        let number = number.trim().to_string();
        if digits >= 7 && !found.contains(&number) {
            found.push(number);
        }
    }
    found
}

/// One kind of shop policy and the words its page address or title uses
/// (English, Portuguese, French, Spanish, German).
pub struct PolicyKind {
    pub name: &'static str,
    pub words: &'static [&'static str],
}

pub const POLICIES: [PolicyKind; 4] = [
    PolicyKind {
        name: "shipping policy",
        words: &[
            "shipping",
            "delivery",
            "envio",
            "entrega",
            "livraison",
            "frete",
            "versand",
        ],
    },
    PolicyKind {
        name: "return / refund policy",
        words: &[
            "return",
            "refund",
            "devolu",
            "reembolso",
            "troca",
            "retour",
            "rembourse",
            "widerruf",
        ],
    },
    PolicyKind {
        name: "terms",
        words: &[
            "terms",
            "conditions",
            "termos",
            "cgv",
            "conditions-generales",
            "agb",
            "condiciones",
        ],
    },
    PolicyKind {
        name: "privacy policy",
        words: &[
            "privacy",
            "privacidade",
            "confidentialite",
            "datenschutz",
            "privacidad",
            "rgpd",
            "gdpr",
            "lgpd",
        ],
    },
];

/// Shorter than this, a policy page is empty or only a heading.
pub const MIN_POLICY_CHARS: usize = 400;

/// Which policies have a real page among the pages read: the page
/// address or title names the policy, and it has real text.
pub fn policies_found(pages: &[WebsitePage]) -> Vec<&'static str> {
    POLICIES
        .iter()
        .filter(|kind| {
            pages.iter().any(|p| {
                let place = format!("{} {}", p.url, p.title).to_lowercase();
                kind.words.iter().any(|w| place.contains(w))
                    && p.text.chars().count() >= MIN_POLICY_CHARS
            })
        })
        .map(|kind| kind.name)
        .collect()
}

/// Labels a shop writes before its company number, in the languages
/// of the countries Safely checks numbers for.
static NUMBER_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i:\b(?:CNPJ|SIREN|SIRET|TVA(?:\s+intracommunautaire)?|N°\s*TVA|VAT(?:\s+(?:no|number|id))?|USCC|Unified Social Credit Code|统一社会信用代码|Company (?:registration )?(?:no|number)))\.?\s*[:：#]?\s*((?:[A-Z]{2}\s?)?[0-9A-Z]*[0-9][0-9A-Z]*(?:[./-][0-9A-Z]+| [0-9][0-9A-Z]*)*)",
    )
    .unwrap()
});

/// Company numbers the shop writes next to a label ("CNPJ:
/// 12.345.678/0001-95", "SIRET 732 829 320 00074", "TVA FR44732829320").
pub fn company_numbers(pages: &[WebsitePage]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for page in pages {
        for m in NUMBER_LABEL.captures_iter(&page.text) {
            let number = m[1].trim().to_string();
            if number.chars().filter(char::is_ascii_digit).count() >= 6 && !found.contains(&number)
            {
                found.push(number);
            }
        }
    }
    found
}

/// Famous brands that scam shops copy, with the name in their own
/// domains. A domain is the brand's own when its first part IS the
/// name ("amazon.co.uk").
const BRANDS: &[(&str, &str)] = &[
    ("amazon", "Amazon"),
    ("apple", "Apple"),
    ("paypal", "PayPal"),
    ("ebay", "eBay"),
    ("walmart", "Walmart"),
    ("alibaba", "Alibaba"),
    ("aliexpress", "AliExpress"),
    ("nike", "Nike"),
    ("adidas", "Adidas"),
    ("samsung", "Samsung"),
    ("microsoft", "Microsoft"),
    ("netflix", "Netflix"),
    ("shein", "Shein"),
    ("temu", "Temu"),
    ("bestbuy", "Best Buy"),
    ("costco", "Costco"),
    ("ikea", "IKEA"),
    ("zara", "Zara"),
    ("louisvuitton", "Louis Vuitton"),
    ("gucci", "Gucci"),
    ("rolex", "Rolex"),
    ("mercadolivre", "Mercado Livre"),
    ("mercadolibre", "Mercado Libre"),
    ("americanas", "Americanas"),
    ("magazineluiza", "Magazine Luiza"),
    ("shopee", "Shopee"),
    ("lululemon", "Lululemon"),
    ("ray-ban", "Ray-Ban"),
    ("rayban", "Ray-Ban"),
];

/// "amaz0n" -> "amazon": digits scam domains use in place of letters.
fn undo_lookalike_letters(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '0' => 'o',
            '1' => 'l',
            '3' => 'e',
            '4' => 'a',
            '5' => 's',
            '7' => 't',
            other => other,
        })
        .collect()
}

/// Number of single-letter changes between two words.
fn edits(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let keep = row[j];
            row[j] = (row[j] + 1)
                .min(row[j - 1] + 1)
                .min(prev + usize::from(a[i - 1] != b[j - 1]));
            prev = keep;
        }
    }
    row[b.len()]
}

/// The famous brand this domain copies, or None. "amazon-deals.shop",
/// "amaz0n.com" and "paypa1-secure.net" copy one; "amazon.co.uk" is the
/// brand's own and "targetmarketing.com" copies nothing (a brand name
/// hidden inside another word does not count).
pub fn copied_brand(domain: &str) -> Option<&'static str> {
    let first = domain.split('.').next().unwrap_or("");
    if BRANDS.iter().any(|(name, _)| first == *name) {
        return None;
    }
    let parts: Vec<String> = undo_lookalike_letters(first)
        .split('-')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    let joined = undo_lookalike_letters(first).replace('-', "");
    BRANDS.iter().find_map(|(name, label)| {
        let name = name.replace('-', "");
        let exact_part = parts.iter().any(|p| *p == name) || joined == name;
        // Only longer names: "apple" is one letter from "apply" and "ample".
        let near = name.len() >= 6
            && (edits(&joined, &name) == 1 || parts.iter().any(|p| edits(p, &name) == 1));
        (exact_part || near).then_some(*label)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::sources::websites::{PageLink, WebsitePage};

    fn page(url: &str, title: &str, text: &str, links: &[&str]) -> WebsitePage {
        WebsitePage {
            url: url.to_string(),
            title: title.to_string(),
            text: text.to_string(),
            links: links
                .iter()
                .map(|h| PageLink {
                    href: h.to_string(),
                    text: String::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn emails_are_found_in_text_and_links() {
        let pages = [page(
            "https://shop.com/contact",
            "Contact",
            "Write to Sales@Shop.com or our support desk. logo@2x.png",
            &["mailto:help@shop.com?subject=Hi", "mailto:sales@shop.com"],
        )];
        assert_eq!(emails(&pages), vec!["sales@shop.com", "help@shop.com"]);
        assert!(is_own_domain_email("a@mail.shop.com", "shop.com"));
        assert!(!is_own_domain_email("a@notshop.com", "shop.com"));
        assert!(is_free_email("seller123@gmail.com"));
        assert!(!is_free_email("info@shop.com"));
    }

    #[test]
    fn phones_come_from_tel_and_whatsapp_links() {
        let pages = [page(
            "u",
            "",
            "",
            &[
                "tel:+55 11 4002-8922",
                "https://wa.me/5511999998888?text=oi",
                "tel:123",
            ],
        )];
        assert_eq!(
            phone_links(&pages),
            vec!["+55 11 4002-8922", "5511999998888"]
        );
    }

    #[test]
    fn policies_need_a_real_page() {
        let long = "x".repeat(MIN_POLICY_CHARS);
        let pages = [
            page(
                "https://shop.com/policies/refund-policy",
                "Refund policy",
                &long,
                &[],
            ),
            page(
                "https://shop.com/pages/politica-de-privacidade",
                "",
                &long,
                &[],
            ),
            page(
                "https://shop.com/policies/shipping-policy",
                "Shipping",
                "Coming soon",
                &[],
            ),
        ];
        assert_eq!(
            policies_found(&pages),
            vec!["return / refund policy", "privacy policy"]
        );
    }

    #[test]
    fn company_numbers_next_to_their_label() {
        let pages = [page(
            "u",
            "",
            "© 2026 Loja Exemplo LTDA - CNPJ: 11.222.333/0001-81 - Rua X, 10. SIRET 732 829 320 00074 · TVA intracommunautaire : FR44732829320. Order 12345 shipped.",
            &[],
        )];
        assert_eq!(
            company_numbers(&pages),
            vec!["11.222.333/0001-81", "732 829 320 00074", "FR44732829320"]
        );
    }

    #[test]
    fn copied_brands_are_spotted() {
        assert_eq!(copied_brand("amazon-deals.shop"), Some("Amazon"));
        assert_eq!(copied_brand("amaz0n.com"), Some("Amazon"));
        assert_eq!(copied_brand("paypa1-secure.net"), Some("PayPal"));
        assert_eq!(copied_brand("nike-outlet-sale.com"), Some("Nike"));
        assert_eq!(copied_brand("samsvng.store"), Some("Samsung"));
        // The brands' own domains.
        assert_eq!(copied_brand("amazon.co.uk"), None);
        assert_eq!(copied_brand("mercadolivre.com.br"), None);
        // A brand hidden inside another word, and ordinary shops.
        assert_eq!(copied_brand("targetmarketing.com"), None);
        assert_eq!(copied_brand("cheap-iphones-store.com"), None);
        assert_eq!(copied_brand("bellacasa.com.br"), None);
        assert_eq!(copied_brand("apply-now.com"), None);
    }
}
