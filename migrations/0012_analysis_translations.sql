CREATE TABLE analysis_translations (
    id UUID PRIMARY KEY,
    analysis_id UUID NOT NULL REFERENCES analysis(id) ON DELETE CASCADE,
    language TEXT NOT NULL,
    signals JSONB NOT NULL,
    risk_factors JSONB NOT NULL,
    network_summary TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (analysis_id, language)
);

CREATE INDEX idx_analysis_translations_lookup ON analysis_translations (analysis_id, language);
