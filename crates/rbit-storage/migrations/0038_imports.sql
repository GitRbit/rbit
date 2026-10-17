-- Milestone 3, workstream M: repositories imported from GitHub or Gitea/Forgejo.
--
-- One row per imported repository: where it came from, what was asked for, and how far
-- the background job got (phase, counts, warnings, the error that stopped it). The
-- source's access token is never stored: it lives in memory while the job runs, so an
-- import interrupted by a restart is marked failed at start-up. Removing the row (or the
-- repository) forgets the import; the imported data stays ordinary repository data.
CREATE TABLE repo_imports (
    repo_id      INTEGER PRIMARY KEY REFERENCES repositories (id) ON DELETE CASCADE,
    importer_id  INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    source_kind  TEXT    NOT NULL CHECK (source_kind IN ('github', 'github_enterprise', 'gitea')),
    -- The source repository's web page.
    source_url   TEXT    NOT NULL CHECK (length(source_url) BETWEEN 1 AND 2048),
    -- `ImportOptions` as a JSON object.
    options      TEXT    NOT NULL DEFAULT '{}' CHECK (json_valid(options)),
    status       TEXT    NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'succeeded', 'failed')),
    phase        TEXT    NOT NULL DEFAULT 'repository'
                         CHECK (phase IN ('repository', 'lfs', 'labels', 'milestones', 'issues', 'releases', 'finished')),
    -- `ImportCounts` as a JSON object.
    counts       TEXT    NOT NULL DEFAULT '{}' CHECK (json_valid(counts)),
    -- At most 50 messages, as a JSON array of strings.
    warnings     TEXT    NOT NULL DEFAULT '[]' CHECK (json_valid(warnings)),
    error        TEXT,
    created_at   INTEGER NOT NULL,
    started_at   INTEGER,
    finished_at  INTEGER,
    CHECK ((status = 'failed') = (error IS NOT NULL))
) STRICT;

-- Start-up looks for imports a restart interrupted.
CREATE INDEX repo_imports_active ON repo_imports (status) WHERE status IN ('queued', 'running');
