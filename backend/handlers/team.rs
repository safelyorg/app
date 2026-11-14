//! The dashboard's "Team" card (services/features/team.rs):
//!
//! - GET  /api/v1/team          who is in the team, and the seats
//! - POST /api/v1/team/invite   {"email": "..."}   add a person
//! - POST /api/v1/team/remove   {"email": "..."}   remove a person
//! - POST /api/v1/team/leave                       leave the team you are in
//! - POST /api/v1/team/seats    {"extra_seats": 2} change the extra seats
//!
//! A refused change answers 400 {"error": "<code>"} (TeamError::code),
//! which the dashboard turns into a message in the page's language.

use crate::errors::billing::BillingError;
use crate::services::features::auth::extract_user_id;
use crate::services::features::team::{
    SeatChange, TeamError, invite, leave, remove, set_extra_seats, team_view,
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct TeamEmailBody {
    #[serde(default)]
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct TeamSeatsBody {
    pub extra_seats: i64,
}

async fn signed_in(pool: &Pool<Postgres>, headers: &HeaderMap) -> Result<Uuid, Response> {
    extract_user_id(headers, pool)
        .await
        .map_err(|_| {
            BillingError::InternalError("Failed to verify session".to_string()).into_response()
        })?
        .ok_or_else(|| BillingError::Unauthorized.into_response())
}

fn refused(e: TeamError) -> Response {
    let status = match e {
        TeamError::Unavailable => StatusCode::INTERNAL_SERVER_ERROR,
        TeamError::PaymentFailed => StatusCode::BAD_GATEWAY,
        TeamError::NotFound => StatusCode::NOT_FOUND,
        _ => StatusCode::BAD_REQUEST,
    };
    (status, Json(json!({ "error": e.code() }))).into_response()
}

fn done(result: Result<(), TeamError>) -> Response {
    match result {
        Ok(()) => Json(json!({ "success": true })).into_response(),
        Err(e) => refused(e),
    }
}

/// GET /api/v1/team
pub async fn get_team(State(pool): State<Pool<Postgres>>, headers: HeaderMap) -> Response {
    let user_id = match signed_in(&pool, &headers).await {
        Ok(id) => id,
        Err(r) => return r,
    };
    match team_view(&pool, user_id).await {
        Ok(view) => Json(view).into_response(),
        Err(e) => refused(e),
    }
}

/// POST /api/v1/team/invite
pub async fn invite_member(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(body): Json<TeamEmailBody>,
) -> Response {
    match signed_in(&pool, &headers).await {
        Ok(owner_id) => done(invite(&pool, owner_id, &body.email).await),
        Err(r) => r,
    }
}

/// POST /api/v1/team/remove
pub async fn remove_member(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(body): Json<TeamEmailBody>,
) -> Response {
    match signed_in(&pool, &headers).await {
        Ok(owner_id) => done(remove(&pool, owner_id, &body.email).await),
        Err(r) => r,
    }
}

/// POST /api/v1/team/leave
pub async fn leave_team(State(pool): State<Pool<Postgres>>, headers: HeaderMap) -> Response {
    match signed_in(&pool, &headers).await {
        Ok(user_id) => done(leave(&pool, user_id).await),
        Err(r) => r,
    }
}

/// POST /api/v1/team/seats
///
/// Answers {"result": "checkout", "checkout_url": "..."} when the seats
/// must be paid on Creem's page first, else {"result": "updated" |
/// "cancelled" | "unchanged"}.
pub async fn change_seats(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(body): Json<TeamSeatsBody>,
) -> Response {
    let owner_id = match signed_in(&pool, &headers).await {
        Ok(id) => id,
        Err(r) => return r,
    };
    let answer: Value = match set_extra_seats(&pool, owner_id, body.extra_seats).await {
        Ok(SeatChange::Checkout(url)) => json!({ "result": "checkout", "checkout_url": url }),
        Ok(SeatChange::Removed(removed)) => json!({ "result": "removed", "removed": removed }),
        Ok(SeatChange::Unchanged) => json!({ "result": "unchanged" }),
        Err(e) => return refused(e),
    };
    Json(answer).into_response()
}
