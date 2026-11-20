//! Vendor checks (handlers/vendors.rs): the buyer's API, the vendor's
//! form API, and the vendor's form page /v/<link>
//! (site/templates/vendor_form.html). The page is the same file for
//! every link; it reads the link from the address.

use crate::handlers::vendors::{
    get_document, get_form, get_vendor, get_vendor_pdf, get_vendors, post_ask, post_cancel,
    post_confirm_code, post_decision, post_document, post_draft, post_invite, post_new_link,
    post_remind, post_report, post_retry, post_send_code, post_submit, post_watch_vendor,
};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
use sqlx::{Pool, Postgres};
use tower_http::services::ServeFile;

const FORM_PAGE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../site/templates/vendor_form.html"
);

pub fn vendors_routes() -> Router<Pool<Postgres>> {
    Router::new()
        .route("/api/v1/vendors", get(get_vendors))
        .route("/api/v1/vendors/invite", post(post_invite))
        .route("/api/v1/vendors/{id}", get(get_vendor))
        .route("/api/v1/vendors/{id}/remind", post(post_remind))
        .route("/api/v1/vendors/{id}/link", post(post_new_link))
        .route("/api/v1/vendors/{id}/cancel", post(post_cancel))
        .route("/api/v1/vendors/{id}/decision", post(post_decision))
        .route("/api/v1/vendors/{id}/ask", post(post_ask))
        .route("/api/v1/vendors/{id}/watch", post(post_watch_vendor))
        .route("/api/v1/vendors/{id}/retry", post(post_retry))
        .route("/api/v1/vendors/{id}/report", post(post_report))
        .route("/api/v1/vendors/{id}/pdf", get(get_vendor_pdf))
        .route("/api/v1/vendors/{id}/documents/{doc_id}", get(get_document))
        .route("/api/v1/vendor-form/{token}", get(get_form))
        .route("/api/v1/vendor-form/{token}/draft", post(post_draft))
        .route(
            "/api/v1/vendor-form/{token}/send-code",
            post(post_send_code),
        )
        .route(
            "/api/v1/vendor-form/{token}/confirm-code",
            post(post_confirm_code),
        )
        .route("/api/v1/vendor-form/{token}/submit", post(post_submit))
        // A file up to 5 MB (the usual limit is 2 MB).
        .route(
            "/api/v1/vendor-form/{token}/document",
            post(post_document).layer(DefaultBodyLimit::max(6 * 1024 * 1024)),
        )
        .route_service("/v/{token}", ServeFile::new(FORM_PAGE))
}
