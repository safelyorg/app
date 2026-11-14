-- Team plans: the people a paying account (the "owner") lets use its
-- plan, and the extra seats the owner bought.
--
-- How many people a plan has (services/features/team.rs):
--   Free        1 (only the owner, no invites)
--   Team        3 (the owner + 2), plus one per extra seat bought
--   Enterprise  unlimited
--
-- team_members: one row per invited person, by email. The person uses
-- the owner's plan once they sign in to Safely with that email - no
-- invite link is needed, because signing in proves the email is theirs.
-- An email can be in one team only (UNIQUE). If the owner has fewer
-- seats than members (for example after a downgrade), the members
-- invited first keep access. CASCADE: deleting the owner's account
-- deletes their team list.
CREATE TABLE IF NOT EXISTS team_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    email TEXT NOT NULL UNIQUE CHECK (email = lower(email)),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_team_members_owner ON team_members(owner_id, created_at);

-- team_extra_seats: the owner's extra seats. Each extra seat is its own
-- "Team extra member" subscription at Creem ($25), so each seat is its
-- own payment: removing a seat refunds that seat's payment in full. One
-- row per seat subscription (seats is Creem's "units", normally 1).
-- It is kept apart from the subscriptions table on purpose: it is not a
-- plan, so it never counts as one.
CREATE TABLE IF NOT EXISTS team_extra_seats (
    creem_subscription_id TEXT PRIMARY KEY,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    seats INTEGER NOT NULL DEFAULT 1 CHECK (seats >= 0),
    status TEXT NOT NULL,
    -- Ends with the owner's canceled plan, at the end of the paid period.
    -- NULL = not ending.
    cancel_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_team_extra_seats_owner ON team_extra_seats(owner_id, status);
