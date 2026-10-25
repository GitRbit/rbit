-- rbit Actions: workflow runs on rbit-runner (docs/design/actions.md).
--
-- The tables always exist, but with `[actions] enabled = false` nothing writes to them.
-- Statuses are rbit_core::actions::Status codes: 1 success, 2 failure, 3 cancelled,
-- 4 skipped, 5 waiting, 6 running, 7 blocked, 8 awaiting approval. Every token is stored
-- only as its SHA-256 digest; secrets only sealed (AES-256-GCM, key outside the database).

-- The per-repository switch and settings. No row: Actions is off for the repository.
CREATE TABLE actions_repositories (
    repo_id           INTEGER PRIMARY KEY REFERENCES repositories (id) ON DELETE CASCADE,
    enabled           INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    -- The default permission of task tokens; workflows can only lower it.
    token_permission  TEXT    NOT NULL DEFAULT 'read' CHECK (token_permission IN ('read', 'write')),
    updated_at        INTEGER NOT NULL
) STRICT;

-- Owners (users or organisations) whose jobs instance runners take, unless
-- `[actions] instance_runners_for_all` is on.
CREATE TABLE actions_owner_policies (
    owner_id          INTEGER PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    instance_runners  INTEGER NOT NULL CHECK (instance_runners IN (0, 1)),
    updated_by        INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    updated_at        INTEGER NOT NULL
) STRICT;

-- Runners at instance, owner or repository scope.
CREATE TABLE actions_runners (
    id             INTEGER PRIMARY KEY,
    uuid           TEXT    NOT NULL UNIQUE CHECK (length(uuid) = 36),
    -- SHA-256 of the runner's secret token.
    token_digest   BLOB    NOT NULL UNIQUE CHECK (length(token_digest) = 32),
    name           TEXT    NOT NULL CHECK (length(name) BETWEEN 1 AND 255),
    scope_kind     TEXT    NOT NULL CHECK (scope_kind IN ('instance', 'owner', 'repository')),
    owner_id       INTEGER REFERENCES accounts (id) ON DELETE CASCADE,
    repo_id        INTEGER REFERENCES repositories (id) ON DELETE CASCADE,
    -- JSON array of the declared labels.
    labels         TEXT    NOT NULL DEFAULT '[]' CHECK (json_valid(labels) AND json_type(labels) = 'array'),
    -- JSON array of the labels its registration token allowed, or NULL for any.
    pinned_labels  TEXT    CHECK (pinned_labels IS NULL OR json_valid(pinned_labels) AND json_type(pinned_labels) = 'array'),
    version        TEXT    NOT NULL DEFAULT '' CHECK (length(version) <= 64),
    ephemeral      INTEGER NOT NULL DEFAULT 0 CHECK (ephemeral IN (0, 1)),
    disabled       INTEGER NOT NULL DEFAULT 0 CHECK (disabled IN (0, 1)),
    last_online    INTEGER,
    created_at     INTEGER NOT NULL,
    -- What it declared at its last start (JSON object: backends, os, arch, capacity,
    -- host_labels) and the runner protocol version it speaks.
    capabilities   TEXT    NOT NULL DEFAULT '{}' CHECK (json_valid(capabilities) AND json_type(capabilities) = 'object'),
    protocol       INTEGER NOT NULL DEFAULT 1 CHECK (protocol >= 1),
    CHECK (scope_kind = 'instance' AND owner_id IS NULL AND repo_id IS NULL
        OR scope_kind = 'owner' AND owner_id IS NOT NULL AND repo_id IS NULL
        OR scope_kind = 'repository' AND owner_id IS NULL AND repo_id IS NOT NULL)
) STRICT;

CREATE INDEX actions_runners_by_owner ON actions_runners (owner_id) WHERE owner_id IS NOT NULL;
CREATE INDEX actions_runners_by_repo ON actions_runners (repo_id) WHERE repo_id IS NOT NULL;

