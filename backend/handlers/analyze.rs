use crate::{
    errors::analyze::AnalyzeError,
    models::{
        analysis::{AnalyzeRequest, AnalyzeResponse, RiskLevel, Signal},
        sellers::Sellers,
    },
    services::{
        analysis::{
            BuildResponseData, authorize_request, authorize_scan, build_b2b_analysis_path,
            build_requests, resolve_seller, save_and_build_response,
        },
        b2b_scrapers::{
            b2b_page_from_browser, b2b_page_from_browser_first, fetch_b2b_page,
            get_scraper_for_platform,
        },
        listings::{create_listing, update_listing_from_b2b},
        osint::{SellerIdentifiers, verify_social_link},
        sellers::update_seller_from_b2b,
        signals::{is_supplier_page_scan, sort_signals_by_table},
        translation::{AnalysisTranslation, get_or_create_translation, save_english_baseline},
    },
};
use axum::{Json, extract::State, http::HeaderMap};
use chrono::NaiveDate;
use serde_json::{Value, to_value};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, serde::Deserialize)]
pub struct VerifySocialLinkRequest {
    pub seller_id: Uuid,
    pub url: String,
}

#[derive(Debug, serde::Serialize)]
pub struct VerifySocialLinkResponse {
    pub matched: bool,
    pub matched_identifiers: Vec<String>,
    pub confidence: String,
    pub message: String,
}

