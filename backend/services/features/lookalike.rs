//! The Lookalike Domain Checker (safely.sh/tools/lookalike-domain-checker).
//!
//! A scammer copies a real company's website name with a small trick:
//! one letter changed (alibabba.com), a number for a letter (paypa1.com),
//! letters that look alike together (rn for m), an extra word
//! (paypal-secure.com), another ending (.co for .com), the real name in
//! front of another website (paypal.com.login-help.xyz) or letters from
//! another alphabet that look the same (Cyrillic "а" in аpple.com).
//!
//! 1. The visitor's text is cleaned into a domain, like the Domain Age
//!    Checker does, but letters from other alphabets are kept, and
//!    "xn--" names are turned back into the letters they stand for.
//! 2. With the real company's website, the two are compared.
//!    Without it, the domain is compared with famous brands (BRANDS).
//! 3. The answer names each trick found, so the page can show it.
//!
//! Nothing is looked up on the internet: the check is done here, so it is
//! instant and has no limit.

use crate::models::lookalike::{LookalikeAnswer, LookalikeVerdict, Trick, TrickKind};

/// Famous brands that scam websites copy: (the name in their domain, the
/// brand as people write it, its own websites). A domain with the same
/// name but another ending is "Close", not "Same": it can be the brand's
/// site for another country.
const BRANDS: &[(&str, &str, &[&str])] = &[
    (
        "amazon",
        "Amazon",
        &[
            "amazon.com",
            "amazon.co.uk",
            "amazon.de",
            "amazon.fr",
            "amazon.it",
            "amazon.es",
            "amazon.ca",
            "amazon.com.br",
            "amazon.in",
            "amazon.co.jp",
            "amazon.com.mx",
            "amazon.com.au",
            "amazon.nl",
            "amazon.ae",
            "amazon.sa",
            "amazon.sg",
            "amazon.pl",
            "amazon.se",
            "amazon.com.tr",
        ],
    ),
    ("paypal", "PayPal", &["paypal.com", "paypal.me"]),
    ("ebay", "eBay", &["ebay.com", "ebay.co.uk", "ebay.de"]),
    ("walmart", "Walmart", &["walmart.com"]),
    ("alibaba", "Alibaba", &["alibaba.com"]),
    (
        "aliexpress",
        "AliExpress",
        &["aliexpress.com", "aliexpress.us"],
    ),
    ("made-in-china", "Made-in-China", &["made-in-china.com"]),
    ("globalsources", "Global Sources", &["globalsources.com"]),
    ("dhgate", "DHgate", &["dhgate.com"]),
    ("thomasnet", "ThomasNet", &["thomasnet.com"]),
    ("kompass", "Kompass", &["kompass.com"]),
    ("tradewheel", "TradeWheel", &["tradewheel.com"]),
    ("exporthub", "ExportHub", &["exporthub.com"]),
    ("apple", "Apple", &["apple.com"]),
    ("google", "Google", &["google.com"]),
    ("microsoft", "Microsoft", &["microsoft.com"]),
    ("facebook", "Facebook", &["facebook.com"]),
    ("instagram", "Instagram", &["instagram.com"]),
    ("whatsapp", "WhatsApp", &["whatsapp.com"]),
    ("linkedin", "LinkedIn", &["linkedin.com"]),
    ("netflix", "Netflix", &["netflix.com"]),
    ("samsung", "Samsung", &["samsung.com"]),
    ("nike", "Nike", &["nike.com"]),
    ("adidas", "Adidas", &["adidas.com"]),
    ("shein", "Shein", &["shein.com"]),
    ("temu", "Temu", &["temu.com"]),
    ("bestbuy", "Best Buy", &["bestbuy.com"]),
    ("costco", "Costco", &["costco.com"]),
    ("ikea", "IKEA", &["ikea.com"]),
    ("zara", "Zara", &["zara.com"]),
    ("louisvuitton", "Louis Vuitton", &["louisvuitton.com"]),
    ("gucci", "Gucci", &["gucci.com"]),
    ("rolex", "Rolex", &["rolex.com"]),
    ("lululemon", "Lululemon", &["lululemon.com"]),
    ("ray-ban", "Ray-Ban", &["ray-ban.com"]),
    ("mercadolivre", "Mercado Livre", &["mercadolivre.com.br"]),
    ("mercadolibre", "Mercado Libre", &["mercadolibre.com"]),
    ("americanas", "Americanas", &["americanas.com.br"]),
    ("magazineluiza", "Magazine Luiza", &["magazineluiza.com.br"]),
    ("shopee", "Shopee", &["shopee.com", "shopee.com.br"]),
    ("dhl", "DHL", &["dhl.com", "dhl.de"]),
    ("fedex", "FedEx", &["fedex.com"]),
    ("ups", "UPS", &["ups.com"]),
    ("maersk", "Maersk", &["maersk.com"]),
    ("hsbc", "HSBC", &["hsbc.com", "hsbc.co.uk"]),
    ("payoneer", "Payoneer", &["payoneer.com"]),
    ("stripe", "Stripe", &["stripe.com"]),
    ("coinbase", "Coinbase", &["coinbase.com"]),
    ("binance", "Binance", &["binance.com"]),
];

