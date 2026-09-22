CREATE TYPE subscription_status AS ENUM (
    'active',
    'trialing',
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
    -- Records a pending downgrade that should only take effect once
    -- the current, already-paid-for period genuinely ends - not
    -- immediately. NULL means no downgrade is currently scheduled.
    scheduled_product_id TEXT,
    scheduled_plan_name TEXT,
    -- Real, monthly scan count for this billing period - reset to 0
    -- whenever upsert_subscription detects current_period_end has
    -- genuinely moved forward (a real renewal), never on any other
    -- update. Enforced against each plan's real limit in
    -- authorize_request via scan_limit_for_plan.
    scans_used_this_period INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_subscriptions_user_id ON subscriptions(user_id);
CREATE INDEX IF NOT EXISTS idx_subscriptions_creem_subscription_id ON subscriptions(creem_subscription_id);
