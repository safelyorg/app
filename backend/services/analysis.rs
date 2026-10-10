use crate::{
    errors::analyze::AnalyzeError,
    models::{
        analysis::{Analysis, AnalyzeRequest, AnalyzeResponse, RiskLevel, Signal},
        helpers::format_account_age,
        listings::{Listings, ListingsRequest},
        sellers::{SellerVerification, Sellers, SellersRequest, SellersResponse},
    },
    services::{
        auth::extract_user_id,
        b2b_scrapers::{B2bListingProfile, B2bSupplierProfile, SupplierRecord},
        b2c_scrapers::check_store_page,
        billing::{ScanLimitError, check_and_increment_scan_usage},
        claude::{
            CallB2bClaudeArguments, CallClaudeArguments, ClaudeAnalysis, call_b2b_claude,
            call_b2c_claude,
        },
        confidence::calculate_confidence,
        entity_detection::classify_entity,
        evidence::{record_evidence, record_risk_factors},
        fraud_reports::{build_network_summary, count_fraud_reports},
        listings::get_monthly_visit_activity,
        network_memory::build_network_memory_signal,
        osint::{PlatformCheckResult, SCAM_MENTIONS_FOUND, build_social_presence_matrix},
        risk_factors::{derive_risk_factors, not_enough_information_factor},
        sellers::{create_seller, find_seller},
        signals::{
            apply_risky_payment_note, apply_supplier_record, build_b2b_claude_signals,
            build_b2b_company_age_signal, build_b2b_listing_completeness_signal,
            build_b2b_transparency_signal, build_b2b_verification_signal, build_domain_signal,
            build_seller_verification_signals, build_signals, build_store_page_signal,
            build_whois_signal,
        },
        whois::check_domain_whois,
    },
};
use axum::{Json, http::HeaderMap};
use chrono::{Datelike, NaiveDate};
use serde_json::{Value, to_value};
use sqlx::{Error, Pool, Postgres, query_as};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use uuid::Uuid;

// This stops any one person from calling the /analyze endpoint more than 10 times within any 5-minute stretch.
// It sets up a way to track how many times each logged-in user has called the expensive /analyze endpoint recently.
pub static RATE_LIMITS: OnceLock<Mutex<HashMap<Uuid, (u32, Instant)>>> = OnceLock::new();
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(300);
const RATE_LIMIT_MAX_REQUESTS: u32 = 10;

pub struct CreateAnalysisData<'a> {
    pub pool: &'a Pool<Postgres>,
    pub listing_id: Uuid,
    pub risk_score: i16,
    pub risk_level: RiskLevel,
    pub signals: Value,
    pub network_summary: String,
    pub claude_raw: String,
    pub user_id: Uuid,
    pub confidence_level: String,
    pub confidence_reasoning: String,
    pub risk_factors: Value,
    pub social_candidates: Value,
}

pub struct ResolvedSeller {
    pub seller: Sellers,
    pub fraud_count: i64,
    pub network_summary: String,
}

pub struct BuildResponseData<'a> {
    pub pool: &'a Pool<Postgres>,
    pub listing_id: Uuid,
    pub risk_score: i16,
    pub risk_level: RiskLevel,
    pub signals: Vec<Signal>,
    pub overall_risk_notes: String,
    pub user_id: Uuid,
    pub seller: Sellers,
    pub fraud_count: i64,
    pub network_summary: String,
    pub is_b2b: bool,
    pub social_candidates: Vec<PlatformCheckResult>,
}

/// Confirms the caller is genuinely signed in, then checks they haven't
/// exceeded their request rate limit. Real analysis costs real Claude
/// API money per request, so this endpoint must actually reject an
/// anonymous or over-limit caller, not just proceed anyway.
///
/// It checks if this is a genuinely signed-in person, checks if they've
/// already hit their rate limit and if both checks passed, hand back their real user ID.
pub async fn authorize_request(
    headers: &HeaderMap,
    pool: &Pool<Postgres>,
) -> Result<Uuid, AnalyzeError> {
    let user_id = extract_user_id(headers, pool)
        .await
        .map_err(|_| AnalyzeError::Unauthorized)?
        .ok_or(AnalyzeError::Unauthorized)?;

    check_rate_limit(user_id)?;

    check_and_increment_scan_usage(pool, user_id)
        .await
        .map_err(|e| match e {
            ScanLimitError::LimitReached { limit } => AnalyzeError::ScanLimitReached(limit),
            ScanLimitError::FreeLimitReached { limit, resets_on } => {
                AnalyzeError::FreeScanLimitReached { limit, resets_on }
            }
            ScanLimitError::Unavailable => {
                AnalyzeError::Database("Could not check your scan allowance".to_string())
            }
        })?;

    Ok(user_id)
}