/// Shorter brand names only count when copied exactly: "apply" is one
/// letter from "apple", "ops" from "ups".
const BRAND_MIN_FOR_ONE_CHANGE: usize = 6;
/// With the real website given, one change counts from this length.
const REAL_MIN_FOR_ONE_CHANGE: usize = 4;
/// Two changes only count for names this long.
const MIN_FOR_TWO_CHANGES: usize = 7;

/// Second-level parts of country endings: "co" in example.co.uk.
const SECOND_LEVEL: &[&str] = &[
    "co", "com", "net", "org", "gov", "edu", "ac", "or", "ne", "go", "ltd", "plc", "gob",
];

/// A cleaned domain, split into its parts.
#[derive(Debug, Clone, PartialEq)]
pub struct Domain {
    /// "shop.paypal.com", with any "xn--" part turned back into letters.
    pub full: String,
    /// The domain as it was typed, cleaned: "xn--pple-43d.com" stays as
    /// it is, so the page shows what the visitor gave.
    pub typed: String,
    /// The website part of `typed`: "xn--pple-43d.com", "login-help.xyz".
    pub typed_site: String,
    /// The parts in front of the name: ["shop"].
    pub subs: Vec<String>,
    /// The name: "paypal".
    pub name: String,
    /// The ending: "com", "co.uk".
    pub ending: String,
}

impl Domain {
    /// "paypal.com": the name and the ending, the part that is bought.
    pub fn site(&self) -> String {
        format!("{}.{}", self.name, self.ending)
    }
}

/// Turns what the visitor typed (a domain, a link or an email address)
/// into a Domain, or None when it is not a domain.
///
/// "https://www.Shop.PayPal.com/login" -> shop | paypal | com
pub fn parse_domain(input: &str) -> Option<Domain> {
    let mut text = input.trim().to_lowercase();
    if let Some((_, rest)) = text.split_once("://") {
        text = rest.to_string();
    }
    let host = text.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.rsplit('@').next().unwrap_or_default();
    let host = host.split(':').next().unwrap_or_default();
    let host = host.trim_end_matches('.');
    let host = host.strip_prefix("www.").unwrap_or(host);
    if host.chars().count() > 253 {
        return None;
    }

    let labels: Vec<String> = host
        .split('.')
        .map(|l| match l.strip_prefix("xn--") {
            Some(code) => punycode_decode(code).unwrap_or_else(|| l.to_string()),
            None => l.to_string(),
        })
        .collect();
    let valid = labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && l.chars().count() <= 63
                && l.chars().all(|c| c.is_alphanumeric() || c == '-')
                && !l.starts_with('-')
                && !l.ends_with('-')
        });
    let tld = labels.last()?;
    if !valid || tld.chars().count() < 2 || !tld.chars().all(char::is_alphabetic) {
        return None;
    }

    // "example.co.uk": a 2-letter country ending after "co", "com"...
    let n = labels.len();
    let two_part =
        n >= 3 && tld.chars().count() == 2 && SECOND_LEVEL.contains(&labels[n - 2].as_str());
    let ending_len = if two_part { 2 } else { 1 };
    let name = labels[n - ending_len - 1].clone();
    let ending = labels[n - ending_len..].join(".");
    let subs = labels[..n - ending_len - 1].to_vec();
    Some(Domain {
        full: labels.join("."),
        typed: host.to_string(),
        typed_site: host
            .split('.')
            .skip(n - ending_len - 1)
            .collect::<Vec<_>>()
            .join("."),
        subs,
        name,
        ending,
    })
}

