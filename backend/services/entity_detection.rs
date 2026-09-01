/// Classifies what kind of entity this seller likely is, using three
/// real, independent pieces of evidence: a name-keyword guess, a
/// WHOIS-confirmed website, and now, a genuinely CROSS-CONFIRMED
/// website - the same domain independently claimed both in the
/// listing AND on the seller's own store page.
///
/// Only this last, real evidence can safely upgrade someone to "business" on the strength
/// of a website alone; a plain WHOIS-registered domain, unconfirmed by
/// the store page, is deliberately NOT enough on its own, since that's
/// exactly what a scammer could fake by naming a famous, unrelated site.
pub fn classify_entity(seller_name: Option<&str>, website_fully_confirmed: bool) -> String {
    let name_suggests_business = seller_name
        .map(|name| {
            let lower = name.to_lowercase();
            let business_keywords = [
                "motors",
                "traders",
                "enterprises",
                "enterprise",
                "store",
                "shop",
                "pvt",
                "ltd",
                "co.",
                "company",
                "corp",
                "industries",
                "group",
            ];
            business_keywords.iter().any(|kw| lower.contains(kw))
        })
        .unwrap_or(false);

    if name_suggests_business || website_fully_confirmed {
        "business".to_string()
    } else if seller_name.is_some() {
        "individual".to_string()
    } else {
        "unknown".to_string()
    }
}
