-- Two-factor authentication (TOTP, RFC 6238). One row per user who has started enrolling;
-- `confirmed_at` is set once a code from the authenticator app has been accepted, and only
-- confirmed rows protect the account.
CREATE TABLE two_factor (
    account_id      INTEGER PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    secret          BLOB    NOT NULL,
    created_at      INTEGER NOT NULL,
    confirmed_at    INTEGER,
    -- Replay protection: the highest TOTP time step accepted so far. A code is only
    -- accepted for a later step, so each code works once.
    last_used_step  INTEGER
) STRICT;

-- Single-use recovery codes, stored as SHA-256 digests (salted with the account id).
CREATE TABLE recovery_codes (
    account_id   INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    code_digest  BLOB    NOT NULL,
    used_at      INTEGER,
    PRIMARY KEY (account_id, code_digest)
) STRICT, WITHOUT ROWID;

-- Append-only security audit log. Deliberately no foreign keys: entries outlive the
-- accounts they mention, so names are stored as they were at the time.
CREATE TABLE audit_events (
    id           INTEGER PRIMARY KEY,
    created_at   INTEGER NOT NULL,
    category     TEXT    NOT NULL,
    action       TEXT    NOT NULL,
    actor_id     INTEGER,
    actor_name   TEXT,
    target_name  TEXT,
    ip_address   TEXT,
    detail       TEXT    NOT NULL DEFAULT ''
) STRICT;

CREATE INDEX audit_events_by_category ON audit_events (category, id DESC);
CREATE INDEX audit_events_by_actor ON audit_events (actor_id, id DESC);
