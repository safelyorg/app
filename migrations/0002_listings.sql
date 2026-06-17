CREATE TABLE IF NOT EXISTS listings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES sellers(id),
    platform VARCHAR(20) NOT NULL,
    listing_url TEXT NOT NULL UNIQUE,
    listing_id VARCHAR(200),
    title TEXT,
    price BIGINT,
    description TEXT,
    category listing_categories,
    image_urls TEXT[],
    posted_date DATE,
    first_seen_at TIMESTAMPZ NOT NULL DEFAULT NOW(),
    last_analyzed_at TIMESTAMPZ
);

CREATE INDEX idx_user_id ON listings(user_id);
CREATE INDEX idx_listing_id ON listings(listing_id);
