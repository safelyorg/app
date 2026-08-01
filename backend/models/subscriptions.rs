use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::prelude::FromRow;
use uuid::Uuid;

/// Matches subscription_status exactly - Creem's own vocabulary for
/// these states, kept as-is rather than renamed, so mapping a real
/// webhook event straight into this type needs no translation step.
#[derive(Debug, sqlx::Type, Serialize, Clone, Copy, PartialEq)]
#[sqlx(type_name = "subscription_status", rename_all = "snake_case")]
pub enum SubscriptionStatus {
    Active,
    Trialing,
    PastDue,
    Canceled,
    Paused,
    Expired,
    Unpaid,
}

#[derive(Debug, FromRow, Serialize, Clone)]
pub struct Subscription {
    pub id: Uuid,
    pub user_id: Uuid,
    pub creem_subscription_id: String,
    pub creem_customer_id: String,
    pub creem_product_id: String,
    pub plan_name: String,
    pub status: SubscriptionStatus,
    pub current_period_end: Option<DateTime<Utc>>,
    pub canceled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
