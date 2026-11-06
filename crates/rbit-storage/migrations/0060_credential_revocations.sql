-- Inactive foundation: existing password/session paths do not use this fence yet.
ALTER TABLE user_logins ADD COLUMN authentication_epoch INTEGER NOT NULL DEFAULT 0
    CHECK (authentication_epoch >= 0);

-- Never store PHCs, cookie/token material, or generic event payloads here.
-- AUTOINCREMENT prevents reuse of committed IDs after completed-row cleanup.
CREATE TABLE credential_revocations (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id    INTEGER NOT NULL REFERENCES user_logins(account_id) ON DELETE CASCADE,
    target_epoch  INTEGER NOT NULL CHECK (target_epoch > 0),
    created_at    INTEGER NOT NULL,
    outcome       TEXT CHECK (outcome IN ('kept_current', 'ended_all')),
    completed_at  INTEGER,
    CHECK ((outcome IS NULL) = (completed_at IS NULL)),
    UNIQUE (account_id, target_epoch)
) STRICT;
CREATE INDEX credential_revocations_pending ON credential_revocations(id)
    WHERE completed_at IS NULL;
