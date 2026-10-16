-- Webhooks on a repository or an organisation (exactly one), and their delivery queue.
--
-- `secret` is the one documented exception to "secrets are stored only as digests":
-- signing a delivery with HMAC needs the raw value. It is write-only in the application
-- (never shown or returned again) and never copied into the delivery log.
CREATE TABLE webhooks (
    id          INTEGER PRIMARY KEY,
    repo_id     INTEGER REFERENCES repositories (id) ON DELETE CASCADE,
    org_id      INTEGER REFERENCES accounts (id) ON DELETE CASCADE,
    url         TEXT    NOT NULL,
    secret      TEXT,
    -- A JSON array of event names, e.g. ["push","release"].
    events      TEXT    NOT NULL DEFAULT '[]',
    active      INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    created_by  INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    CHECK ((repo_id IS NULL) <> (org_id IS NULL))
) STRICT;

CREATE INDEX webhooks_by_repo ON webhooks (repo_id) WHERE repo_id IS NOT NULL;
CREATE INDEX webhooks_by_org ON webhooks (org_id) WHERE org_id IS NOT NULL;

-- One event for one hook. The subscriber only inserts `pending` rows; the worker claims
-- due ones (`delivering`) and records the outcome of each attempt on the row.
CREATE TABLE webhook_deliveries (
    id                INTEGER PRIMARY KEY,
    hook_id           INTEGER NOT NULL REFERENCES webhooks (id) ON DELETE CASCADE,
    guid              TEXT    NOT NULL UNIQUE,
    event             TEXT    NOT NULL,
    action            TEXT,
    payload           TEXT    NOT NULL,
    status            TEXT    NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'delivering', 'succeeded', 'failed')),
    attempts          INTEGER NOT NULL DEFAULT 0,
    next_attempt_at   INTEGER,
    last_attempt_at   INTEGER,
    -- `Name: value` lines of the latest attempt's request and response.
    request_headers   TEXT,
    response_status   INTEGER,
    response_headers  TEXT,
    response_body     TEXT,
    duration_ms       INTEGER,
    error             TEXT,
    created_at        INTEGER NOT NULL
) STRICT;

CREATE INDEX webhook_deliveries_due ON webhook_deliveries (status, next_attempt_at);
CREATE INDEX webhook_deliveries_by_hook ON webhook_deliveries (hook_id, id DESC);
CREATE INDEX webhook_deliveries_by_age ON webhook_deliveries (created_at);