/// POST /api/v1/analyze
///
/// It's the actual, real endpoint your extension calls, it runs the entire
/// fraud-analysis process, step by step, from checking who's asking, all the way
/// to sending back a complete, saved risk report.
///
/// It confirms who's calling, and that they're allowed to right now, splits the
/// incoming request into its two separate pieces, finds or creates the seller,
/// with correctly-set verification, creates the listing itself, runs the actual
/// Claude analysis, builds the complete signal list, domain check included,
/// calculates the actual risk score, and converts it into a risk level and
/// saves everything, and builds the final response.
///
/// Language: the analysis itself always runs in English, because the
/// scoring, the risk factors and the extension all read fixed English
/// words (verdicts like "normal", values like "Full prepayment"). When
/// the person reads Safely in another language ("pt-br"), only the
/// finished text (the reason under each check, the risk factor
/// explanations) is translated at the end. Labels and values stay in
/// English in the response; the extension shows them in the person's
/// language itself.
pub async fn analyze(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(request): Json<AnalyzeRequest>,
) -> Result<Json<AnalyzeResponse>, AnalyzeError> {
    // The scan is counted now and given back if it does not finish (an
    // error below, or the buyer closed or refreshed the tab, which stops
    // this request). It stays counted once the result is saved.
    let (user_id, scan_hold) = authorize_scan(&headers, &pool).await?;
    let mut request = request;
    let reader_language = reader_language(request.language.as_deref());
    request.language = Some("en".to_string());
    // Only the supported B2B marketplaces (b2b_scrapers) are scanned.
    // Anything else is refused before a page is fetched; the scan held
    // above is given back.
    if get_scraper_for_platform(&request.platform).is_none() {
        return Err(AnalyzeError::ClaudeAnalysisFailed(format!(
            "Unsupported platform: {}",
            request.platform
        )));
    }

    // Fetch the supplier page BEFORE choosing the seller record, so the
    // record can be picked by company rather than by product link.
    // Platforms whose scraper implements company_key get one seller
    // record per company; the rest keep the per-listing ID below.
    // For platforms in BROWSER_FIRST_PLATFORMS (Wholesale Central,
    // Made-in-China, Global Sources, Alibaba), the pages the buyer's
    // browser sent are read first, and ScraperAPI is only the backup.
    // Everywhere else ScraperAPI is first. For platforms in
    // BROWSER_PAGE_PLATFORMS (Alibaba), the browser's page is tried once
    // more at the end if ScraperAPI could not get the listing either.
    let from_browser = b2b_page_from_browser_first(
        &request.platform,
        &request.listing_url,
        request.page_html.as_deref(),
        request.company_page_html.as_deref(),
    )
    .await;
    let page = match from_browser {
        Some(page) => Some(page),
        None => match fetch_b2b_page(&request.platform, &request.listing_url).await {
            Some(page) => Some(page),
            None => {
                b2b_page_from_browser(
                    &request.platform,
                    &request.listing_url,
                    request.page_html.as_deref(),
                )
                .await
            }
        },
    };
    let page = page.ok_or_else(|| {
        AnalyzeError::ClaudeAnalysisFailed("Could not fetch B2B supplier page".to_string())
    })?;
    let company_key = page.company_key.clone();
    let company_page_missing = page.company_page_missing;
    let company_url = page.company_url.clone();

    let (mut seller_req, listing_req) = build_requests(&request);
    if seller_req.platform_id.is_none() {
        seller_req.platform_id = company_key
            .clone()
            .or_else(|| match request.platform.as_str() {
                "b2brazil" => request
                    .listing_url
                    .split("/hotsite/")
                    .nth(1)
                    .and_then(|rest| rest.split('/').next())
                    .map(|s| s.to_string()),
                "exporthub" | "tradewheel" | "alibaba" => Some(request.listing_url.clone()),
                _ => None,
            });
    }
    let platform_id = seller_req.platform_id.as_deref().unwrap_or("unknown");
    let resolved = resolve_seller(&pool, &seller_req, &request.platform, platform_id).await?;

    let listing = create_listing(&pool, &listing_req, resolved.seller.id)
        .await
        .map_err(|e| AnalyzeError::Database(e.to_string()))?;

    let mut resolved = resolved;

    let (signals, risk_score, overall_risk_notes, social_candidates) = {
        let (signals, risk_score, notes, supplier, listing_data, candidates_from_b2b) =
            build_b2b_analysis_path(
                &pool,
                &request,
                resolved.fraud_count,
                resolved.seller.id,
                resolved.seller.handle.as_deref(),
                resolved.seller.join_date,
                page.supplier,
                page.listing,
                page.record,
            )
            .await?;
        let join_date = supplier.year_established.as_deref().and_then(|y| {
            y.trim()
                .parse::<i32>()
                .ok()
                .and_then(|year| NaiveDate::from_ymd_opt(year, 1, 1))
        });
        // With a company key, the seller record is the company, so its
        // profile link is the company's shop, not whichever product page
        // happened to be scanned last.
        // Prefer the company page link the scraper found. The
        // "https://{key}" form only works for keys that are hosts
        // (Alibaba's "yheyewear.en.alibaba.com"), not slugs like
        // B2Brazil's "asmetecgmbh".
        let seller_profile_url = company_url
            .clone()
            .or_else(|| {
                company_key
                    .as_deref()
                    .filter(|k| k.contains('.'))
                    .map(|k| format!("https://{}", k))
            })
            .unwrap_or_else(|| request.listing_url.clone());
        let clean_contact_name = supplier
            .contact_name
            .as_deref()
            .map(|n| n.replace('*', "").trim().to_string());
        let _ = update_seller_from_b2b(
            &pool,
            resolved.seller.id,
            supplier.company_name.as_deref(),
            supplier.country.as_deref(),
            join_date,
            clean_contact_name.as_deref(),
            supplier.contact_phone.as_deref(),
            Some(&seller_profile_url),
        )
        .await;
        // A supplier's own page (Wholesale Central) has no product title,
        // so the scan is saved under the supplier's name, which the
        // dashboard then shows as its title.
        let listing_title = listing_data.title.clone().or_else(|| {
            is_supplier_page_scan(&listing_data)
                .then(|| supplier.company_name.clone())
                .flatten()
        });
        let _ = update_listing_from_b2b(
            &pool,
            listing.id,
            listing_title.as_deref(),
            listing_data.description.as_deref(),
        )
        .await;
        if supplier.company_name.is_some() {
            resolved.seller.name = supplier.company_name.clone();
        }
        if supplier.country.is_some() {
            resolved.seller.location = supplier.country.clone();
        }
        if let Some(year_str) = supplier.year_established.as_deref() {
            if let Ok(year) = year_str.trim().parse::<i32>() {
                if let Some(date) = chrono::NaiveDate::from_ymd_opt(year, 1, 1) {
                    resolved.seller.join_date = Some(date);
                }
            }
        }
        if let Some(name) = &supplier.contact_name {
            resolved.seller.handle = Some(name.replace('*', "").trim().to_string());
        }
        if let Some(phone) = &supplier.contact_phone {
            resolved.seller.phone = Some(phone.clone());
        }
        (signals, risk_score, notes, candidates_from_b2b)
    };

    let mut signals = signals;
    // The company's own page could not be loaded (the reason is in the
    // terminal), so some company details may be missing. Tell the user
    // instead of silently showing a thinner result. "info" does not
    // change the risk score.
    if company_page_missing {
        signals.push(company_page_missing_signal());
    }
    sort_signals_by_table(&mut signals);

    let risk_level = match risk_score {
        0..=33 => RiskLevel::Low,
        34..=66 => RiskLevel::Caution,
        _ => RiskLevel::High,
    };

    let data = BuildResponseData {
        pool: &pool,
        listing_id: listing.id,
        risk_score,
        risk_level,
        signals,
        overall_risk_notes,
        user_id,
        seller: resolved.seller,
        fraud_count: resolved.fraud_count,
        network_summary: resolved.network_summary,
        social_candidates,
    };

    let mut response = save_and_build_response(data).await?;
    // The result is saved: this scan counts.
    scan_hold.finish();

    // The English result is always saved first: it is the source every
    // translation is made from. The summary saved with it is the
    // seller's fraud-report line (seller.network_summary) - the same one
    // the dashboard translates - because the dashboard and the extension
    // share these saved translations.
    let analysis_id = response.0.analysis_id;
    let signals_value = to_value(&response.0.signals).unwrap_or_default();
    let factors_value = to_value(&response.0.risk_factors).unwrap_or_default();
    if let Err(e) = save_english_baseline(
        &pool,
        analysis_id,
        &signals_value,
        &factors_value,
        &response.0.seller.network_summary,
        "en",
    )
    .await
    {
        eprintln!(
            "Safely: failed to save translation baseline for analysis {}: {:?}",
            analysis_id, e
        );
    }

    // Reader wants another language: translate the text. If that fails,
    // the scan is still shown, in English, never lost.
    if reader_language != "en" {
        match get_or_create_translation(
            &pool,
            analysis_id,
            &signals_value,
            &factors_value,
            &response.0.seller.network_summary,
            reader_language,
        )
        .await
        {
            Ok(translation) => apply_translation(&mut response.0, &translation),
            Err(e) => eprintln!(
                "Safely: could not translate analysis {} into {}, showing English: {:?}",
                analysis_id, reader_language, e
            ),
        }
    }

    Ok(response)
}

