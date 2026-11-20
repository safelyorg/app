//! Vendor checks (services/features/vendors.rs).
//!
//! The buyer (signed in, Team or Enterprise):
//! - GET  /api/v1/vendors?who=me|team|<user id>   the list (like History)
//! - POST /api/v1/vendors/invite           {buyer_company, language, send_email, vendors: [...]}
//! - GET  /api/v1/vendors/{id}             one vendor's answers and result
//! - POST /api/v1/vendors/{id}/remind      email the vendor again
//! - POST /api/v1/vendors/{id}/link        a new link (the old one stops)
//! - POST /api/v1/vendors/{id}/cancel      cancel the link
//! - POST /api/v1/vendors/{id}/decision    {decision: "approve" | "reject", note}
//! - POST /api/v1/vendors/{id}/ask         {fields, note, reason: "more_info" | "bank_change"}
//! - POST /api/v1/vendors/{id}/watch       add to Watched suppliers
//! - POST /api/v1/vendors/{id}/retry       run a waiting check now
//! - POST /api/v1/vendors/{id}/report      {report_type, description} scam report
//! - GET  /api/v1/vendors/{id}/documents/{doc_id}   download a vendor's file
//! - GET  /api/v1/vendors/{id}/pdf         the result as a PDF report
//!
//! The vendor (no account, only the link's secret):
//! - GET  /api/v1/vendor-form/{token}
//! - POST /api/v1/vendor-form/{token}/draft          {answers}
//! - POST /api/v1/vendor-form/{token}/send-code      {email}
//! - POST /api/v1/vendor-form/{token}/confirm-code   {code}
//! - POST /api/v1/vendor-form/{token}/submit         {answers}
//! - POST /api/v1/vendor-form/{token}/document?kind=registration|bank_letter
//!   (multipart, one file: PDF, JPG, PNG or WebP, at most 5 MB)

use crate::services::features::auth::extract_user_id;
use crate::services::features::team_history::HistoryWho;
use crate::services::features::vendor_pdf::generate_vendor_pdf;
use crate::services::features::vendors::{
    AskRequest, DecisionRequest, InviteRequest, ReportRequest, VendorError, ask_vendor, can_retry,
    cancel_vendor, confirm_email_code, decide, download_document, invite_vendors, new_vendor_link,
    open_form, remind_vendor, report_vendor, run_check, save_draft, send_email_code, submit_form,
    team_for, upload_document, vendor_detail, vendor_list, watch_vendor,
};
use axum::{
    Json,
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

type Reply = (StatusCode, Json<Value>);

async fn signed_in(pool: &Pool<Postgres>, headers: &HeaderMap) -> Result<Uuid, Reply> {
    match extract_user_id(headers, pool).await {
        Ok(Some(id)) => Ok(id),
        Ok(None) => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthorized" })),
        )),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "internal_error" })),
        )),
    }
}

