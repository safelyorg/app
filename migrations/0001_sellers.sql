CREATE TABLE IF NOT EXISTS sellers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    platform VARCHAR(30) NOT NULL,
    platform_id VARCHAR(200) NOT NULL,
    name VARCHAR(200),
    handle VARCHAR(200),
    phone VARCHAR(20),
    profile_url TEXT,
    join_date DATE,
    verification seller_verification NOT NULL DEFAULT 'unknown',
    location TEXT,
    last_seen_at TIMESTAMPTZ,
    last_active_text TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(platform, platform_id)
);

CREATE INDEX IF NOT EXISTS idx_platform_id ON sellers(platform_id);
CREATE INDEX IF NOT EXISTS idx_created_at ON sellers(created_at);