/// The language the person reads Safely in: "pt-br" or "en". Anything
/// else (missing, a typo, a language Safely doesn't have yet) is English.
fn reader_language(code: Option<&str>) -> &'static str {
    match code.map(str::trim) {
        Some(c) if c.eq_ignore_ascii_case("pt-br") || c.eq_ignore_ascii_case("pt") => "pt-br",
        _ => "en",
    }
}

/// Puts the translated text into the response: the reason under each
/// check, each risk factor's explanation and the seller's fraud-report
/// line. Labels, values, types and severities are never changed - the
/// scoring and the extension read them as fixed English words.
fn apply_translation(response: &mut AnalyzeResponse, translation: &AnalysisTranslation) {
    if let Some(items) = translation.signals.as_array() {
        for (signal, item) in response.signals.iter_mut().zip(items) {
            if let Some(sub) = item.get("sub").and_then(Value::as_str) {
                signal.sub = sub.to_string();
            }
        }
    }
    if let Some(items) = translation.risk_factors.as_array() {
        for (factor, item) in response.risk_factors.iter_mut().zip(items) {
            if let Some(description) = item.get("description").and_then(Value::as_str) {
                factor.description = description.to_string();
            }
        }
    }
    response.seller.network_summary = translation.network_summary.clone();
}

/// The note shown when the company's own page could not be loaded.
fn company_page_missing_signal() -> Signal {
    Signal {
        label: "Company details".to_string(),
        sub: "The company's own page on this platform couldn't be loaded right now, so details like founding year, employees and description may be missing. Scan again for the full result.".to_string(),
        value: "Couldn't be loaded".to_string(),
        signal_type: "info".to_string(),
        category: "company".to_string(),
        check_type: "existence".to_string(),
    }
}

pub async fn verify_social_link_handler(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(body): Json<VerifySocialLinkRequest>,
) -> Result<Json<VerifySocialLinkResponse>, AnalyzeError> {
    let _user_id = authorize_request(&headers, &pool).await?;

    let seller = sqlx::query_as::<_, Sellers>("SELECT * FROM sellers WHERE id = $1")
        .bind(body.seller_id)
        .fetch_one(&pool)
        .await
        .map_err(|e| AnalyzeError::Database(e.to_string()))?;

    let identifiers = SellerIdentifiers {
        name: seller.name.clone().or(seller.handle.clone()),
        phone: seller.phone.clone(),
        email: None,
        website: None,
        location: seller.location.clone(),
    };

    let result = verify_social_link(&body.url, &identifiers)
        .await
        .map_err(AnalyzeError::ClaudeAnalysisFailed)?;

    let has_scam_language = result
        .matched_identifiers
        .contains(&"scam_language".to_string());
    let has_profile_identifiers = result
        .matched_identifiers
        .iter()
        .any(|id| id == "name" || id == "location" || id == "phone");

    let message = match (has_profile_identifiers, has_scam_language) {
        (true, true) => "This page mentions the company and also contains scam-related language - review it carefully.".to_string(),
        (true, false) => "This appears to be a genuine profile or page for this company.".to_string(),
        (false, true) => "A complaint or scam-related mention was found, though the company's own identifying details weren't directly confirmed here.".to_string(),
        (false, false) => "No genuine match was found on this page.".to_string(),
    };

    Ok(Json(VerifySocialLinkResponse {
        matched: result.confidence != "none",
        matched_identifiers: result.matched_identifiers,
        confidence: result.confidence,
        message,
    }))
}

#[cfg(test)]
mod reader_language_tests {
    use super::reader_language;

    #[test]
    fn only_portuguese_switches_the_language() {
        assert_eq!(reader_language(Some("pt-br")), "pt-br");
        assert_eq!(reader_language(Some("PT-BR")), "pt-br");
        assert_eq!(reader_language(Some("pt")), "pt-br");
        assert_eq!(reader_language(Some("en")), "en");
        assert_eq!(reader_language(Some("fr")), "en");
        assert_eq!(reader_language(None), "en");
    }
}
