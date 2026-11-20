-- Vendor checks (backend/services/features/vendors.rs).
--
-- A buyer invites a vendor (supplier) to fill in a short form at
-- safely.sh/v/<link>. Safely checks the answers in the background and
-- shows the result only to the buyer's team. The vendor never sees it.
--
-- team_owner_id: the account whose plan is used (plan_holder), so a team
-- shares one list. invited_by: the person who sent the invite.
--
-- status:
--   invited    link made, not opened yet
--   opened     the vendor opened the link
--   checking   the vendor sent the form, the checks are running
--   no_scans   sent, but the plan had no scans left (tried again later)
--   ready      the result is ready for the buyer
--   more_info  the buyer asked the vendor to fix some answers
--   approved / rejected   the buyer's decision
--   canceled   the buyer canceled the link
--   expired    the link was not used in time
--
-- known: what the buyer knew when inviting (name, email, country, phone,
-- website), to compare with the vendor's answers.
-- answers: the vendor's answers (company_name, country, company_number,
-- email, phone, website, bank_holder). More fields can be added later
-- without a new column.
-- ask_fields: when the buyer asks for more info, the only fields the
-- vendor can change. ask_note: the buyer's short note to the vendor.
-- previous: older answers, kept when the vendor sends changes.
-- result: the checks (cards) and the score.
-- asked_at: when the vendor was last asked to fill the form (invite or
-- "please fix"); one reminder is sent 3 days later (reminder_sent_at).
-- scan_counted: the plan's scan was counted for this vendor (once; a
-- vendor's changes are checked again without a new scan).
CREATE TABLE IF NOT EXISTS vendor_checks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    invited_by UUID REFERENCES users(id) ON DELETE SET NULL,
    buyer_company TEXT NOT NULL,
    vendor_name TEXT NOT NULL,
    vendor_email TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'invited',
    language TEXT NOT NULL DEFAULT 'en',
    send_email BOOLEAN NOT NULL DEFAULT TRUE,
    known JSONB NOT NULL DEFAULT '{}',
    answers JSONB NOT NULL DEFAULT '{}',
    ask_fields JSONB NOT NULL DEFAULT '[]',
    ask_note TEXT,
    previous JSONB NOT NULL DEFAULT '[]',
    email_code_hash TEXT,
    email_code_for TEXT,
    email_code_sent_at TIMESTAMPTZ,
    email_code_tries INTEGER NOT NULL DEFAULT 0,
    email_confirmed TEXT,
    result JSONB,
    risk_score INTEGER,
    risk_level TEXT,
    decision_note TEXT,
    decided_by UUID REFERENCES users(id) ON DELETE SET NULL,
    seller_id UUID REFERENCES sellers(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    opened_at TIMESTAMPTZ,
    submitted_at TIMESTAMPTZ,
    checked_at TIMESTAMPTZ,
    decided_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '14 days',
    asked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reminder_sent_at TIMESTAMPTZ,
    scan_counted BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_vendor_checks_owner ON vendor_checks(team_owner_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_vendor_checks_status ON vendor_checks(status);

-- vendor_check_links: the form links of one vendor check. token_hash is
-- the SHA-256 of the link's secret part: the link itself is never
-- saved, so a copy of the database cannot open any form. A reminder
-- adds a new link and the old one keeps working; "New link" and
-- "Cancel" close the old ones (revoked_at).
CREATE TABLE IF NOT EXISTS vendor_check_links (
    token_hash TEXT PRIMARY KEY,
    vendor_check_id UUID NOT NULL REFERENCES vendor_checks(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_vendor_check_links_check ON vendor_check_links(vendor_check_id);

-- vendor_documents: files the vendor uploads on the form (optional):
-- "registration" (company registration / business licence) and
-- "bank_letter" (a bank letter or statement header showing the account
-- holder). One file of each kind; a new upload replaces the old one.
-- reading: what Safely read from the file (company name, number,
-- account holder...), to compare with the answers. Only the buyer's
-- team can download a file; the vendor can't see it again.
CREATE TABLE IF NOT EXISTS vendor_documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    vendor_check_id UUID NOT NULL REFERENCES vendor_checks(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    file_name TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    data BYTEA NOT NULL,
    reading JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (vendor_check_id, kind)
);

-- vendor_email_sends: each invite, reminder or "please fix" email Safely
-- sent to a vendor's address, so one address gets at most 3 a day from
-- all Safely customers together (stops Safely being used to spam
-- someone). Email codes are not counted (the vendor asks for those).
CREATE TABLE IF NOT EXISTS vendor_email_sends (
    email TEXT NOT NULL,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_vendor_email_sends ON vendor_email_sends(email, sent_at);