-- Registration tokens: shown once, expire, single-use unless created otherwise.
CREATE TABLE actions_registration_tokens (
    id              INTEGER PRIMARY KEY,
    digest          BLOB    NOT NULL UNIQUE CHECK (length(digest) = 32),
    scope_kind      TEXT    NOT NULL CHECK (scope_kind IN ('instance', 'owner', 'repository')),
    owner_id        INTEGER REFERENCES accounts (id) ON DELETE CASCADE,
    repo_id         INTEGER REFERENCES repositories (id) ON DELETE CASCADE,
    -- JSON array: a runner registered with the token may declare only these labels.
    labels          TEXT    CHECK (labels IS NULL OR json_valid(labels) AND json_type(labels) = 'array'),
    -- NULL: unlimited.
    uses_remaining  INTEGER CHECK (uses_remaining IS NULL OR uses_remaining >= 0),
    created_by      INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    created_at      INTEGER NOT NULL,
    expires_at      INTEGER NOT NULL CHECK (expires_at > created_at),
    revoked_at      INTEGER,
    CHECK (scope_kind = 'instance' AND owner_id IS NULL AND repo_id IS NULL
        OR scope_kind = 'owner' AND owner_id IS NOT NULL AND repo_id IS NULL
        OR scope_kind = 'repository' AND owner_id IS NULL AND repo_id IS NOT NULL)
) STRICT;

CREATE INDEX actions_registration_tokens_by_expiry ON actions_registration_tokens (expires_at);

-- Run numbers per repository: monotonic, never reused, even after runs are deleted.
CREATE TABLE actions_run_numbers (
    repo_id  INTEGER PRIMARY KEY REFERENCES repositories (id) ON DELETE CASCADE,
    last     INTEGER NOT NULL CHECK (last > 0)
) STRICT;

-- One workflow file triggered by one event. A run without jobs is a workflow error.
CREATE TABLE actions_runs (
    id                  INTEGER PRIMARY KEY,
    repo_id             INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    run_number          INTEGER NOT NULL CHECK (run_number > 0),
    workflow_path       TEXT    NOT NULL CHECK (length(workflow_path) BETWEEN 1 AND 1024),
    workflow_name       TEXT    NOT NULL CHECK (length(workflow_name) <= 1024),
    title               TEXT    NOT NULL CHECK (length(title) <= 1024),
    event               TEXT    NOT NULL CHECK (event IN ('push', 'pull_request', 'workflow_dispatch', 'schedule')),
    ref                 TEXT    NOT NULL CHECK (length(ref) BETWEEN 1 AND 1024),
    sha                 TEXT    NOT NULL CHECK (length(sha) IN (40, 64) AND sha NOT GLOB '*[^0-9a-f]*'),
    actor_id            INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    trigger_actor_id    INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    pull_number         INTEGER CHECK (pull_number IS NULL OR pull_number > 0),
    is_fork             INTEGER NOT NULL DEFAULT 0 CHECK (is_fork IN (0, 1)),
    -- Untrusted runs get a read-only token, no secrets and no variables (§11.1).
    trusted             INTEGER NOT NULL CHECK (trusted IN (0, 1)),
    status              INTEGER NOT NULL CHECK (status BETWEEN 1 AND 8),
    attempt             INTEGER NOT NULL DEFAULT 1 CHECK (attempt >= 1),
    concurrency_group   TEXT    CHECK (concurrency_group IS NULL OR length(concurrency_group) BETWEEN 1 AND 1024),
    cancel_in_progress  INTEGER NOT NULL DEFAULT 0 CHECK (cancel_in_progress IN (0, 1)),
    -- The GitHub-shaped event payload (JSON).
    event_payload       TEXT    NOT NULL CHECK (length(event_payload) <= 262144 AND json_valid(event_payload)),
    -- workflow_dispatch: the checked inputs (JSON object).
    dispatch_inputs     TEXT    CHECK (dispatch_inputs IS NULL OR json_valid(dispatch_inputs)),
    -- Why the workflow produced no jobs.
    error               TEXT    CHECK (error IS NULL OR length(error) <= 2048),
    approved_by         INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    approved_at         INTEGER,
    created_at          INTEGER NOT NULL,
    started_at          INTEGER,
    finished_at         INTEGER,
    -- When the run's logs were removed (retention or the instance log budget).
    logs_expired_at     INTEGER,
    -- The `workflow_run` action last announced to webhooks, so each is sent once.
    announced           TEXT    CHECK (announced IN ('requested', 'in_progress', 'completed')),
    UNIQUE (repo_id, run_number)
) STRICT;

