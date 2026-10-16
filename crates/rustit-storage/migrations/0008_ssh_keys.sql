-- SSH public keys of users, used to authenticate Git over SSH.
-- `public_key` is the OpenSSH form without its comment (`ssh-ed25519 AAAA…`);
-- `fingerprint` is its SHA256 fingerprint (`SHA256:…`), the lookup key at sign-in.
CREATE TABLE ssh_keys (
    id            INTEGER PRIMARY KEY,
    account_id    INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    title         TEXT    NOT NULL,
    public_key    TEXT    NOT NULL,
    algorithm     TEXT    NOT NULL,
    fingerprint   TEXT    NOT NULL UNIQUE,
    created_at    INTEGER NOT NULL,
    last_used_at  INTEGER
) STRICT;

CREATE INDEX ssh_keys_by_account ON ssh_keys (account_id, created_at DESC);

-- Keys that grant access to exactly one repository, for servers and CI.
CREATE TABLE deploy_keys (
    id            INTEGER PRIMARY KEY,
    repo_id       INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    title         TEXT    NOT NULL,
    public_key    TEXT    NOT NULL,
    algorithm     TEXT    NOT NULL,
    fingerprint   TEXT    NOT NULL UNIQUE,
    read_only     INTEGER NOT NULL DEFAULT 1 CHECK (read_only IN (0, 1)),
    created_at    INTEGER NOT NULL,
    last_used_at  INTEGER
) STRICT;

CREATE INDEX deploy_keys_by_repo ON deploy_keys (repo_id, created_at DESC);

-- A fingerprint identifies one key holder, so it may appear in only one of the two tables.
CREATE TRIGGER ssh_keys_fingerprint_not_a_deploy_key
BEFORE INSERT ON ssh_keys
WHEN EXISTS (SELECT 1 FROM deploy_keys WHERE fingerprint = NEW.fingerprint)
BEGIN
    SELECT RAISE(ABORT, 'UNIQUE constraint failed: ssh key fingerprint');
END;

CREATE TRIGGER deploy_keys_fingerprint_not_a_user_key
BEFORE INSERT ON deploy_keys
WHEN EXISTS (SELECT 1 FROM ssh_keys WHERE fingerprint = NEW.fingerprint)
BEGIN
    SELECT RAISE(ABORT, 'UNIQUE constraint failed: ssh key fingerprint');
END;
