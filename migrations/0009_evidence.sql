CREATE TYPE evidence_type AS ENUM ('signal', 'check', 'outcome');

CREATE TABLE evidence (
    id UUID PRIMARY KEY,
    analysis_id UUID NOT NULL REFERENCES analysis(id),
    seller_id UUID NOT NULL REFERENCES sellers(id),
    evidence_type evidence_type NOT NULL,
    label TEXT NOT NULL,
    value TEXT NOT NULL,
    source TEXT NOT NULL,
    found_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_evidence_analysis_id ON evidence(analysis_id);
CREATE INDEX idx_evidence_seller_id ON evidence(seller_id);