/// Decodes the part after "xn--" (RFC 3492), or None when it is broken.
/// "pple-43d" -> "аpple" (with a Cyrillic "а").
pub fn punycode_decode(code: &str) -> Option<String> {
    const BASE: u32 = 36;
    const TMIN: u32 = 1;
    const TMAX: u32 = 26;
    const SKEW: u32 = 38;
    const DAMP: u32 = 700;

    fn adapt(delta: u32, points: u32, first: bool) -> u32 {
        let mut delta = if first { delta / DAMP } else { delta / 2 };
        delta += delta / points;
        let mut k = 0;
        while delta > ((BASE - TMIN) * TMAX) / 2 {
            delta /= BASE - TMIN;
            k += BASE;
        }
        k + (BASE - TMIN + 1) * delta / (delta + SKEW)
    }

    let (basic, rest) = match code.rfind('-') {
        Some(i) => (&code[..i], &code[i + 1..]),
        None => ("", code),
    };
    if !basic.is_ascii() {
        return None;
    }
    let mut output: Vec<char> = basic.chars().collect();
    let (mut n, mut i, mut bias) = (128u32, 0u32, 72u32);
    let mut chars = rest.chars().peekable();
    while chars.peek().is_some() {
        let old_i = i;
        let mut w = 1u32;
        let mut k = BASE;
        loop {
            let c = chars.next()?;
            let digit = match c {
                'a'..='z' => c as u32 - 'a' as u32,
                '0'..='9' => c as u32 - '0' as u32 + 26,
                _ => return None,
            };
            i = i.checked_add(digit.checked_mul(w)?)?;
            let t = if k <= bias {
                TMIN
            } else if k >= bias + TMAX {
                TMAX
            } else {
                k - bias
            };
            if digit < t {
                break;
            }
            w = w.checked_mul(BASE - t)?;
            k += BASE;
        }
        let len = output.len() as u32 + 1;
        bias = adapt(i - old_i, len, old_i == 0);
        n = n.checked_add(i / len)?;
        i %= len;
        output.insert(i as usize, char::from_u32(n)?);
        i += 1;
    }
    Some(output.into_iter().collect())
}

/// A letter from another alphabet (or with an accent) that looks like a
/// plain Latin letter, as that letter.
fn latin_twin(c: char) -> Option<char> {
    Some(match c {
        // Cyrillic
        'а' => 'a',
        'е' => 'e',
        'о' => 'o',
        'р' => 'p',
        'с' => 'c',
        'у' => 'y',
        'х' => 'x',
        'і' => 'i',
        'ј' => 'j',
        'ѕ' => 's',
        'һ' => 'h',
        'ԁ' => 'd',
        'ԛ' => 'q',
        'ԝ' => 'w',
        'ӏ' => 'l',
        'к' => 'k',
        'м' => 'm',
        'т' => 't',
        'в' => 'b',
        'п' => 'n',
        // Greek
        'α' => 'a',
        'ο' => 'o',
        'ε' => 'e',
        'ι' => 'i',
        'κ' => 'k',
        'ν' => 'v',
        'ρ' => 'p',
        'τ' => 't',
        'υ' => 'u',
        // Latin letters with an accent
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ı' => 'i',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        'ñ' => 'n',
        'ý' | 'ÿ' => 'y',
        _ => return None,
    })
}

