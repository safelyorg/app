CREATE TABLE IF NOT EXISTS fraud_reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    seller_id UUID REFERENCES sellers(id) ON DELETE SET NULL,
    platform VARCHAR(30),
    platform_id VARCHAR(200),
    report_type report_types NOT NULL,
    description TEXT,
    reported_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_fraud_seller_id ON fraud_reports(seller_id);
CREATE INDEX IF NOT EXISTS idx_fraud_platform_id ON fraud_reports(platform_id);
