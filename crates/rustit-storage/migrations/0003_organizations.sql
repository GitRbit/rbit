-- Organisation profile fields. Kept apart from `accounts` so user and organisation
-- columns never mix; one row per account of kind 'organization'.
CREATE TABLE org_profiles (
    account_id   INTEGER PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    description  TEXT,
    website      TEXT
) STRICT;

-- "Owners of this organisation" (last-owner checks, member list ordering).
CREATE INDEX org_members_by_role ON org_members (org_id, role);

-- Explore: public repositories, newest first.
CREATE INDEX repositories_newest_public ON repositories (visibility, created_at DESC);
