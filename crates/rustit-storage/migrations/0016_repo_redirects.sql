-- Old locations of renamed and transferred repositories: `owner/name` that used to name
-- repository `repo_id`. Web requests to an old location redirect; Git over HTTP and SSH
-- serve the repository there.
--
-- Maintained by the triggers below, so every way a repository arrives at or leaves a
-- location (create, fork, mirror, rename, transfer) keeps the table right:
-- * leaving `owner/name` records it as a redirect to the repository;
-- * arriving at `owner/name` removes any redirect from it, so a repository that exists
--   always takes precedence over an old name (a repository renamed back reclaims it).
-- Deleting the repository or the old owner removes its redirects.
CREATE TABLE repo_redirects (
    owner_id    INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    name        TEXT    NOT NULL COLLATE NOCASE,
    repo_id     INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (owner_id, name)
) STRICT;

CREATE INDEX repo_redirects_by_repo ON repo_redirects (repo_id);

CREATE TRIGGER repo_redirects_on_create AFTER INSERT ON repositories
BEGIN
    DELETE FROM repo_redirects WHERE owner_id = NEW.owner_id AND name = NEW.name;
END;

-- A change of letter case only is not a move: both spellings name the same location.
CREATE TRIGGER repo_redirects_on_move AFTER UPDATE OF owner_id, name ON repositories
WHEN OLD.owner_id <> NEW.owner_id OR OLD.name <> NEW.name COLLATE NOCASE
BEGIN
    DELETE FROM repo_redirects WHERE owner_id = NEW.owner_id AND name = NEW.name;
    INSERT OR REPLACE INTO repo_redirects (owner_id, name, repo_id, created_at)
    VALUES (OLD.owner_id, OLD.name, NEW.id, CAST(strftime('%s', 'now') AS INTEGER));
END;
