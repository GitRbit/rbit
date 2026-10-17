-- Branch protection (milestone 2, wave B, workstream B): rules per branch pattern of a
-- repository, with the users and teams a rule lets push when it restricts pushers.
-- Patterns are case-sensitive (`main` and `Main` are different branches).
CREATE TABLE branch_protections (
    id                          INTEGER PRIMARY KEY,
    repo_id                     INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    pattern                     TEXT    NOT NULL CHECK (length(pattern) BETWEEN 1 AND 255),
    deny_force_push             INTEGER NOT NULL DEFAULT 1 CHECK (deny_force_push IN (0, 1)),
    deny_deletion               INTEGER NOT NULL DEFAULT 1 CHECK (deny_deletion IN (0, 1)),
    require_pull_request        INTEGER NOT NULL DEFAULT 0 CHECK (require_pull_request IN (0, 1)),
    required_approvals          INTEGER NOT NULL DEFAULT 0 CHECK (required_approvals BETWEEN 0 AND 10),
    dismiss_stale_approvals     INTEGER NOT NULL DEFAULT 0 CHECK (dismiss_stale_approvals IN (0, 1)),
    block_on_changes_requested  INTEGER NOT NULL DEFAULT 1 CHECK (block_on_changes_requested IN (0, 1)),
    restrict_pushers            INTEGER NOT NULL DEFAULT 0 CHECK (restrict_pushers IN (0, 1)),
    allow_admin_bypass          INTEGER NOT NULL DEFAULT 0 CHECK (allow_admin_bypass IN (0, 1)),
    created_by                  INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    created_at                  INTEGER NOT NULL,
    updated_at                  INTEGER NOT NULL,
    UNIQUE (repo_id, pattern),
    CHECK (required_approvals = 0 OR require_pull_request = 1)
) STRICT;

-- Users a restricting rule lets push.
CREATE TABLE branch_protection_users (
    rule_id  INTEGER NOT NULL REFERENCES branch_protections (id) ON DELETE CASCADE,
    user_id  INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    PRIMARY KEY (rule_id, user_id)
) STRICT;

-- Teams (of the repository's organisation) whose members a restricting rule lets push.
CREATE TABLE branch_protection_teams (
    rule_id  INTEGER NOT NULL REFERENCES branch_protections (id) ON DELETE CASCADE,
    team_id  INTEGER NOT NULL REFERENCES teams (id) ON DELETE CASCADE,
    PRIMARY KEY (rule_id, team_id)
) STRICT;

-- A repository transferred to another owner keeps its rules, but teams of the former
-- organisation no longer let anyone push.
CREATE TRIGGER branch_protection_teams_follow_owner
AFTER UPDATE OF owner_id ON repositories
WHEN NEW.owner_id <> OLD.owner_id
BEGIN
    DELETE FROM branch_protection_teams
    WHERE rule_id IN (SELECT id FROM branch_protections WHERE repo_id = NEW.id)
      AND team_id NOT IN (SELECT id FROM teams WHERE org_id = NEW.owner_id);
END;
