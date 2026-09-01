CREATE TYPE outcome_action AS ENUM ('proceeded', 'aborted');

CREATE TABLE outcomes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    analysis_id UUID NOT NULL REFERENCES analysis(id),
    user_id UUID NOT NULL REFERENCES users(id),
    action outcome_action NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_outcomes_analysis_id ON outcomes(analysis_id);
