-- rbit Actions: the policies GitHub's `actions/permissions` endpoints read and write, for a
-- repository and for its owner (a user or an organisation). An owner's policies are ceilings
-- its repositories can only narrow; a repository's own row says what it asked for.

-- Which actions a repository's workflows may use besides rbit's built-ins and the
-- workspace's own: `all`, `local_only` (none), or `selected` (GitHub's own `actions/*` and
-- `github/*` when `github_owned_allowed`, and those matching `patterns_allowed`, a JSON
-- array of strings).
ALTER TABLE actions_repositories ADD COLUMN allowed_actions TEXT NOT NULL DEFAULT 'all'
    CHECK (allowed_actions IN ('all', 'local_only', 'selected'));
ALTER TABLE actions_repositories ADD COLUMN github_owned_allowed INTEGER NOT NULL DEFAULT 1
    CHECK (github_owned_allowed IN (0, 1));
ALTER TABLE actions_repositories ADD COLUMN patterns_allowed TEXT NOT NULL DEFAULT '[]';
-- Whether the workflow token may approve pull requests.
ALTER TABLE actions_repositories ADD COLUMN can_approve_reviews INTEGER NOT NULL DEFAULT 0
    CHECK (can_approve_reviews IN (0, 1));
-- Which pull request authors without write access wait for a writer's approval.
ALTER TABLE actions_repositories ADD COLUMN approval_policy TEXT NOT NULL DEFAULT 'first_time_contributors'
    CHECK (approval_policy IN ('first_time_contributors_new_to_github', 'first_time_contributors', 'all_external_contributors'));

-- An owner's ceilings. Without a row nothing is capped: every repository may run
-- workflows and choose any setting.
CREATE TABLE actions_owner_settings (
    owner_id              INTEGER PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    -- Which of the owner's repositories may run workflows: `all`, `none`, or `selected`
    -- (those in actions_owner_enabled_repositories).
    enabled_repositories  TEXT    NOT NULL DEFAULT 'all' CHECK (enabled_repositories IN ('all', 'none', 'selected')),
    allowed_actions       TEXT    NOT NULL DEFAULT 'all' CHECK (allowed_actions IN ('all', 'local_only', 'selected')),
    github_owned_allowed  INTEGER NOT NULL DEFAULT 1 CHECK (github_owned_allowed IN (0, 1)),
    patterns_allowed      TEXT    NOT NULL DEFAULT '[]',
    -- The most a repository's default token permission may be.
    token_permission      TEXT    NOT NULL DEFAULT 'write' CHECK (token_permission IN ('read', 'write')),
    can_approve_reviews   INTEGER NOT NULL DEFAULT 1 CHECK (can_approve_reviews IN (0, 1)),
    -- The least strict approval policy a repository may have.
    approval_policy       TEXT    NOT NULL DEFAULT 'first_time_contributors_new_to_github'
        CHECK (approval_policy IN ('first_time_contributors_new_to_github', 'first_time_contributors', 'all_external_contributors')),
    updated_at            INTEGER NOT NULL
) STRICT;

-- The repositories an owner with `enabled_repositories = 'selected'` lets run workflows.
CREATE TABLE actions_owner_enabled_repositories (
    owner_id  INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    repo_id   INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    PRIMARY KEY (owner_id, repo_id)
) STRICT;
