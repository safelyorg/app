CREATE TABLE IF NOT EXISTS (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    listing_id UUID NOT NULL REFERENCES listings(id),
    risk_score SMALLINT NOT NULL,
    risk_level VARCHAR(10) NOT NULL,
    signals JSONB NOT NULL,
    price_analysis JSONB,
    network_summary TEXT,
    claude_raw TEXT,
    created_at TIMESTAMPZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_listing_id ON analyses(listing_id);
