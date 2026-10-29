-- rbit Actions (docs/design/actions.md §12.2.2): deployment environments. A job that names one
-- with `environment:` gets its secrets and variables over the repository's, waits for one of
-- its reviewers when it has any, and runs only from the refs its branch policy allows.

CREATE TABLE actions_environments (
    id                  INTEGER PRIMARY KEY,
    repo_id             INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    -- Names compare without case, as on GitHub.
    name                TEXT    NOT NULL COLLATE NOCASE CHECK (length(name) BETWEEN 1 AND 255),
    prevent_self_review INTEGER NOT NULL DEFAULT 0 CHECK (prevent_self_review IN (0, 1)),
    -- 'all' refs, 'protected' branches, or the 'selected' GitHub filter patterns.
    branch_policy       TEXT    NOT NULL DEFAULT 'all' CHECK (branch_policy IN ('all', 'protected', 'selected')),
    branch_patterns     TEXT    NOT NULL DEFAULT '[]' CHECK (json_valid(branch_patterns) AND json_type(branch_patterns) = 'array'),
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL,
    UNIQUE (repo_id, name)
) STRICT;

-- Who may approve an environment's jobs; none means its jobs run without review.
CREATE TABLE actions_environment_reviewers (
    environment_id INTEGER NOT NULL REFERENCES actions_environments (id) ON DELETE CASCADE,
    account_id     INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    PRIMARY KEY (environment_id, account_id)
) STRICT, WITHOUT ROWID;

-- Environment secrets, sealed as the others are (the scope in the associated data is the
-- environment). Tables of their own: `actions_secrets`' scope CHECK cannot be widened
-- without rebuilding it, which would cascade through its owner secrets' repository grants.
CREATE TABLE actions_environment_secrets (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    environment_id INTEGER NOT NULL REFERENCES actions_environments (id) ON DELETE CASCADE,
    name           TEXT    NOT NULL CHECK (length(name) BETWEEN 1 AND 100),
    key_id         BLOB    NOT NULL CHECK (length(key_id) = 8),
    nonce          BLOB    NOT NULL CHECK (length(nonce) = 12),
    ciphertext     BLOB    NOT NULL CHECK (length(ciphertext) BETWEEN 16 AND 65552),
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    updated_by     INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    UNIQUE (environment_id, name)
) STRICT;

CREATE TABLE actions_environment_variables (
    id             INTEGER PRIMARY KEY,
    environment_id INTEGER NOT NULL REFERENCES actions_environments (id) ON DELETE CASCADE,
    name           TEXT    NOT NULL CHECK (length(name) BETWEEN 1 AND 100),
    value          TEXT    NOT NULL CHECK (length(value) <= 49152),
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    updated_by     INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    UNIQUE (environment_id, name)
) STRICT;

-- A job's environment (its name, as planned), the URL the runner reported for it, whether it
-- waits for a reviewer (its status is then 8), and the attempt a reviewer approved: a rerun
-- is a new attempt and waits again.
ALTER TABLE actions_jobs ADD COLUMN environment TEXT CHECK (environment IS NULL OR length(environment) BETWEEN 1 AND 255);
ALTER TABLE actions_jobs ADD COLUMN environment_url TEXT CHECK (environment_url IS NULL OR length(environment_url) <= 2048);
ALTER TABLE actions_jobs ADD COLUMN review INTEGER NOT NULL DEFAULT 0 CHECK (review IN (0, 1));
ALTER TABLE actions_jobs ADD COLUMN approved_attempt INTEGER;

-- Reviewers' decisions, one per job attempt they decided.
CREATE TABLE actions_deployment_reviews (
    id          INTEGER PRIMARY KEY,
    job_id      INTEGER NOT NULL REFERENCES actions_jobs (id) ON DELETE CASCADE,
    attempt     INTEGER NOT NULL CHECK (attempt >= 1),
    environment TEXT    NOT NULL CHECK (length(environment) BETWEEN 1 AND 255),
    reviewer_id INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    decision    TEXT    NOT NULL CHECK (decision IN ('approved', 'rejected')),
    comment     TEXT    NOT NULL DEFAULT '' CHECK (length(comment) <= 65536),
    created_at  INTEGER NOT NULL
) STRICT;

CREATE INDEX actions_deployment_reviews_by_job ON actions_deployment_reviews (job_id);