/// Every time someone tries to use /analyze, this checks their notebook entry, lets them
/// through and counts it, unless they've already hit 10 within the last 5 minutes, in which
/// case it tells them exactly how many seconds until they can try again.
///
/// Gets the notebook (creating it, if this is the very first time), finds this person's
/// page in the notebook — or creates one, if they've never called before, checks
/// if their 5-minute window has already run out and counts the current request,
/// checks if they're still under the limit. If they've gone over and calculate
/// exactly how long they need to wait.
pub fn check_rate_limit(user_id: Uuid) -> Result<(), AnalyzeError> {
    let map = RATE_LIMITS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = map.lock().expect("expected to lock the map");
    let now = Instant::now();
    let entry = map.entry(user_id).or_insert((0, now));

    if now.duration_since(entry.1) > RATE_LIMIT_WINDOW {
        entry.0 = 0;
        entry.1 = now;
    }
    entry.0 += 1;

    if entry.0 <= RATE_LIMIT_MAX_REQUESTS {
        Ok(())
    } else {
        let elapsed = now.duration_since(entry.1);
        let remaining = RATE_LIMIT_WINDOW.saturating_sub(elapsed);
        Err(AnalyzeError::RateLimited(remaining.as_secs()))
    }
}

/// It takes the one big request that arrives from your extension, and splits it into two separate,
/// smaller pieces. One containing just the seller's information, and one containing just
/// the listing's information.
///
/// It builds the seller-specific piece, builds the listing-specific piece
/// and returns both pieces together, as a pair.
pub fn build_requests(r: &AnalyzeRequest) -> (SellersRequest, ListingsRequest) {
    let seller_request = SellersRequest {
        platform: r.platform.clone(),
        platform_id: r.platform_id.clone(),
        name: r.seller_name.clone(),
        handle: r.seller_handle.clone(),
        phone: r.seller_phone.clone(),
        profile_url: r.seller_profile_url.clone(),
        join_date: r.seller_join_date.clone(),
        location: r.seller_location.clone(),
        last_active: r.seller_last_active.clone(),
    };

    let listing_request = ListingsRequest {
        seller_id: r.seller_id,
        platform: r.platform.clone(),
        listing_url: r.listing_url.clone(),
        listing_id: r.listing_id.clone(),
        title: r.title.clone(),
        price: r.price,
        description: r.description.clone(),
        category: r.category.clone(),
        image_urls: r.image_urls.clone(),
        posted_date: r.posted_date.clone(),
    };

    (seller_request, listing_request)
}

/// Before writing anything to the database, first check if this seller already has fraud
/// reports against them so their very first database record is already correct, never briefly,
/// incorrectly saying 'Unknown' about someone who's already known to be a problem.
///
/// It checks if this seller already exists in the database. If they already exist,
/// checks how many fraud reports they have — before doing anything else, decides their
/// verification status, based on that count, creates (or updates) the seller row,
/// using that correctly-determined verification, counts their fraud reports again,
/// this time for the real, final result.
pub async fn resolve_seller(
    pool: &Pool<Postgres>,
    seller_req: &SellersRequest,
    platform: &str,
    platform_id: &str,
) -> Result<ResolvedSeller, AnalyzeError> {
    let existing_seller = find_seller(pool, platform, platform_id)
        .await
        .map_err(|e| AnalyzeError::Database(e.to_string()))?;

    let preliminary_fraud_count = if let Some(ref s) = existing_seller {
        count_fraud_reports(pool, s.id)
            .await
            .map_err(|e| AnalyzeError::Database(e.to_string()))?
    } else {
        0
    };

    let verification = if preliminary_fraud_count > 0 {
        SellerVerification::Reported
    } else {
        SellerVerification::Unknown
    };

    let seller = create_seller(pool, seller_req, verification)
        .await
        .map_err(|e| AnalyzeError::Database(e.to_string()))?;

    let fraud_count = count_fraud_reports(pool, seller.id)
        .await
        .map_err(|e| AnalyzeError::Database(e.to_string()))?;

    let network_summary = build_network_summary(fraud_count);

    Ok(ResolvedSeller {
        seller,
        fraud_count,
        network_summary,
    })
}

/// It sends the listing's real details to Claude, asking it to analyze
/// whether this looks like a genuine or fraudulent listing by filling in
/// sensible defaults for anything that's missing, so Claude always gets
/// something usable, even if the original listing had gaps.
///
/// It calculates the seller's account age, from their real join date, gets
/// the listing's images, or an empty list if there are none, actually calls
/// Claude, with everything it needs and it waits for Claude's response,
/// and handles failure clearly
pub async fn run_claude_analysis(
    listing: &Listings,
    seller: &Sellers,
    language: &str,
) -> Result<ClaudeAnalysis, AnalyzeError> {
    let account_age = seller
        .join_date
        .map(format_account_age)
        .unwrap_or_else(|| "Unknown".to_string());

    let image_urls = listing.image_urls.as_deref().unwrap_or(&[]);

    call_b2c_claude(CallClaudeArguments {
        platform: &listing.platform,
        seller_name: seller.name.as_deref().unwrap_or("Unknown"),
        seller_account_age: &account_age,
        title: listing.title.as_deref().unwrap_or("Untitled"),
        price: listing.price.unwrap_or(0),
        description: listing.description.as_deref().unwrap_or("No Description"),
        image_urls,
        language,
    })
    .await
    .map_err(|e| AnalyzeError::ClaudeAnalysisFailed(e.to_string()))
}