CREATE INDEX actions_runs_by_repo ON actions_runs (repo_id, id DESC);
CREATE INDEX actions_runs_by_workflow ON actions_runs (repo_id, workflow_path, id DESC);
CREATE INDEX actions_runs_by_group ON actions_runs (repo_id, concurrency_group, status)
    WHERE concurrency_group IS NOT NULL AND status IN (5, 6, 7, 8);
CREATE INDEX actions_runs_by_pull ON actions_runs (repo_id, pull_number) WHERE pull_number IS NOT NULL;
-- Log pruning, oldest finished runs first.
CREATE INDEX actions_runs_with_logs ON actions_runs (finished_at) WHERE finished_at IS NOT NULL AND logs_expired_at IS NULL;

-- One job of a run: a workflow job, or one matrix combination of it.
CREATE TABLE actions_jobs (
    id           INTEGER PRIMARY KEY,
    run_id       INTEGER NOT NULL REFERENCES actions_runs (id) ON DELETE CASCADE,
    -- Denormalised from the run for the scheduler's queries.
    repo_id      INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    job_key      TEXT    NOT NULL CHECK (length(job_key) BETWEEN 1 AND 100),
    name         TEXT    NOT NULL CHECK (length(name) BETWEEN 1 AND 1024),
    -- The matrix combination (JSON object), NULL without a matrix.
    matrix       TEXT    CHECK (matrix IS NULL OR json_valid(matrix) AND json_type(matrix) = 'object'),
    -- JSON array of job keys this job waits for.
    needs        TEXT    NOT NULL DEFAULT '[]' CHECK (json_valid(needs) AND json_type(needs) = 'array'),
    -- The effective `permissions:` (JSON object of contents, issues, pull_requests), or NULL.
    permissions  TEXT    CHECK (permissions IS NULL OR json_valid(permissions) AND json_type(permissions) = 'object'),
    -- The single-job workflow handed to the runner.
    payload      BLOB    NOT NULL,
    status       INTEGER NOT NULL CHECK (status BETWEEN 1 AND 8),
    attempt      INTEGER NOT NULL DEFAULT 1 CHECK (attempt >= 1),
    task_id      INTEGER REFERENCES actions_tasks (id) ON DELETE SET NULL,
    runner_id    INTEGER REFERENCES actions_runners (id) ON DELETE SET NULL,
    created_at   INTEGER NOT NULL,
    -- When the job last became waiting: queue order and the abandoned sweep.
    queued_at    INTEGER,
    started_at   INTEGER,
    finished_at  INTEGER,
    -- The `workflow_job` action last announced to webhooks, so each is sent once.
    announced    TEXT    CHECK (announced IN ('queued', 'in_progress', 'completed', 'waiting')),
    -- The job has its own `if:`, which the server decides once its needs are done (§12.4):
    -- until then it stays blocked, so no runner can take it first.
    condition    INTEGER NOT NULL DEFAULT 0 CHECK (condition IN (0, 1)),
    -- `strategy.max-parallel`: at most this many jobs of the same key run at once.
    max_parallel INTEGER CHECK (max_parallel IS NULL OR max_parallel >= 1),
    -- Job-level `concurrency.group`: one job of a group runs at a time in the repository.
    job_group    TEXT    CHECK (job_group IS NULL OR length(job_group) BETWEEN 1 AND 1024)
) STRICT;

CREATE INDEX actions_jobs_by_run ON actions_jobs (run_id);
-- What FetchTask scans: waiting jobs, oldest first.
CREATE INDEX actions_jobs_waiting ON actions_jobs (queued_at, id) WHERE status = 5;
CREATE INDEX actions_jobs_waiting_by_repo ON actions_jobs (repo_id) WHERE status = 5;

