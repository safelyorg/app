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
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_subscriptions_user_id ON subscriptions(user_id);
CREATE INDEX IF NOT EXISTS idx_subscriptions_creem_subscription_id ON subscriptions(creem_subscription_id);