/// It builds the complete list of warning signals shown on the dashboard
/// starting with everything Claude's analysis found, then adding a domain-mismatch
/// check at the very top, if one was detected.
///
/// It builds the main signal list from Claude's analysis, checks if a domain
/// mismatch was detected and returns the complete list.
pub async fn build_all_signals(
    pool: &Pool<Postgres>,
    claude_analysis: &ClaudeAnalysis,
    seller: &Sellers,
    request: &AnalyzeRequest,
) -> Vec<Signal> {
    let mut signals = build_signals(claude_analysis, seller);

    if let Some(domain_signal) = build_domain_signal(
        request.domain_check_status.as_deref(),
        request.domain_check_real_name.as_deref(),
        request.domain_check_real_domain.as_deref(),
        request.domain_check_current_domain.as_deref(),
        request.domain_check_current_html.as_deref(),
        request.domain_check_real_html.as_deref(),
    ) {
        signals.insert(0, domain_signal);
    }

    if let Some(memory_signal) = build_network_memory_signal(pool, seller.id).await {
        signals.push(memory_signal);
    }

    // Layer 3, Active collection - if the seller mentioned a real
    // website, check its genuine registration via WHOIS. Most
    // listings won't have one at all, so this only fires when Tier
    // 1's extraction actually found something real.
    if let Some(website) = request.seller_website.as_deref() {
        let whois_result = check_domain_whois(website).await;
        if let Some(whois_signal) = build_whois_signal(whois_result.as_ref()) {
            signals.push(whois_signal);
        }
    } else {
        signals.push(Signal {
            label: "Seller website check".to_string(),
            sub: "No website was mentioned or claimed by this seller.".to_string(),
            value: "No website found".to_string(),
            signal_type: "info".to_string(),
            category: "website".to_string(),
            check_type: "existence".to_string(),
        });
    }

    // Genuinely free, real trust data for OLX's verified-seller
    // accounts - member duration, listing count, real rating -
    // already present on the listing page for these sellers.
    signals.extend(build_seller_verification_signals(
        request.seller_verified.unwrap_or(false),
        request.seller_rating,
        request.seller_total_products,
    ));

    // Tier 2 - visits the seller's own, separate store/profile page,
    // confirming whether their real name genuinely appears there, and
    // checking for any self-referenced website mentioned on that page.
    if let Some(profile_url) = request.seller_profile_url.as_deref() {
        let seller_name = request.seller_name.as_deref().unwrap_or("");
        if let Some(store_result) =
            check_store_page(&request.platform, profile_url, seller_name).await
        {
            if let Some(store_signal) =
                build_store_page_signal(&store_result, request.seller_website.as_deref())
            {
                signals.push(store_signal);
            }
        }
    } else {
        signals.push(Signal {
            label: "Store page check".to_string(),
            sub: "No separate store or profile page was found for this seller.".to_string(),
            value: "No store page found".to_string(),
            signal_type: "info".to_string(),
            category: "identity".to_string(),
            check_type: "consistency".to_string(),
        });
    }

    signals
}

/// It saves the complete analysis result to the database, fetches the seller's
/// real visit-history chart, and packages everything together into the final
/// response the extension actually receives.
///
/// It converts the signals list into a format the database can store, actually
/// saves the analysis to the database, fetches the seller's real visit history,
/// for the chart, builds the seller portion of the response, assembles and returns
/// the complete, final response.
pub async fn save_and_build_response(
    data: BuildResponseData<'_>,
) -> Result<Json<AnalyzeResponse>, AnalyzeError> {
    let signals_json =
        to_value(&data.signals).map_err(|e| AnalyzeError::SerializationFailed(e.to_string()))?;

    let entity_type = if data.is_b2b {
        "business".to_string()
    } else {
        let website_fully_confirmed = data
            .signals
            .iter()
            .any(|s| s.label == "Store page check" && s.value == "Fully confirmed");
        classify_entity(data.seller.name.as_deref(), website_fully_confirmed)
    };
    let (confidence_level, confidence_reasoning) = calculate_confidence(&data.signals);

    let mut risk_factors = derive_risk_factors(&data.signals);
    // B2B: when Safely could check too little, say so (the score was
    // already lifted to Moderate in build_b2b_analysis_path).
    if data.is_b2b {
        if let Some(note) = not_enough_information_factor(&data.signals) {
            risk_factors.push(note);
        }
    }
    let risk_factors_json =
        to_value(&risk_factors).map_err(|e| AnalyzeError::SerializationFailed(e.to_string()))?;

    let social_candidates_json = to_value(&data.social_candidates)
        .map_err(|e| AnalyzeError::SerializationFailed(e.to_string()))?;
    let saved_analysis = create_analysis(CreateAnalysisData {
        pool: data.pool,
        listing_id: data.listing_id,
        risk_score: data.risk_score,
        risk_level: data.risk_level,
        signals: signals_json,
        network_summary: data.overall_risk_notes.clone(),
        claude_raw: String::new(),
        user_id: data.user_id,
        confidence_level: confidence_level.clone(),
        confidence_reasoning: confidence_reasoning.clone(),
        risk_factors: risk_factors_json,
        social_candidates: social_candidates_json,
    })
    .await
    .map_err(|e| AnalyzeError::Database(e.to_string()))?;

    record_evidence(
        data.pool,
        saved_analysis.id,
        data.seller.id,
        &data.signals,
        data.risk_score,
    )
    .await;

    record_risk_factors(data.pool, saved_analysis.id, data.seller.id, &risk_factors).await;

    let monthly_activity = get_monthly_visit_activity(data.pool, data.listing_id)
        .await
        .unwrap_or_else(|_| vec![0i32; 12]);

    let mut seller_response = SellersResponse::from(data.seller);
    // B2B: show the same company age as the "Account age" check
    // ("About 11 years"), not one counted in months from 1 January -
    // only the founding year is known.
    if data.is_b2b {
        if let Some(age) = b2b_company_age(&data.signals) {
            seller_response.account_age = age;
        }
    }
    seller_response.network_summary = data.network_summary;
    seller_response.monthly_activity = monthly_activity;

    Ok(Json(AnalyzeResponse {
        analysis_id: saved_analysis.id,
        risk_score: saved_analysis.risk_score,
        risk_level: saved_analysis.risk_level,
        seller: seller_response,
        signals: data.signals,
        network_summary: data.overall_risk_notes,
        fraud_report_count: data.fraud_count,
        entity_type,
        confidence_level,
        confidence_reasoning,
        risk_factors,
        social_candidates: data.social_candidates,
    }))
}