-- The labels a job asks for (`runs-on`), so the claim query matches them in SQL.
CREATE TABLE actions_job_labels (
    job_id  INTEGER NOT NULL REFERENCES actions_jobs (id) ON DELETE CASCADE,
    label   TEXT    NOT NULL CHECK (length(label) BETWEEN 1 AND 255),
    PRIMARY KEY (job_id, label)
) STRICT, WITHOUT ROWID;

-- Job outputs, inserted once per key (later values for a key are ignored).
CREATE TABLE actions_job_outputs (
    job_id  INTEGER NOT NULL REFERENCES actions_jobs (id) ON DELETE CASCADE,
    key     TEXT    NOT NULL CHECK (length(key) BETWEEN 1 AND 255),
    value   TEXT    NOT NULL CHECK (length(value) <= 1048576),
    PRIMARY KEY (job_id, key)
) STRICT, WITHOUT ROWID;

-- One attempt of a job on a runner. AUTOINCREMENT: an id is never reused, so neither is a
-- task token's meaning.
CREATE TABLE actions_tasks (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id         INTEGER NOT NULL REFERENCES actions_jobs (id) ON DELETE CASCADE,
    attempt        INTEGER NOT NULL CHECK (attempt >= 1),
    runner_id      INTEGER REFERENCES actions_runners (id) ON DELETE SET NULL,
    -- SHA-256 of the task token; cleared as soon as the task stops or is cancelled.
    token_digest   BLOB    UNIQUE CHECK (token_digest IS NULL OR length(token_digest) = 32),
    -- The runner's Idempotency-Key of the poll that claimed it: a retried poll gets the
    -- same task (with a fresh token).
    idempotency_key TEXT    CHECK (idempotency_key IS NULL OR length(idempotency_key) BETWEEN 1 AND 64),
    status         INTEGER NOT NULL CHECK (status BETWEEN 1 AND 8),
    started_at     INTEGER NOT NULL,
    stopped_at     INTEGER,
    heartbeat_at   INTEGER NOT NULL,
    log_lines      INTEGER NOT NULL DEFAULT 0 CHECK (log_lines >= 0),
    log_bytes      INTEGER NOT NULL DEFAULT 0 CHECK (log_bytes >= 0),
    log_state      TEXT    NOT NULL DEFAULT 'live' CHECK (log_state IN ('live', 'final', 'expired')),
    log_truncated  INTEGER NOT NULL DEFAULT 0 CHECK (log_truncated IN (0, 1)),
    UNIQUE (runner_id, idempotency_key)
) STRICT;

CREATE INDEX actions_tasks_by_job ON actions_tasks (job_id);
-- The zombie sweep: running tasks by last heartbeat.
CREATE INDEX actions_tasks_running ON actions_tasks (heartbeat_at) WHERE status = 6;

-- The steps of a task, by index, with their slice of the task's log.
CREATE TABLE actions_steps (
    task_id     INTEGER NOT NULL REFERENCES actions_tasks (id) ON DELETE CASCADE,
    number      INTEGER NOT NULL CHECK (number >= 0),
    name        TEXT    NOT NULL CHECK (length(name) <= 1024),
    status      INTEGER NOT NULL CHECK (status BETWEEN 1 AND 8),
    started_at  INTEGER,
    stopped_at  INTEGER,
    log_index   INTEGER NOT NULL DEFAULT 0 CHECK (log_index >= 0),
    log_length  INTEGER NOT NULL DEFAULT 0 CHECK (log_length >= 0),
    PRIMARY KEY (task_id, number)
) STRICT, WITHOUT ROWID;

