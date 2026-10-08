CREATE TYPE subscription_status AS ENUM (
    'active',
    'past_due',
    'canceled',
    'paused',
    'expired',
    'unpaid'
);

CREATE TABLE IF NOT EXISTS subscriptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- CASCADE (not SET NULL like analysis/fraud_reports) - a
    -- subscription record has zero shared/community value once someone
    -- deletes their account, unlike fraud reports which still protect
    -- other users. Deleting the account should genuinely delete this
    -- too, not anonymize it.
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    creem_subscription_id TEXT NOT NULL UNIQUE,
    creem_customer_id TEXT NOT NULL,
    creem_product_id TEXT NOT NULL,
    plan_name TEXT NOT NULL,
    status subscription_status NOT NULL,
    current_period_end TIMESTAMPTZ,
    canceled_at TIMESTAMPTZ,
    -- A pending downgrade (e.g. Enterprise -> Team). Creem already
    -- bills the new product from the next renewal; here the user keeps
    -- the current plan until that renewal arrives, then these are
    -- applied and cleared. NULL means no downgrade is scheduled.
    scheduled_product_id TEXT,
    scheduled_plan_name TEXT,
    -- Scans used this month - reset to 0 whenever upsert_subscription
    -- sees current_period_end move forward (a renewal), and on yearly
    -- plans also every month (see scan_anchor). Enforced against
    -- each plan's limit in authorize_request via scan_limit_for_plan.
    scans_used_this_period INTEGER NOT NULL DEFAULT 0,
    -- 'month' or 'year' - how often Creem bills this subscription.
    -- Scan limits are monthly on both: a yearly plan still gets its
    -- scans back every month (see scan_anchor).
    billing_interval TEXT NOT NULL DEFAULT 'month'
        CHECK (billing_interval IN ('month', 'year')),
    -- Start of the scan month the count belongs to (yearly plans).
    scan_period_start TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- When the plan was bought or last renewed. A yearly plan's scan
    -- months run from this date (bought on the 14th -> scans come back
    -- every 14th). Set to NOW() whenever a new billing period begins.
    scan_anchor TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_subscriptions_user_id ON subscriptions(user_id);
CREATE INDEX IF NOT EXISTS idx_subscriptions_creem_subscription_id ON subscriptions(creem_subscription_id);

-- Free plan: a fixed number of scans a month for anyone without an
-- active paid subscription (the number is FREE_MONTHLY_SCANS in
-- services/billing.rs). The Free month runs from the day the user
-- signed up (users.created_at): signed up on the 14th -> scans come
-- back every 14th. One row per user, created on their first free scan.
-- period_start is when the Free month the count belongs to started;
-- once a new Free month begins, the next scan starts the count again
-- at 1 (see use_free_scan in services/billing.rs). Unused scans do not
-- carry over. CASCADE: deleting the account deletes this too.
CREATE TABLE IF NOT EXISTS free_scan_usage (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    period_start TIMESTAMPTZ NOT NULL,
    scans_used INTEGER NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