// This only ever fails for one genuine reason - a real database
// problem (e.g. a bad foreign key on listing_id/user_id) - so plain
// sqlx::Error is honest here; a custom error type isn't needed.
pub async fn create_analysis(data: CreateAnalysisData<'_>) -> Result<Analysis, Error> {
    let id = Uuid::now_v7();
    let analysis = query_as::<_, Analysis>(
        "
        INSERT INTO analysis (
            id,
            listing_id,
            risk_score,
            risk_level,
            signals,
            network_summary,
            claude_raw,
            user_id,
            confidence_level,
            confidence_reasoning,
            risk_factors,
            social_candidates,
            created_at
        )
        VALUES (
            $1,  $2,  $3,  $4,   $5,
            $6,  $7,  $8,  $9,   $10, $11, $12, NOW()
        )
        RETURNING *
        ",
    )
    .bind(id)
    .bind(&data.listing_id)
    .bind(&data.risk_score)
    .bind(&data.risk_level)
    .bind(&data.signals)
    .bind(&data.network_summary)
    .bind(&data.claude_raw)
    .bind(&data.user_id)
    .bind(&data.confidence_level)
    .bind(&data.confidence_reasoning)
    .bind(&data.risk_factors)
    .bind(&data.social_candidates)
    .fetch_one(data.pool)
    .await?;

    Ok(analysis)
}

/// Applies the client-side website fallback (some platforms, e.g.
/// TradeWheel, only reveal a supplier's real website to a logged-in
/// visitor, so the anonymous server-side fetch genuinely can't see
/// it) and builds the resulting "Seller website check" signal - never
/// overwrites a website the server-side fetch already found on its
/// own. Pulled out as a pure function, with no database/Claude/Serper
/// dependency, specifically so this logic can be tested directly.
pub fn resolve_supplier_website(
    mut supplier: B2bSupplierProfile,
    client_website: Option<&str>,
) -> (B2bSupplierProfile, Signal) {
    if supplier.website_url.is_none() {
        if let Some(website) = client_website {
            if !website.is_empty() {
                supplier.website_url = Some(website.to_string());
            }
        }
    }

    let signal = match supplier.website_url.as_deref() {
        Some(url) => Signal {
            label: "Seller website check".to_string(),
            sub: format!("This supplier's website was found: {}", url),
            value: "Website found".to_string(),
            signal_type: "info".to_string(),
            category: "website".to_string(),
            check_type: "existence".to_string(),
        },
        None => Signal {
            label: "Seller website check".to_string(),
            sub: "No website was found for this supplier on this platform.".to_string(),
            value: "No website found".to_string(),
            signal_type: "info".to_string(),
            category: "website".to_string(),
            check_type: "existence".to_string(),
        },
    };

    (supplier, signal)
}

/// Same category of problem as resolve_supplier_website - b2bmap only
/// ever shows a masked phone number to an anonymous fetch (e.g.
/// "+8618217xxxxx"), so a server-side scrape can look like it found a
/// phone that isn't actually usable. Prefers the server-scraped
/// number when it doesn't look masked; otherwise falls back to
/// whatever the client (a genuinely signed-in visitor) read directly
/// off the page. If neither is real, drops it entirely rather than
/// analyzing or displaying a masked value as if it were genuine.
pub fn resolve_supplier_phone(
    mut supplier: B2bSupplierProfile,
    client_phone: Option<&str>,
) -> B2bSupplierProfile {
    let scraped_is_usable = supplier
        .contact_phone
        .as_deref()
        .map(|p| !looks_masked(p))
        .unwrap_or(false);

    if !scraped_is_usable {
        let usable_client_phone = client_phone
            .map(|p| p.trim())
            .filter(|p| !p.is_empty() && !looks_masked(p));

        supplier.contact_phone = usable_client_phone.map(|p| p.to_string());
    }

    supplier
}