/// Which alphabet a look-alike letter is from, for the page to name:
/// "cyrillic", "greek" or "accent" (a Latin letter with an accent).
fn alphabet(c: char) -> &'static str {
    match c as u32 {
        0x0400..=0x052F => "cyrillic",
        0x0370..=0x03FF => "greek",
        _ => "accent",
    }
}

/// A number scam names use for a letter.
fn number_twin(c: char) -> Option<char> {
    Some(match c {
        '0' => 'o',
        '1' => 'l',
        '3' => 'e',
        '4' => 'a',
        '5' => 's',
        '7' => 't',
        _ => return None,
    })
}

/// Letters that look like one letter together.
const LETTER_PAIRS: &[(&str, &str)] = &[("rn", "m"), ("vv", "w"), ("cl", "d")];

/// The name with every look-alike undone, and the tricks that were undone.
fn plain(name: &str) -> (String, Vec<Trick>) {
    let mut tricks = Vec::new();
    let foreign: String = name.chars().filter(|c| latin_twin(*c).is_some()).collect();
    let mut text: String = name.chars().map(|c| latin_twin(c).unwrap_or(c)).collect();
    if let Some(first) = foreign.chars().next() {
        tricks.push(trick(TrickKind::ForeignLetters, &foreign, alphabet(first)));
    }
    let mut numbers = Vec::new();
    text = text
        .chars()
        .map(|c| match number_twin(c) {
            Some(letter) => {
                numbers.push((c, letter));
                letter
            }
            None => c,
        })
        .collect();
    for (number, letter) in numbers {
        let t = trick(
            TrickKind::NumberForLetter,
            &number.to_string(),
            &letter.to_string(),
        );
        if !tricks.contains(&t) {
            tricks.push(t);
        }
    }
    for (pair, letter) in LETTER_PAIRS {
        if text.contains(pair) {
            text = text.replace(pair, letter);
            tricks.push(trick(TrickKind::LookalikeLetters, pair, letter));
        }
    }
    (text, tricks)
}

fn trick(kind: TrickKind, a: &str, b: &str) -> Trick {
    Trick {
        kind,
        a: a.to_string(),
        b: b.to_string(),
    }
}