-- Encrypted secrets at owner or repository scope. The ciphertext's associated data binds
-- (scope kind, scope id, secret id), so a row moved elsewhere fails to open.
CREATE TABLE actions_secrets (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    scope_kind  TEXT    NOT NULL CHECK (scope_kind IN ('owner', 'repository')),
    owner_id    INTEGER REFERENCES accounts (id) ON DELETE CASCADE,
    repo_id     INTEGER REFERENCES repositories (id) ON DELETE CASCADE,
    name        TEXT    NOT NULL CHECK (length(name) BETWEEN 1 AND 100),
    -- Owner secrets: all of the owner's repositories, or those in actions_secret_repositories.
    visibility  TEXT    NOT NULL DEFAULT 'all' CHECK (visibility IN ('all', 'selected')),
    -- The first 8 bytes of SHA-256(key): a wrong or rotated key is detected, not misread.
    key_id      BLOB    NOT NULL CHECK (length(key_id) = 8),
    nonce       BLOB    NOT NULL CHECK (length(nonce) = 12),
    ciphertext  BLOB    NOT NULL CHECK (length(ciphertext) BETWEEN 16 AND 65552),
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    updated_by  INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    CHECK (scope_kind = 'owner' AND owner_id IS NOT NULL AND repo_id IS NULL
        OR scope_kind = 'repository' AND owner_id IS NULL AND repo_id IS NOT NULL AND visibility = 'all')
) STRICT;

CREATE UNIQUE INDEX actions_secrets_owner_name ON actions_secrets (owner_id, name) WHERE scope_kind = 'owner';
CREATE UNIQUE INDEX actions_secrets_repo_name ON actions_secrets (repo_id, name) WHERE scope_kind = 'repository';

-- The repositories an owner secret with visibility 'selected' is given to.
CREATE TABLE actions_secret_repositories (
    secret_id  INTEGER NOT NULL REFERENCES actions_secrets (id) ON DELETE CASCADE,
    repo_id    INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    PRIMARY KEY (secret_id, repo_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX actions_secret_repositories_by_repo ON actions_secret_repositories (repo_id);

-- Plain configuration variables at owner or repository scope.
CREATE TABLE actions_variables (
    id          INTEGER PRIMARY KEY,
    scope_kind  TEXT    NOT NULL CHECK (scope_kind IN ('owner', 'repository')),
    owner_id    INTEGER REFERENCES accounts (id) ON DELETE CASCADE,
    repo_id     INTEGER REFERENCES repositories (id) ON DELETE CASCADE,
    name        TEXT    NOT NULL CHECK (length(name) BETWEEN 1 AND 100),
    value       TEXT    NOT NULL CHECK (length(value) <= 49152),
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    updated_by  INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    CHECK (scope_kind = 'owner' AND owner_id IS NOT NULL AND repo_id IS NULL
        OR scope_kind = 'repository' AND owner_id IS NULL AND repo_id IS NOT NULL)
) STRICT;

CREATE UNIQUE INDEX actions_variables_owner_name ON actions_variables (owner_id, name) WHERE scope_kind = 'owner';
CREATE UNIQUE INDEX actions_variables_repo_name ON actions_variables (repo_id, name) WHERE scope_kind = 'repository';

-- Pull request authors whose untrusted runs need no approval in a repository.
CREATE TABLE actions_trusted_users (
    repo_id      INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    user_id      INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    approved_by  INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    created_at   INTEGER NOT NULL,
    PRIMARY KEY (repo_id, user_id)
) STRICT, WITHOUT ROWID;

-- `schedule` entries of default-branch workflows, with their next fire time.
CREATE TABLE actions_schedules (
    repo_id        INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    workflow_path  TEXT    NOT NULL CHECK (length(workflow_path) BETWEEN 1 AND 1024),
    cron           TEXT    NOT NULL CHECK (length(cron) BETWEEN 1 AND 255),
    next_at        INTEGER NOT NULL,
    -- Consecutive scheduled runs that failed to plan; 3 pause the schedule.
    failures       INTEGER NOT NULL DEFAULT 0 CHECK (failures >= 0),
    paused         INTEGER NOT NULL DEFAULT 0 CHECK (paused IN (0, 1)),
    PRIMARY KEY (repo_id, workflow_path, cron)
) STRICT, WITHOUT ROWID;

CREATE INDEX actions_schedules_due ON actions_schedules (next_at) WHERE paused = 0;