fn looks_masked(value: &str) -> bool {
    value.contains('*') || value.to_lowercase().contains('x')
}

/// It makes sure "Contact info" never disagrees with "Username" — if this scan's
/// server-side scrape didn't find a contact name (the enrichment fetch can fail
/// independently of the main page fetch), it falls back to the same handle the
/// extension already scraped client-side and that "Username" displays, instead
/// of telling Claude no name was found when one actually is known.
pub fn resolve_supplier_contact_name(
    mut supplier: B2bSupplierProfile,
    client_handle: Option<&str>,
) -> B2bSupplierProfile {
    if supplier.contact_name.is_none() {
        let usable_handle = client_handle.map(|h| h.trim()).filter(|h| !h.is_empty());
        supplier.contact_name = usable_handle.map(|h| h.to_string());
    }
    supplier
}

/// Same idea as resolve_supplier_contact_name, for the founding year.
/// A scan doesn't always find the year (Alibaba serves more than one
/// page layout), but an earlier scan of the same seller may have saved
/// it. Falls back to that saved year so "Account age" never shows
/// "Not provided" when Safely already knows the answer. A year found by
/// this scan always wins over the saved one.
pub fn resolve_supplier_year(
    mut supplier: B2bSupplierProfile,
    saved_join_date: Option<NaiveDate>,
) -> B2bSupplierProfile {
    let has_year = supplier
        .year_established
        .as_deref()
        .map_or(false, |y| !y.trim().is_empty());
    if !has_year {
        if let Some(date) = saved_join_date {
            supplier.year_established = Some(date.year().to_string());
        }
    }
    supplier
}

/// The company age from the B2B "Account age" check, when it has one
/// (not "Not provided" / "Invalid date").
fn b2b_company_age(signals: &[Signal]) -> Option<String> {
    signals
        .iter()
        .find(|s| s.label == "Account age")
        .map(|s| s.value.clone())
        .filter(|v| v != "Not provided" && v != "Invalid date")
}

/// Points one warning adds to a B2B score. Strong scam signs count
/// more, weak ones less (every warning used to count the same 15):
/// - 25: a price that doesn't add up, contact details that can't be
///   confirmed, payment demands, a company that doesn't look real, and
///   the company's name found next to "scam" words online;
/// - 10: no verified badge, no order details at all, a hidden founding
///   year - common on honest listings too;
/// - 15: every other warning.
/// Only "caution" results count here; "bad" results (fake website,
/// scam reports) are handled by the risk factors instead.
fn warning_points(signal: &Signal) -> i16 {
    if signal.signal_type != "caution" {
        return 0;
    }
    match signal.label.as_str() {
        "Price analysis"
        | "Contact info"
        | "Advance payment request"
        | "Overall legitimacy check" => 25,
        "Social presence check" if signal.value == SCAM_MENTIONS_FOUND => 25,
        "Platform verification" | "Listing completeness" => 10,
        "Account age" if signal.value == "Not provided" => 10,
        _ => 15,
    }
}

/// Points each "info" card adds to a B2B score. Info is not a warning,
/// but it is not good news either (photos not confirmed, a risky
/// payment option listed, a company page that would not load...), so
/// it adds a little. Capped so info alone can never leave Low risk:
/// a supplier with only info cards scores at most 15.
const INFO_POINTS: i16 = 5;
const INFO_POINTS_MAX: i16 = 15;

fn info_points(signals: &[Signal]) -> i16 {
    let count = signals.iter().filter(|s| s.signal_type == "info").count() as i16;
    (count * INFO_POINTS).min(INFO_POINTS_MAX)
}

/// The lowest score a B2B scan shows when Safely could check too little
/// about the supplier (see not_enough_information_factor): the start of
/// Moderate, so "nothing found" never reads as "Low risk".
const NOT_ENOUGH_INFORMATION_FLOOR: i16 = 34;

/// B2B risk score: warnings give the base score, and any "Serious"
/// risk factor lifts it to at least the High band (67), +10 for each
/// extra one. A "Pattern match" (compound) factor, e.g. full prepayment
/// to a brand-new company, lifts it to at least the Caution band (45),
/// +10 for each extra one, but never into High on its own (max 66).
/// Capped at 100.
fn b2b_risk_score(base_score: i16, serious_count: i16, compound_count: i16) -> i16 {
    let floor = if serious_count > 0 {
        67 + (serious_count - 1) * 10
    } else if compound_count > 0 {
        (45 + (compound_count - 1) * 10).min(66)
    } else {
        0
    };
    base_score.max(floor).min(100)
}

/// Serper (web search for the social presence check) is only called
/// when SERPER_ENABLED is set to "true" / "1". Default: off.
fn serper_enabled() -> bool {
    std::env::var("SERPER_ENABLED")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(false)
}