/// Number of single-letter changes between two words.
fn edits(a: &[char], b: &[char]) -> usize {
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

/// The one small change from `real` to `fake`, if there is exactly one:
/// a letter changed, left out or added, or two letters swapped.
fn one_change(fake: &str, real: &str) -> Option<Trick> {
    let f: Vec<char> = fake.chars().collect();
    let r: Vec<char> = real.chars().collect();
    if f == r {
        return None;
    }
    if f.len() == r.len() {
        let diff: Vec<usize> = (0..f.len()).filter(|&i| f[i] != r[i]).collect();
        if diff.len() == 1 {
            let i = diff[0];
            return Some(trick(
                TrickKind::ChangedLetter,
                &f[i].to_string(),
                &r[i].to_string(),
            ));
        }
        if diff.len() == 2
            && diff[1] == diff[0] + 1
            && f[diff[0]] == r[diff[1]]
            && f[diff[1]] == r[diff[0]]
        {
            let i = diff[0];
            let fs: String = f[i..=i + 1].iter().collect();
            let rs: String = r[i..=i + 1].iter().collect();
            return Some(trick(TrickKind::SwappedLetters, &fs, &rs));
        }
        return None;
    }
    let (long, short, kind) = if f.len() == r.len() + 1 {
        (&f, &r, TrickKind::ExtraLetter)
    } else if f.len() + 1 == r.len() {
        (&r, &f, TrickKind::MissingLetter)
    } else {
        return None;
    };
    let i = (0..short.len())
        .find(|&i| long[i] != short[i])
        .unwrap_or(short.len());
    (long[..i] == short[..i] && long[i + 1..] == short[i..])
        .then(|| trick(kind, &long[i].to_string(), ""))
}

/// How `fake` copies the name `real`: the verdict and the tricks used.
/// `strict` is for famous brands (no real website given): the name must
/// stand on its own or between dashes, and short names must be exact.
fn compare_names(fake: &str, real: &str, strict: bool) -> (LookalikeVerdict, Vec<Trick>) {
    use LookalikeVerdict::*;
    let (fake_plain, mut tricks) = plain(fake);
    let (real_plain, real_tricks) = plain(real);
    // "cl" in cloudflare-shop is not a trick when the real name has it too.
    tricks.retain(|t| !real_tricks.contains(t));
    let real_flat = real_plain.replace('-', "");
    let fake_flat = fake_plain.replace('-', "");
    let real_len = real_flat.chars().count();
    let min_one = if strict {
        BRAND_MIN_FOR_ONE_CHANGE
    } else {
        REAL_MIN_FOR_ONE_CHANGE
    };

    if fake == real {
        return (Same, Vec::new());
    }
    // Only look-alike letters: аpple, paypa1, rnicrosoft.
    if fake_plain == real_plain {
        return (Lookalike, tricks);
    }
    // A dash added: pay-pal.
    if fake_flat == real_flat {
        tricks.push(trick(TrickKind::ExtraDash, "", ""));
        return (Lookalike, tricks);
    }
    // One change in the whole name: alibabba, amazn, gogle.
    if real_len >= min_one
        && let Some(change) = one_change(&fake_flat, &real_flat)
    {
        tricks.push(change);
        return (Lookalike, tricks);
    }
    // The real name, or the name with one change, plus a word: paypal-secure.
    let parts: Vec<&str> = fake_plain.split('-').filter(|p| !p.is_empty()).collect();
    if parts.len() > 1 {
        for (i, part) in parts.iter().enumerate() {
            let rest: Vec<&str> = parts
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, p)| *p)
                .collect();
            let change = if *part == real_flat {
                None
            } else if real_len >= min_one {
                match one_change(part, &real_flat) {
                    Some(c) => Some(c),
                    None => continue,
                }
            } else {
                continue;
            };
            tricks.extend(change);
            tricks.push(trick(TrickKind::ExtraWord, &rest.join("-"), ""));
            return (Lookalike, tricks);
        }
        // "made-in-china-shop": a real name with dashes, plus a word.
        if real_plain.contains('-') && fake_plain.starts_with(&format!("{real_plain}-")) {
            let extra = &fake_plain[real_plain.len() + 1..];
            tricks.push(trick(TrickKind::ExtraWord, extra, ""));
            return (Lookalike, tricks);
        }
    }
    // The name joined to a word: paypalsecure, securepaypal. Not for brands:
    // "targetmarketing" is not Target.
    if !strict && real_len >= REAL_MIN_FOR_ONE_CHANGE {
        let extra = fake_flat
            .strip_prefix(real_flat.as_str())
            .or_else(|| fake_flat.strip_suffix(real_flat.as_str()));
        if let Some(extra) = extra
            && extra.chars().count() >= 2
        {
            tricks.push(trick(TrickKind::ExtraWord, extra, ""));
            return (Lookalike, tricks);
        }
    }
    // Two small changes in a long name: a worth-a-look, not a clear copy.
    let f: Vec<char> = fake_flat.chars().collect();
    let r: Vec<char> = real_flat.chars().collect();
    if real_len >= MIN_FOR_TWO_CHANGES && edits(&f, &r) == 2 {
        tricks.push(trick(TrickKind::TwoChanges, "", ""));
        return (Close, tricks);
    }
    (Different, Vec::new())
}

/// The real name used in front of another website:
/// paypal.com.login-help.xyz or paypal.login-help.xyz.
fn name_in_front(domain: &Domain, real_name: &str) -> bool {
    domain
        .subs
        .iter()
        .any(|sub| sub == real_name || plain(sub).0 == plain(real_name).0)
}

