-- Agent loop: request ids (agent plan step 4.3). A call that names a request id (`pr ensure`)
-- records what it answered, so a retry after a lost response gets the same answer and a
-- reuse of the id for other inputs is refused. Kept for a week.
CREATE TABLE agent_requests (
    account_id  INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    request_id  TEXT    NOT NULL CHECK (length(request_id) BETWEEN 1 AND 200),
    fingerprint TEXT    NOT NULL CHECK (length(fingerprint) BETWEEN 1 AND 128),
    answer      TEXT    NOT NULL CHECK (length(answer) <= 1024),
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (account_id, request_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX agent_requests_by_age ON agent_requests (created_at);
