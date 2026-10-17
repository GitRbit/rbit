-- Releases: notes on an existing tag, one release per tag and repository.
-- `published_at` is set the first time a release stops being a draft.
CREATE TABLE releases (
    id             INTEGER PRIMARY KEY,
    repo_id        INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    tag_name       TEXT    NOT NULL,
    name           TEXT    NOT NULL,
    notes          TEXT    NOT NULL DEFAULT '',
    is_draft       INTEGER NOT NULL DEFAULT 0 CHECK (is_draft IN (0, 1)),
    is_prerelease  INTEGER NOT NULL DEFAULT 0 CHECK (is_prerelease IN (0, 1)),
    author_id      INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    published_at   INTEGER,
    UNIQUE (repo_id, tag_name)
) STRICT;

-- The releases list (newest first) and the latest published release.
CREATE INDEX releases_by_repo ON releases (repo_id, is_draft, published_at DESC);

-- Metadata of files attached to releases; the bytes live on disk at
-- `<attachments_dir>/<repo_id>/<id>`, named by id only.
CREATE TABLE release_assets (
    id              INTEGER PRIMARY KEY,
    release_id      INTEGER NOT NULL REFERENCES releases (id) ON DELETE CASCADE,
    repo_id         INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    name            TEXT    NOT NULL,
    size            INTEGER NOT NULL CHECK (size >= 0),
    content_type    TEXT    NOT NULL,
    sha256          BLOB    NOT NULL CHECK (length(sha256) = 32),
    download_count  INTEGER NOT NULL DEFAULT 0,
    uploader_id     INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    created_at      INTEGER NOT NULL,
    UNIQUE (release_id, name)
) STRICT;

CREATE INDEX release_assets_by_repo ON release_assets (repo_id);
