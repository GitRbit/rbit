-- Users and organisations share one URL namespace.
CREATE TABLE accounts (
    id            INTEGER PRIMARY KEY,
    kind          TEXT    NOT NULL CHECK (kind IN ('user', 'organization')),
    name          TEXT    NOT NULL COLLATE NOCASE UNIQUE,
    display_name  TEXT,
    bio           TEXT,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
) STRICT;

-- Credentials for accounts of kind 'user'.
CREATE TABLE user_logins (
    account_id     INTEGER PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    email          TEXT    NOT NULL COLLATE NOCASE UNIQUE,
    password_hash  TEXT    NOT NULL,
    is_admin       INTEGER NOT NULL DEFAULT 0 CHECK (is_admin IN (0, 1))
) STRICT;

CREATE TABLE org_members (
    org_id   INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    user_id  INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    role     TEXT    NOT NULL CHECK (role IN ('owner', 'member')),
    PRIMARY KEY (org_id, user_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX org_members_by_user ON org_members (user_id);

CREATE TABLE repositories (
    id              INTEGER PRIMARY KEY,
    owner_id        INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    name            TEXT    NOT NULL COLLATE NOCASE,
    description     TEXT,
    visibility      TEXT    NOT NULL CHECK (visibility IN ('public', 'private')),
    default_branch  TEXT    NOT NULL,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    pushed_at       INTEGER,
    UNIQUE (owner_id, name)
) STRICT;

CREATE INDEX repositories_recent_public ON repositories (visibility, updated_at DESC);
