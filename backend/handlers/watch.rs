//! "Watch this supplier" (services/features/watch.rs).
//!
//! - GET  /api/v1/watch        the team's watched suppliers
//! - POST /api/v1/watch        {"analysis_id"} start watching a scan's supplier
//! - POST /api/v1/watch/stop   {"seller_id"}   stop watching

use crate::services::features::auth::extract_user_id;
use crate::services::features::watch::{WatchError, add_watch, stop_watch, watch_list};
use axum::{Json, extract::State, http::HeaderMap, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct WatchRequest {
    pub analysis_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct StopWatchRequest {
    pub seller_id: Uuid,
}

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

fn failed(e: WatchError) -> Reply {
    let status = match e {
        WatchError::NeedsPaidPlan => StatusCode::FORBIDDEN,
        WatchError::LimitReached(_) => StatusCode::CONFLICT,
        WatchError::NotFound => StatusCode::NOT_FOUND,
        WatchError::Database(ref m) => {
            eprintln!("Safely: watch request failed: {m}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };
    let limit = match e {
        WatchError::LimitReached(n) => Some(n),
        _ => None,
    };
    (status, Json(json!({ "error": e.code(), "limit": limit })))
}

pub async fn get_watch_list(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
) -> Result<Json<Value>, Reply> {
    let user_id = signed_in(&pool, &headers).await?;
    watch_list(&pool, user_id).await.map(Json).map_err(failed)
}

pub async fn post_watch(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(req): Json<WatchRequest>,
) -> Result<Json<Value>, Reply> {
    let user_id = signed_in(&pool, &headers).await?;
    add_watch(&pool, user_id, req.analysis_id)
        .await
        .map_err(failed)?;
    Ok(Json(json!({ "watching": true })))
}

pub async fn post_stop_watch(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(req): Json<StopWatchRequest>,
) -> Result<Json<Value>, Reply> {
    let user_id = signed_in(&pool, &headers).await?;
    stop_watch(&pool, user_id, req.seller_id)
        .await
        .map_err(failed)?;
    Ok(Json(json!({ "watching": false })))
}
