-- Milestone 2 access: collaborators, organisation teams and the organisation base
-- permission. Effective access is computed by `rustit_auth::policy::repo_access`.

-- People given a permission on one repository directly ("outside collaborators" when the
-- owner is an organisation they do not belong to).
CREATE TABLE collaborators (
    repo_id     INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    user_id     INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    permission  TEXT    NOT NULL CHECK (permission IN ('read', 'write', 'admin')),
    added_by    INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (repo_id, user_id)
) STRICT, WITHOUT ROWID;

-- "Repositories shared with this user" (dashboard, readable-by checks).
CREATE INDEX collaborators_by_user ON collaborators (user_id);

CREATE TABLE teams (
    id           INTEGER PRIMARY KEY,
    org_id       INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    name         TEXT    NOT NULL COLLATE NOCASE,
    description  TEXT,
    created_at   INTEGER NOT NULL,
    UNIQUE (org_id, name)
) STRICT;

CREATE TABLE team_members (
    team_id  INTEGER NOT NULL REFERENCES teams (id) ON DELETE CASCADE,
    user_id  INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    PRIMARY KEY (team_id, user_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX team_members_by_user ON team_members (user_id);

CREATE TABLE team_repositories (
    team_id     INTEGER NOT NULL REFERENCES teams (id) ON DELETE CASCADE,
    repo_id     INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    permission  TEXT    NOT NULL CHECK (permission IN ('read', 'write', 'admin')),
    PRIMARY KEY (team_id, repo_id)
) STRICT, WITHOUT ROWID;

-- "Teams with access to this repository" (collaborators page, readable-by checks).
CREATE INDEX team_repositories_by_repo ON team_repositories (repo_id);

-- What every member gets on every repository of the organisation. `write` keeps the
-- behaviour of milestone 1.
ALTER TABLE org_profiles ADD COLUMN member_permission TEXT NOT NULL DEFAULT 'write'
    CHECK (member_permission IN ('none', 'read', 'write'));

-- Whoever leaves (or is removed from) an organisation loses everything it granted them,
-- in the same transaction, however the removal happened: team memberships (team members
-- must be organisation members) and collaborator entries on its repositories (such as
-- the admin entry a member gets for a repository they created). Someone who should keep
-- access after leaving is added back as an outside collaborator.
CREATE TRIGGER org_members_revoke_grants AFTER DELETE ON org_members
BEGIN
    DELETE FROM team_members
    WHERE user_id = OLD.user_id AND team_id IN (SELECT id FROM teams WHERE org_id = OLD.org_id);
    DELETE FROM collaborators
    WHERE user_id = OLD.user_id AND repo_id IN (SELECT id FROM repositories WHERE owner_id = OLD.org_id);
END;
