CREATE TABLE IF NOT EXISTS sellers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    platform VARCHAR(30) NOT NULL,
    platform_id VARCHAR(200) NOT NULL,
    name VARCHAR(200),
    handle VARCHAR(200),
    phone VARCHAR(20),
    profile_url TEXT NOT NULL,
    join_date DATE,
    verification VARCHAR(20) NOT NULL DEFAULT 'unknown',
    total_deals INTEGER NOT NULL DEFAULT 0,
    disputes INTEGER NOT NULL DEFAULT 0,
    completion_rate INTEGER,
    location VARCHAR(150),
    last_seen_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(platform, platform_id)
);

CREATE INDEX idx_platform_id ON sellers(platform_id);
CREATE INDEX idx_created_at ON sellers(created_at);
