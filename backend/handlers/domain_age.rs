use crate::models::domain_age::DomainAgeQuery;
use crate::services::domain_age::{LookupError, allow_visitor, lookup, normalize_domain};
use axum::{
    Json,
    extract::Query,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;

/// GET /api/v1/tools/domain-age?domain=supplier.com
///
/// The Domain Age Checker's lookup. Answers with the dates and a verdict,
/// or {"error": "..."} that the page turns into a message:
///
/// - 400 invalid_domain  what was typed is not a domain
/// - 404 not_found       no registry knows the domain
/// - 429 too_many        this visitor ran 30 checks in the last hour
/// - 502 lookup_failed   the registries could not be reached
pub async fn domain_age(headers: HeaderMap, Query(query): Query<DomainAgeQuery>) -> Response {
    let Some(domain) = normalize_domain(&query.domain) else {
        return error(StatusCode::BAD_REQUEST, "invalid_domain");
    };
    if !allow_visitor(&visitor(&headers)) {
        return error(StatusCode::TOO_MANY_REQUESTS, "too_many");
    }
    match lookup(&domain).await {
        Ok(answer) => Json(answer).into_response(),
        Err(LookupError::NotFound) => error(StatusCode::NOT_FOUND, "not_found"),
        Err(LookupError::Failed) => error(StatusCode::BAD_GATEWAY, "lookup_failed"),
    }
}

/// Who is asking. Behind Cloudflare the real address is in
/// CF-Connecting-IP; without it, the first X-Forwarded-For address.
pub fn visitor(headers: &HeaderMap) -> String {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(',').next().unwrap_or_default().trim().to_string())
            .filter(|v| !v.is_empty())
    };
    header("cf-connecting-ip")
        .or_else(|| header("x-forwarded-for"))
        .unwrap_or_else(|| "unknown".to_string())
}

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn finds_the_real_visitor() {
        let mut h = HeaderMap::new();
        assert_eq!(visitor(&h), "unknown");
        h.insert(
            "x-forwarded-for",
            HeaderValue::from_static("9.9.9.9, 10.0.0.1"),
        );
        assert_eq!(visitor(&h), "9.9.9.9");
        h.insert("cf-connecting-ip", HeaderValue::from_static("1.2.3.4"));
        assert_eq!(visitor(&h), "1.2.3.4", "Cloudflare's header wins");
    }

    #[tokio::test]
    async fn rejects_text_that_is_not_a_domain() {
        let r = domain_age(
            HeaderMap::new(),
            Query(DomainAgeQuery {
                domain: "not a domain".to_string(),
            }),
        )
        .await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        let r = domain_age(
            HeaderMap::new(),
            Query(DomainAgeQuery {
                domain: String::new(),
            }),
        )
        .await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }
}
