use crate::models::lookalike::LookalikeQuery;
use crate::services::features::lookalike::{check_against, check_brands, parse_domain};
use axum::{
    Json,
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// GET /api/v1/tools/lookalike?domain=paypa1-secure.com&real=paypal.com
///
/// The Lookalike Domain Checker. `real` is optional: without it, the
/// domain is compared with famous brands. Answers with the verdict and
/// the tricks found, or {"error": "..."}:
///
/// - 400 invalid_domain  what was typed in the first box is not a domain
/// - 400 invalid_real    what was typed in the second box is not a domain
///
/// Nothing is looked up on the internet, so there is no limit per visitor.
pub async fn lookalike(Query(query): Query<LookalikeQuery>) -> Response {
    let Some(domain) = parse_domain(&query.domain) else {
        return error("invalid_domain");
    };
    let answer = if query.real.trim().is_empty() {
        check_brands(&domain)
    } else {
        let Some(real) = parse_domain(&query.real) else {
            return error("invalid_real");
        };
        check_against(&domain, &real)
    };
    Json(answer).into_response()
}

fn error(code: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": code }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn ask(domain: &str, real: &str) -> (StatusCode, String) {
        let r = lookalike(Query(LookalikeQuery {
            domain: domain.to_string(),
            real: real.to_string(),
        }))
        .await;
        let status = r.status();
        let body = to_bytes(r.into_body(), 100_000).await.unwrap();
        (status, String::from_utf8_lossy(&body).to_string())
    }

    #[tokio::test]
    async fn answers_and_errors() {
        let (s, body) = ask("paypa1.com", "paypal.com").await;
        assert_eq!(s, StatusCode::OK);
        assert!(body.contains("\"lookalike\""), "{body}");
        let (s, body) = ask("amaz0n.com", "  ").await;
        assert_eq!(s, StatusCode::OK);
        assert!(body.contains("\"brand\":\"Amazon\""), "{body}");
        let (s, body) = ask("not a domain", "").await;
        assert_eq!(
            (s, body.contains("invalid_domain")),
            (StatusCode::BAD_REQUEST, true)
        );
        let (s, body) = ask("paypa1.com", "paypal").await;
        assert_eq!(
            (s, body.contains("invalid_real")),
            (StatusCode::BAD_REQUEST, true)
        );
    }
}