/// Compares the domain with the real company's website.
pub fn check_against(domain: &Domain, real: &Domain) -> LookalikeAnswer {
    let (verdict, tricks) = judge(domain, &real.name, &[real.site()], false);
    LookalikeAnswer {
        domain: domain.typed.clone(),
        site: domain.typed_site.clone(),
        compared_with: Some(real.site()),
        brand: None,
        verdict,
        tricks,
    }
}

/// The verdict for one real name and its own websites.
fn judge(
    domain: &Domain,
    real_name: &str,
    own_sites: &[String],
    strict: bool,
) -> (LookalikeVerdict, Vec<Trick>) {
    use LookalikeVerdict::*;
    let site = domain.site();
    if own_sites.contains(&site) {
        return (Same, Vec::new());
    }
    if name_in_front(domain, real_name) {
        return (Lookalike, vec![trick(TrickKind::NameInFront, &site, "")]);
    }
    let (verdict, mut tricks) = compare_names(&domain.name, real_name, strict);
    let other_ending = |tricks: &mut Vec<Trick>| {
        let real_ending = own_sites
            .first()
            .and_then(|s| s.split_once('.'))
            .map(|(_, e)| e.to_string())
            .unwrap_or_default();
        if real_ending != domain.ending {
            tricks.push(trick(
                TrickKind::DifferentEnding,
                &format!(".{}", domain.ending),
                &format!(".{real_ending}"),
            ));
        }
    };
    match verdict {
        // Same name, another ending: maybe the company's site for another
        // country, maybe a copy.
        Same => {
            other_ending(&mut tricks);
            (Close, tricks)
        }
        Lookalike | Close => {
            other_ending(&mut tricks);
            (verdict, tricks)
        }
        _ => (verdict, tricks),
    }
}

/// Letters from another alphabet mixed with plain ones in one name
/// ("gооgle-shop" with Cyrillic "о"): a trick even without a brand.
fn mixes_alphabets(name: &str) -> bool {
    name.chars().any(|c| c.is_ascii_alphabetic())
        && name.chars().any(|c| {
            !c.is_ascii() && latin_twin(c).is_some() && !"àáâãäåèéêëìíîïòóôõöùúûüçñýÿı".contains(c)
        })
}

