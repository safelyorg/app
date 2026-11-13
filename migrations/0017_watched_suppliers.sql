-- Watch suppliers (backend/services/features/watch.rs).
--
-- seller_facts: what a scan knew about the seller (name, country,
-- website, company numbers), so a watch can run the same outside checks
-- again later. One row per seller, the latest scan's facts.
CREATE TABLE IF NOT EXISTS seller_facts (
    seller_id UUID PRIMARY KEY REFERENCES sellers(id) ON DELETE CASCADE,
    facts JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- watched_suppliers: the suppliers a team (the plan owner's account)
-- watches. state = how the supplier looked at the last check; changes =
-- what got worse at the last check that found something.
-- status: pending (not checked yet), no_change, changed,
-- listing_not_checked, paused (plan not paid).
CREATE TABLE IF NOT EXISTS watched_suppliers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    seller_id UUID NOT NULL REFERENCES sellers(id) ON DELETE CASCADE,
    analysis_id UUID REFERENCES analysis(id) ON DELETE SET NULL,
    listing_url TEXT,
    added_by UUID REFERENCES users(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    state JSONB,
    changes JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_checked_at TIMESTAMPTZ,
    changed_at TIMESTAMPTZ,
    next_check_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (team_owner_id, seller_id)
);

CREATE INDEX IF NOT EXISTS idx_watched_suppliers_due ON watched_suppliers(next_check_at);
