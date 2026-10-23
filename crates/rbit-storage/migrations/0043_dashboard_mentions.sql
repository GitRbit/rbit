-- Durable mention membership for personal dashboards, independent of inbox retention.
CREATE TABLE issue_mentions (
    issue_id INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    PRIMARY KEY (issue_id, user_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX issue_mentions_by_user ON issue_mentions (user_id, issue_id);
CREATE TABLE dashboard_mention_index (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    complete INTEGER NOT NULL DEFAULT 0 CHECK (complete IN (0, 1))
) STRICT;
INSERT INTO dashboard_mention_index (id) VALUES (1);
-- Service startup backfills existing Markdown in bounded batches once.
CREATE INDEX pull_review_requests_by_user ON pull_review_requests (user_id, pull_id);