/// Compares the domain with the famous brands, for when no real website
/// was given.
pub fn check_brands(domain: &Domain) -> LookalikeAnswer {
    use LookalikeVerdict::*;
    let rank = |v: LookalikeVerdict| match v {
        Same => 3,
        Lookalike => 2,
        Close => 1,
        _ => 0,
    };
    let mut best: Option<(LookalikeVerdict, Vec<Trick>, &str, &str)> = None;
    for (name, label, sites) in BRANDS {
        let own: Vec<String> = sites.iter().map(|s| s.to_string()).collect();
        let (verdict, tricks) = judge(domain, name, &own, true);
        if rank(verdict) > best.as_ref().map(|b| rank(b.0)).unwrap_or(0) {
            let main = sites.first().copied().unwrap_or(name);
            let compared = if verdict == Same { "" } else { main };
            best = Some((verdict, tricks, label, compared));
            if verdict == Same {
                break;
            }
        }
    }
    let site = domain.typed_site.clone();
    match best {
        Some((verdict, tricks, label, compared)) => LookalikeAnswer {
            domain: domain.typed.clone(),
            compared_with: Some(if compared.is_empty() {
                site.clone()
            } else {
                compared.to_string()
            }),
            site,
            brand: Some(label.to_string()),
            verdict,
            tricks,
        },
        None => {
            let mixed = mixes_alphabets(&domain.name);
            LookalikeAnswer {
                domain: domain.typed.clone(),
                site,
                compared_with: None,
                brand: None,
                verdict: if mixed { Close } else { NoBrand },
                tricks: if mixed {
                    plain(&domain.name)
                        .1
                        .into_iter()
                        .filter(|t| t.kind == TrickKind::ForeignLetters)
                        .collect()
                } else {
                    Vec::new()
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use LookalikeVerdict::*;
    use TrickKind::*;

    fn d(s: &str) -> Domain {
        parse_domain(s).unwrap_or_else(|| panic!("{s} should parse"))
    }

    fn vs(fake: &str, real: &str) -> LookalikeAnswer {
        check_against(&d(fake), &d(real))
    }

    fn kinds(a: &LookalikeAnswer) -> Vec<TrickKind> {
        a.tricks.iter().map(|t| t.kind).collect()
    }

    #[test]
    fn cleans_what_people_type() {
        let x = d("https://www.Shop.PayPal.com:443/login?x=1");
        assert_eq!(
            (x.subs, x.name.as_str(), x.ending.as_str()),
            (vec!["shop".to_string()], "paypal", "com")
        );
        let x = d("sales@alibabba.com");
        assert_eq!(x.site(), "alibabba.com");
        let x = d("loja.exemplo.com.br");
        assert_eq!((x.name.as_str(), x.ending.as_str()), ("exemplo", "com.br"));
        let x = d("amazon.co.uk");
        assert_eq!((x.name.as_str(), x.ending.as_str()), ("amazon", "co.uk"));
        // Another alphabet is kept, and "xn--" turned back into letters.
        assert_eq!(d("аpple.com").name, "аpple");
        assert_eq!(d("xn--pple-43d.com").name, "аpple");
        for bad in ["", "paypal", "192.168.1.1", "pay pal.com", "-x.com", "x.c"] {
            assert_eq!(parse_domain(bad), None, "{bad}");
        }
    }

    #[test]
    fn punycode() {
        assert_eq!(punycode_decode("mnchen-3ya").as_deref(), Some("münchen"));
        assert_eq!(punycode_decode("pple-43d").as_deref(), Some("аpple"));
        assert_eq!(punycode_decode("!!"), None);
    }

    #[test]
    fn the_same_website() {
        assert_eq!(vs("paypal.com", "paypal.com").verdict, Same);
        assert_eq!(
            vs("https://shop.paypal.com/x", "www.paypal.com").verdict,
            Same
        );
        assert_eq!(vs("info@supplier.com", "supplier.com").verdict, Same);
    }

    #[test]
    fn one_letter_tricks() {
        let a = vs("alibabba.com", "alibaba.com");
        assert_eq!((a.verdict, kinds(&a)), (Lookalike, vec![ExtraLetter]));
        assert_eq!(a.tricks[0].a, "b");
        let a = vs("amazn.com", "amazon.com");
        assert_eq!((a.verdict, kinds(&a)), (Lookalike, vec![MissingLetter]));
        assert_eq!(a.tricks[0].a, "o");
        let a = vs("supplyer.com", "supplier.com");
        assert_eq!(kinds(&a), vec![ChangedLetter]);
        assert_eq!((a.tricks[0].a.as_str(), a.tricks[0].b.as_str()), ("y", "i"));
        let a = vs("goolge.com", "google.com");
        assert_eq!(kinds(&a), vec![SwappedLetters]);
        assert_eq!(
            (a.tricks[0].a.as_str(), a.tricks[0].b.as_str()),
            ("lg", "gl")
        );
    }

    #[test]
    fn letters_that_look_alike() {
        let a = vs("paypa1.com", "paypal.com");
        assert_eq!((a.verdict, kinds(&a)), (Lookalike, vec![NumberForLetter]));
        assert_eq!((a.tricks[0].a.as_str(), a.tricks[0].b.as_str()), ("1", "l"));
        let a = vs("rnicrosoft.com", "microsoft.com");
        assert_eq!(kinds(&a), vec![LookalikeLetters]);
        let a = vs("аpple.com", "apple.com");
        assert_eq!((a.verdict, kinds(&a)), (Lookalike, vec![ForeignLetters]));
        assert_eq!(
            (a.tricks[0].a.as_str(), a.tricks[0].b.as_str()),
            ("а", "cyrillic")
        );
        assert_eq!(a.domain, "аpple.com");
        // Typed as "xn--", it is shown as typed.
        assert_eq!(
            vs("xn--pple-43d.com", "apple.com").domain,
            "xn--pple-43d.com"
        );
        assert_eq!(vs("amazön.com", "amazon.com").tricks[0].b, "accent");
    }

    #[test]
    fn words_dashes_endings_and_names_in_front() {
        let a = vs("paypal-secure.com", "paypal.com");
        assert_eq!((a.verdict, kinds(&a)), (Lookalike, vec![ExtraWord]));
        assert_eq!(a.tricks[0].a, "secure");
        let a = vs("securepaypal.net", "paypal.com");
        assert_eq!(kinds(&a), vec![ExtraWord, DifferentEnding]);
        assert_eq!(kinds(&vs("pay-pal.com", "paypal.com")), vec![ExtraDash]);
        let a = vs("paypal.co", "paypal.com");
        assert_eq!((a.verdict, kinds(&a)), (Close, vec![DifferentEnding]));
        assert_eq!(
            (a.tricks[0].a.as_str(), a.tricks[0].b.as_str()),
            (".co", ".com")
        );
        let a = vs("paypal.com.login-help.xyz", "paypal.com");
        assert_eq!((a.verdict, kinds(&a)), (Lookalike, vec![NameInFront]));
        assert_eq!(
            (a.site.as_str(), a.tricks[0].a.as_str()),
            ("login-help.xyz", "login-help.xyz")
        );
    }

    #[test]
    fn different_names_are_not_copies() {
        assert_eq!(vs("bellacasa.com.br", "paypal.com").verdict, Different);
        assert_eq!(vs("shenzhen-tech.com", "alibaba.com").verdict, Different);
        // Two changes in a long name: worth a look, not a clear copy.
        assert_eq!(vs("alibbabba.com", "alibaba.com").verdict, Close);
    }

    #[test]
    fn famous_brands_without_a_real_website() {
        let a = check_brands(&d("amaz0n-deals.shop"));
        assert_eq!((a.verdict, a.brand.as_deref()), (Lookalike, Some("Amazon")));
        assert_eq!(a.compared_with.as_deref(), Some("amazon.com"));
        assert_eq!(kinds(&a), vec![NumberForLetter, ExtraWord, DifferentEnding]);
        assert_eq!(
            check_brands(&d("alibabba.com")).brand.as_deref(),
            Some("Alibaba")
        );
        assert_eq!(
            check_brands(&d("made-in-china-shop.com")).brand.as_deref(),
            Some("Made-in-China")
        );
        // The brands' own websites.
        let a = check_brands(&d("www.amazon.co.uk"));
        assert_eq!((a.verdict, a.brand.as_deref()), (Same, Some("Amazon")));
        // The same name with another ending: maybe a country site.
        assert_eq!(check_brands(&d("paypal.ru")).verdict, Close);
        // Ordinary names, and brands hidden inside a word.
        for name in [
            "targetmarketing.com",
            "apply-now.com",
            "bellacasa.com.br",
            "ops.com",
            "wiser.com",
        ] {
            assert_eq!(check_brands(&d(name)).verdict, NoBrand, "{name}");
        }
        // Another alphabet mixed in, even without a brand.
        let a = check_brands(&d("sunshіne-shop.com"));
        assert_eq!((a.verdict, kinds(&a)), (Close, vec![ForeignLetters]));
        assert_eq!(check_brands(&d("münchen.de")).verdict, NoBrand);
    }

    #[test]
    fn answer_as_json_for_the_page() {
        let json = serde_json::to_string(&vs("paypa1.com", "paypal.com")).unwrap();
        assert!(json.contains("\"verdict\":\"lookalike\""), "{json}");
        assert!(json.contains("\"kind\":\"number_for_letter\""), "{json}");
        assert!(json.contains("\"compared_with\":\"paypal.com\""), "{json}");
    }
}