fn failed(e: VendorError) -> Reply {
    let status = match e {
        VendorError::NeedsPaidPlan => StatusCode::FORBIDDEN,
        VendorError::NotFound => StatusCode::NOT_FOUND,
        VendorError::LinkClosed => StatusCode::GONE,
        VendorError::WrongStatus | VendorError::WatchLimit(_) | VendorError::AlreadyReported => {
            StatusCode::CONFLICT
        }
        VendorError::BadFile => StatusCode::BAD_REQUEST,
        VendorError::CodeWait | VendorError::CodeTooManyTries | VendorError::EmailLimit => {
            StatusCode::TOO_MANY_REQUESTS
        }
        VendorError::BadInput(_)
        | VendorError::CodeWrong
        | VendorError::CodeExpired
        | VendorError::EmailNotConfirmed => StatusCode::BAD_REQUEST,
        VendorError::Database(ref m) => {
            eprintln!("Safely: vendor check request failed: {m}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };
    let limit = match e {
        VendorError::WatchLimit(n) => Some(n),
        _ => None,
    };
    (status, Json(json!({ "error": e.code(), "limit": limit })))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct WhoQuery {
    /// "me" (default), "team", or a teammate's user id.
    pub who: Option<String>,
}

pub async fn get_vendors(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Query(q): Query<WhoQuery>,
) -> Result<Json<Value>, Reply> {
    let user = signed_in(&pool, &headers).await?;
    vendor_list(&pool, user, HistoryWho::parse(q.who.as_deref()))
        .await
        .map(Json)
        .map_err(failed)
}

pub async fn post_invite(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(req): Json<InviteRequest>,
) -> Result<Json<Value>, Reply> {
    let user = signed_in(&pool, &headers).await?;
    invite_vendors(&pool, user, req)
        .await
        .map(Json)
        .map_err(failed)
}

pub async fn get_vendor(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Reply> {
    let user = signed_in(&pool, &headers).await?;
    vendor_detail(&pool, user, id)
        .await
        .map(Json)
        .map_err(failed)
}

pub async fn post_remind(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Reply> {
    let user = signed_in(&pool, &headers).await?;
    remind_vendor(&pool, user, id).await.map_err(failed)?;
    Ok(Json(json!({ "reminded": true })))
}

pub async fn post_new_link(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Reply> {
    let user = signed_in(&pool, &headers).await?;
    let link = new_vendor_link(&pool, user, id).await.map_err(failed)?;
    Ok(Json(json!({ "link": link })))
}

pub async fn post_cancel(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Reply> {
    let user = signed_in(&pool, &headers).await?;
    cancel_vendor(&pool, user, id).await.map_err(failed)?;
    Ok(Json(json!({ "canceled": true })))
}

pub async fn post_decision(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<DecisionRequest>,
) -> Result<Json<Value>, Reply> {
    let user = signed_in(&pool, &headers).await?;
    decide(&pool, user, id, req).await.map_err(failed)?;
    Ok(Json(json!({ "saved": true })))
}

pub async fn post_ask(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<AskRequest>,
) -> Result<Json<Value>, Reply> {
    let user = signed_in(&pool, &headers).await?;
    let link = ask_vendor(&pool, user, id, req).await.map_err(failed)?;
    Ok(Json(json!({ "link": link })))
}

pub async fn post_watch_vendor(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Reply> {
    let user = signed_in(&pool, &headers).await?;
    watch_vendor(&pool, user, id).await.map_err(failed)?;
    Ok(Json(json!({ "watching": true })))
}

pub async fn post_retry(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Reply> {
    let user = signed_in(&pool, &headers).await?;
    can_retry(&pool, user, id).await.map_err(failed)?;
    tokio::spawn(async move { run_check(&pool, id).await });
    Ok(Json(json!({ "started": true })))
}

// ── The vendor's form ──

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct AnswersBody {
    pub answers: Value,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct EmailBody {
    pub email: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CodeBody {
    pub code: String,
}

pub async fn get_form(
    State(pool): State<Pool<Postgres>>,
    Path(token): Path<String>,
) -> Result<Json<Value>, Reply> {
    open_form(&pool, &token).await.map(Json).map_err(failed)
}

pub async fn post_draft(
    State(pool): State<Pool<Postgres>>,
    Path(token): Path<String>,
    Json(body): Json<AnswersBody>,
) -> Result<Json<Value>, Reply> {
    save_draft(&pool, &token, &body.answers)
        .await
        .map_err(failed)?;
    Ok(Json(json!({ "saved": true })))
}

pub async fn post_send_code(
    State(pool): State<Pool<Postgres>>,
    Path(token): Path<String>,
    Json(body): Json<EmailBody>,
) -> Result<Json<Value>, Reply> {
    send_email_code(&pool, &token, &body.email)
        .await
        .map_err(failed)?;
    Ok(Json(json!({ "sent": true })))
}

pub async fn post_confirm_code(
    State(pool): State<Pool<Postgres>>,
    Path(token): Path<String>,
    Json(body): Json<CodeBody>,
) -> Result<Json<Value>, Reply> {
    let email = confirm_email_code(&pool, &token, &body.code)
        .await
        .map_err(failed)?;
    Ok(Json(json!({ "confirmed": email })))
}

pub async fn post_submit(
    State(pool): State<Pool<Postgres>>,
    Path(token): Path<String>,
    Json(body): Json<AnswersBody>,
) -> Result<Json<Value>, Reply> {
    let id = submit_form(&pool, &token, &body.answers)
        .await
        .map_err(failed)?;
    tokio::spawn(async move { run_check(&pool, id).await });
    Ok(Json(json!({ "sent": true })))
}

pub async fn post_report(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<ReportRequest>,
) -> Result<Json<Value>, Reply> {
    let user = signed_in(&pool, &headers).await?;
    report_vendor(&pool, user, id, req).await.map_err(failed)?;
    Ok(Json(json!({ "reported": true })))
}

/// The file as a download ("attachment"), never shown inside the page.
pub async fn get_document(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Path((id, doc_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, Reply> {
    let user = signed_in(&pool, &headers).await?;
    let (name, content_type, data) = download_document(&pool, user, id, doc_id)
        .await
        .map_err(failed)?;
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || ".-_ ".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut response = Response::new(Body::from(data));
    let h = response.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{safe}\"")) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct KindQuery {
    pub kind: String,
}

pub async fn post_document(
    State(pool): State<Pool<Postgres>>,
    Path(token): Path<String>,
    Query(q): Query<KindQuery>,
    mut multipart: Multipart,
) -> Result<Json<Value>, Reply> {
    let bad = || failed(VendorError::BadFile);
    let field = multipart
        .next_field()
        .await
        .map_err(|_| bad())?
        .ok_or_else(bad)?;
    let name = field.file_name().unwrap_or("document").to_string();
    let bytes = field.bytes().await.map_err(|_| bad())?;
    upload_document(&pool, &token, &q.kind, &name, &bytes)
        .await
        .map_err(failed)?;
    Ok(Json(json!({ "uploaded": true, "file_name": name })))
}

/// The vendor's result as a PDF (vendor_pdf.rs), downloaded like a
/// scan's evidence report. 409 "no_result" before the check is done.
pub async fn get_vendor_pdf(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Response, Reply> {
    let user = signed_in(&pool, &headers).await?;
    // The PDF is the full result: paid plans only (a Free plan sees the
    // short page, vendor_detail).
    team_for(&pool, user).await.map_err(failed)?;
    let detail = vendor_detail(&pool, user, id).await.map_err(failed)?;
    if detail.get("result").is_none_or(Value::is_null) {
        return Err((StatusCode::CONFLICT, Json(json!({ "error": "no_result" }))));
    }
    let pdf = generate_vendor_pdf(&detail).map_err(|e| {
        eprintln!("Safely: vendor PDF failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "internal_error" })),
        )
    })?;
    let mut response = Response::new(Body::from(pdf));
    let h = response.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/pdf"),
    );
    if let Ok(v) =
        HeaderValue::from_str(&format!("attachment; filename=\"safely-vendor-{id}.pdf\""))
    {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(response)
}
