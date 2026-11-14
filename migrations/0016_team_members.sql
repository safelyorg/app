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

-- team_extra_seats: the owner's "Team extra member" subscription at
-- Creem ($25 per seat). seats is Creem's "units" on that subscription.
-- It is kept apart from the subscriptions table on purpose: it is not a
-- plan, so it never counts as one. One row per owner.
CREATE TABLE IF NOT EXISTS team_extra_seats (
    owner_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    creem_subscription_id TEXT NOT NULL UNIQUE,
    seats INTEGER NOT NULL DEFAULT 0 CHECK (seats >= 0),
    status TEXT NOT NULL,
    -- Ends with the owner's canceled plan, at the end of the paid period.
    -- NULL = not ending.
    cancel_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