/// Clean-ups that apply to EVERY B2B platform (Alibaba, ExportHub,
/// TradeWheel, B2Brazil, B2BMap, ThomasNet, Kompass and any added
/// later), done once here instead of in each scraper:
/// - a price with no number in it ("Depends", "Negotiable", "Contact
///   us") is not a price, so it is not counted as provided;
/// - "<br />" typed into a seller's text is removed (the line break
///   itself is kept).
fn clean_b2b_listing(mut listing: B2bListingProfile) -> B2bListingProfile {
    listing.unit_price = listing.unit_price.filter(|p| has_number(p));
    listing.fob_price = listing.fob_price.filter(|p| has_number(p));
    listing.description = listing.description.map(|d| remove_typed_line_breaks(&d));
    listing
}

fn clean_b2b_supplier(mut supplier: B2bSupplierProfile) -> B2bSupplierProfile {
    supplier.company_description = supplier
        .company_description
        .map(|d| remove_typed_line_breaks(&d))
        .filter(|d| !d.is_empty());
    supplier
}

/// True when the text holds at least one digit ("36 - 40 USD").
fn has_number(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_digit())
}

/// "First.<br />\n<br />\nSecond." -> "First.\n\nSecond." - typed tags
/// become line breaks, and blank lines are kept to at most one.
fn remove_typed_line_breaks(text: &str) -> String {
    let mut out = text.to_string();
    for tag in ["<br />", "<br/>", "<br>", "<BR />", "<BR/>", "<BR>"] {
        out = out.replace(tag, "\n");
    }
    let mut lines: Vec<&str> = Vec::new();
    for line in out.lines().map(str::trim_end) {
        if line.trim().is_empty() && lines.last().is_some_and(|l| l.trim().is_empty()) {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n").trim().to_string()
}

/// What Claude sees as the price: the unit price and, when the listing
/// also shows one, the FOB price. TradeWheel often shows the real
/// per-unit price ("60 - 80 USD / Carat") only in the FOB line.
fn price_for_claude(listing: &B2bListingProfile) -> String {
    let unit = listing
        .unit_price
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let fob = listing
        .fob_price
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match (unit, fob) {
        (Some(u), Some(f)) if u != f => format!("{u} (FOB price: {f})"),
        (Some(u), _) => u.to_string(),
        (None, Some(f)) => format!("FOB price: {f}"),
        (None, None) => String::new(),
    }
}

/// The complete, separate B2B analysis path - takes the already-fetched
/// supplier page (analyze.rs fetches it first, so it can pick the
/// seller record by company), calls Claude with B2B-specific due-diligence
/// questions, and builds an entirely separate set of signals. This
/// never touches build_signals or ClaudeAnalysis at all, since B2B
/// due diligence asks fundamentally different questions than
/// consumer-marketplace fraud detection.
pub async fn build_b2b_analysis_path(
    pool: &Pool<Postgres>,
    request: &AnalyzeRequest,
    fraud_count: i64,
    seller_id: Uuid,
    known_seller_handle: Option<&str>,
    known_join_date: Option<NaiveDate>,
    supplier: B2bSupplierProfile,
    listing: B2bListingProfile,
    record: Option<SupplierRecord>,
) -> Result<
    (
        Vec<Signal>,
        i16,
        String,
        B2bSupplierProfile,
        B2bListingProfile,
        Vec<PlatformCheckResult>,
    ),
    AnalyzeError,
> {
    // Same clean-up for every platform (see clean_b2b_listing).
    let supplier = clean_b2b_supplier(supplier);
    let listing = clean_b2b_listing(listing);

    let mut signals = Vec::new();

    if let Some(memory_signal) = build_network_memory_signal(pool, seller_id).await {
        signals.push(memory_signal);
    }

    if let Some(domain_signal) = build_domain_signal(
        request.domain_check_status.as_deref(),
        request.domain_check_real_name.as_deref(),
        request.domain_check_real_domain.as_deref(),
        request.domain_check_current_domain.as_deref(),
        request.domain_check_current_html.as_deref(),
        request.domain_check_real_html.as_deref(),
    ) {
        signals.push(domain_signal);
    }

    let (supplier, website_signal) =
        resolve_supplier_website(supplier, request.seller_website.as_deref());
    signals.push(website_signal);

    let supplier = resolve_supplier_phone(supplier, request.seller_phone.as_deref());
    let supplier = resolve_supplier_contact_name(supplier, known_seller_handle);
    let supplier = resolve_supplier_year(supplier, known_join_date);

    let price_text = price_for_claude(&listing);

    let claude_result = call_b2b_claude(CallB2bClaudeArguments {
        platform: &request.platform,
        company_name: supplier.company_name.as_deref().unwrap_or("Unknown"),
        year_established: supplier.year_established.as_deref().unwrap_or("Unknown"),
        platform_verified: supplier.platform_verified_badge,
        employee_count: supplier.employee_count.as_deref().unwrap_or("Unknown"),
        company_description: supplier.company_description.as_deref().unwrap_or(""),
        contact_name: supplier.contact_name.as_deref().unwrap_or(""),
        contact_phone: supplier.contact_phone.as_deref().unwrap_or(""),
        website_url: supplier.website_url.as_deref().unwrap_or(""),
        product_title: listing.title.as_deref().unwrap_or("Unknown"),
        product_description: listing.description.as_deref().unwrap_or("None provided"),
        image_urls: &listing.image_urls,
        language: request.language.as_deref().unwrap_or("en"),
        unit_price: &price_text,
        minimum_order_quantity: listing.minimum_order_quantity.as_deref().unwrap_or(""),
        payment_type: listing.payment_type.as_deref().unwrap_or(""),
    })
    .await
    .map_err(|e| AnalyzeError::ClaudeAnalysisFailed(e.to_string()))?;

    signals.extend(build_b2b_claude_signals(&claude_result));
    // Always warn about Western Union / MoneyGram / crypto / gift cards
    // in the listing's payment methods, even when Claude did not flag them.
    apply_risky_payment_note(&mut signals, listing.payment_type.as_deref());
    signals.push(build_b2b_verification_signal(&supplier));
    signals.push(build_b2b_company_age_signal(&supplier));
    // The platform's own record (Alibaba: on-site check, paid
    // membership, years on Alibaba, orders, rating). Corrects the two
    // cards above and adds "Seller track record". None elsewhere.
    apply_supplier_record(&mut signals, record.as_ref());
    signals.push(build_b2b_transparency_signal(&supplier));
    signals.push(build_b2b_listing_completeness_signal(&listing));
    // Social presence search runs on Serper, which costs credits. It is
    // OFF unless SERPER_ENABLED=true is set in the environment, so a
    // scan never calls Serper by accident. When off, the "Social
    // presence check" row is simply left out of the scan.
    let social_candidates = if serper_enabled() {
        let (social_presence_signal, social_candidates) = build_social_presence_matrix(
            supplier.company_name.as_deref(),
            supplier.contact_name.as_deref(),
            supplier.country.as_deref(),
            supplier.contact_phone.as_deref(),
            &request.platform,
        )
        .await
        .map_err(AnalyzeError::ClaudeAnalysisFailed)?;
        signals.push(social_presence_signal);
        social_candidates
    } else {
        Vec::new()
    };

    let warning_score: i16 =
        signals.iter().map(warning_points).sum::<i16>() + info_points(&signals);
    let base_score = warning_score.min(100) + (fraud_count as i16 * 5).min(20);
    // Counting warnings alone treats a Western Union demand the same as
    // a missing field. Any "Serious" risk factor (legitimacy concern,
    // unsafe payment terms, a seller Safely already scored high-risk)
    // puts the scan in the High band (67+), plus 10 for each extra one.
    let factors = derive_risk_factors(&signals);
    let serious_count = factors.iter().filter(|f| f.severity == "hard").count() as i16;
    let compound_count = factors.iter().filter(|f| f.severity == "compound").count() as i16;
    let mut risk_score = b2b_risk_score(base_score, serious_count, compound_count);
    if not_enough_information_factor(&signals).is_some() {
        risk_score = risk_score.max(NOT_ENOUGH_INFORMATION_FLOOR);
    }

    let overall_risk_notes = claude_result.overall_risk_notes.clone();
    Ok((
        signals,
        risk_score.min(100),
        overall_risk_notes,
        supplier,
        listing,
        social_candidates,
    ))
}

#[cfg(test)]
mod b2b_score_tests {
    use super::{b2b_risk_score, info_points, warning_points};
    use crate::models::analysis::Signal;

    fn sig(label: &str, value: &str, signal_type: &str) -> Signal {
        Signal {
            label: label.to_string(),
            sub: String::new(),
            value: value.to_string(),
            signal_type: signal_type.to_string(),
            category: String::new(),
            check_type: String::new(),
        }
    }

    #[test]
    fn strong_warnings_count_more_than_weak_ones() {
        assert_eq!(
            warning_points(&sig("Price analysis", "suspiciously low", "caution")),
            25
        );
        assert_eq!(
            warning_points(&sig("Contact info", "Not confirmed", "caution")),
            25
        );
        assert_eq!(
            warning_points(&sig("Platform verification", "Unverified", "caution")),
            10
        );
        assert_eq!(
            warning_points(&sig(
                "Listing completeness",
                "0/9 fields provided",
                "caution"
            )),
            10
        );
        assert_eq!(
            warning_points(&sig("Account age", "Not provided", "caution")),
            10
        );
        assert_eq!(
            warning_points(&sig("Account age", "Founded this year", "caution")),
            15
        );
        assert_eq!(
            warning_points(&sig("Listing detail", "Vague", "caution")),
            15
        );
        assert_eq!(
            warning_points(&sig(
                "Social presence check",
                super::SCAM_MENTIONS_FOUND,
                "caution"
            )),
            25
        );
        assert_eq!(
            warning_points(&sig(
                "Social presence check",
                "No presence found",
                "caution"
            )),
            15
        );
    }

    #[test]
    fn only_warnings_add_points() {
        assert_eq!(warning_points(&sig("Price analysis", "normal", "good")), 0);
        assert_eq!(
            warning_points(&sig("Account age", "Not provided", "info")),
            0
        );
        assert_eq!(warning_points(&sig("Domain check", "Suspicious", "bad")), 0);
    }

    #[test]
    fn a_too_cheap_price_and_a_vague_listing_now_reach_moderate() {
        let total: i16 = [
            sig("Price analysis", "suspiciously low", "caution"),
            sig("Listing detail", "Vague", "caution"),
        ]
        .iter()
        .map(warning_points)
        .sum();
        assert_eq!(total, 40, "was 30 (Low) when every warning counted 15");
        assert!(total >= 34);
    }

    #[test]
    fn info_cards_add_a_little_but_never_leave_low_risk() {
        let info = |n: usize| -> Vec<Signal> {
            (0..n)
                .map(|_| sig("Image authenticity", "not verified", "info"))
                .collect()
        };
        assert_eq!(info_points(&info(0)), 0);
        assert_eq!(info_points(&info(1)), 5);
        assert_eq!(info_points(&info(3)), 15);
        assert_eq!(info_points(&info(9)), 15, "capped, so still Low risk");
        let mixed = vec![
            sig("Price analysis", "normal", "good"),
            sig("Account age", "Not provided", "caution"),
            sig("Image authenticity", "not verified", "info"),
        ];
        assert_eq!(info_points(&mixed), 5, "only info cards count");
    }

    #[test]
    fn no_serious_factor_keeps_the_warning_score() {
        assert_eq!(b2b_risk_score(15, 0, 0), 15);
        assert_eq!(b2b_risk_score(60, 0, 0), 60);
    }

    #[test]
    fn serious_factors_reach_the_high_band() {
        assert_eq!(b2b_risk_score(15, 1, 0), 67);
        assert_eq!(b2b_risk_score(60, 3, 0), 87);
        assert_eq!(b2b_risk_score(90, 1, 0), 90);
        assert_eq!(b2b_risk_score(100, 6, 0), 100);
        assert_eq!(
            b2b_risk_score(15, 1, 1),
            67,
            "serious wins over a pattern match"
        );
    }

    #[test]
    fn pattern_matches_reach_the_caution_band_but_not_high() {
        assert_eq!(b2b_risk_score(30, 0, 1), 45);
        assert_eq!(b2b_risk_score(30, 0, 2), 55);
        assert_eq!(b2b_risk_score(30, 0, 5), 66);
        assert_eq!(
            b2b_risk_score(75, 0, 1),
            75,
            "a higher warning score is kept"
        );
    }
}

#[cfg(test)]
mod b2b_cleanup_tests {
    use super::*;

    #[test]
    fn word_only_prices_are_dropped_on_every_platform() {
        for platform in [
            "alibaba",
            "exporthub",
            "tradewheel",
            "b2brazil",
            "b2bmap",
            "thomasnet",
            "kompass",
        ] {
            let listing = B2bListingProfile {
                unit_price: Some("Negotiable".into()),
                fob_price: Some("36 - 40 USD / Depends on the quantity".into()),
                source_platform: platform.into(),
                ..Default::default()
            };
            let l = clean_b2b_listing(listing);
            assert_eq!(l.unit_price, None, "{platform}");
            assert_eq!(
                l.fob_price.as_deref(),
                Some("36 - 40 USD / Depends on the quantity")
            );
        }
    }

    #[test]
    fn typed_line_breaks_are_removed_everywhere() {
        let supplier = B2bSupplierProfile {
            company_description: Some("First.<br />\n<br />\n\nSecond.".into()),
            ..Default::default()
        };
        assert_eq!(
            clean_b2b_supplier(supplier).company_description.as_deref(),
            Some("First.\n\nSecond.")
        );
        let listing = B2bListingProfile {
            description: Some("A<br>B".into()),
            ..Default::default()
        };
        assert_eq!(
            clean_b2b_listing(listing).description.as_deref(),
            Some("A\nB")
        );
    }
}

#[cfg(test)]
mod b2b_company_age_tests {
    use super::b2b_company_age;
    use crate::models::analysis::Signal;

    fn age(value: &str) -> Signal {
        Signal {
            label: "Account age".to_string(),
            sub: String::new(),
            value: value.to_string(),
            signal_type: "good".to_string(),
            category: "company".to_string(),
            check_type: "anomaly".to_string(),
        }
    }

    #[test]
    fn uses_the_account_age_check_value() {
        assert_eq!(
            b2b_company_age(&[age("About 11 years")]).as_deref(),
            Some("About 11 years")
        );
        assert_eq!(
            b2b_company_age(&[age("Founded this year")]).as_deref(),
            Some("Founded this year")
        );
    }

    #[test]
    fn keeps_the_old_value_when_no_year_is_known() {
        assert_eq!(b2b_company_age(&[age("Not provided")]), None);
        assert_eq!(b2b_company_age(&[]), None);
    }
}
