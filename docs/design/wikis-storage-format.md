# Wiki storage format

W0 schema and on-disk format contract for `rbit-wikis`. It implements the decisions in
the [authority and commit ADR](wikis-authority-commit.md). Status: design only. No
migration, table, Git layout, media store or redb table below exists, and none of
their behavior is tested. Table and column names are the proposed contract: an
implementation may change them before the first release that applies them, and never
after. Numeric limits come from the [budgets](wikis-budgets.md), HTTP behavior from
the [navigation contract](wikis-navigation-contract.md), and the controls these
formats must support from the [threat model](wikis-threat-model.md).

## Format versions

| Version | Where recorded | Initial | Unknown-version behavior |
| --- | --- | --- | --- |
| SQLite schema | sqlx migration history | first wiki migration | Older binary's migrator is expected to refuse the database (see Downgrade) |
| Wiki tree format | `wikis.format_version` and the `rbit-wiki-format` blob in every commit tree | 1 | Wiki is treated as unavailable for reads and writes with a stable code (derived, not stored); never rewritten |
| Path policy | `wikis.path_policy_version` | 1 | Lookups refuse; a rebuild under the writer lock re-keys paths |
| Render policy | Code constant in redb keys | 1 | Different key, so old entries are unreachable |
| redb tables | `_v1` table-name suffix plus a value header byte | 1 | Header mismatch is a miss and is overwritten |
| Media manifest | First line of every manifest blob | 1 | As tree format |

A binary supports a closed set of tree, path and manifest versions. It never upgrades a
wiki in place as a side effect of reading. A format upgrade is an explicit, intent-
protected rewrite that ships with its own migration and fixture.

## Migrations

Migrations are embedded with `sqlx::migrate!("./migrations")`
(`crates/rbit-storage/src/sqlite/schema.rs`). Files in `crates/rbit-storage/migrations/`
use sequential four-digit names. The newest as of this writing is
`0062_user_locale.sql`. Released migrations are immutable
(`docs/design/rewrite/security-contracts.md`). The wiki schema ships in two
migrations, each numbered at merge time with the next free number and never edited
afterward:

- `NNNN_wikis.sql` (W1, the first slice that queries wiki state: the repository tab
  and read routes read `wikis`, `wiki_pages` and `wiki_paths`; W2 adds the first
  writes): `wikis`, `wiki_pages`, `wiki_page_revisions`, `wiki_paths` (canonical rows
  only until W3), `wiki_mutation_intents`, `wiki_intent_pages`,
  `wiki_cleanup_intents` and their triggers.
- `NNNN_wiki_search_media.sql` (W3): `wiki_search_fts`, `wiki_media`,
  `wiki_revision_media`, plus alias use of `wiki_paths`.

Tables are `STRICT`, as every existing table is. Identifiers whose reuse would be
unsafe use `AUTOINCREMENT`, following `0048_actions.sql` and
`0060_credential_revocations.sql`. Object IDs are lowercase hex and checked for length
40, because wiki repositories are created as SHA-1, the same as existing repositories.
A SHA-256 wiki needs a new tree format version.

## SQLite schema

```sql
CREATE TABLE wikis (                       -- one incarnation per repository
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id             INTEGER NOT NULL UNIQUE
                        REFERENCES repositories (id) ON DELETE CASCADE,
    state               TEXT NOT NULL
                        CHECK (state IN ('provisioning', 'active', 'integrity_hold')),
    format_version      INTEGER NOT NULL CHECK (format_version >= 1),
    path_policy_version INTEGER NOT NULL CHECK (path_policy_version >= 1),
    head_oid            TEXT CHECK (head_oid IS NULL OR length(head_oid) = 40),
    catalog_revision    INTEGER NOT NULL DEFAULT 0 CHECK (catalog_revision >= 0),
    indexed_catalog_revision INTEGER NOT NULL DEFAULT 0,
    live_pages          INTEGER NOT NULL DEFAULT 0 CHECK (live_pages >= 0),
    media_bytes         INTEGER NOT NULL DEFAULT 0 CHECK (media_bytes >= 0),
    created_at          INTEGER NOT NULL,
    CHECK ((head_oid IS NULL) = (catalog_revision = 0))
) STRICT;

CREATE TABLE wiki_pages (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    wiki_id          INTEGER NOT NULL REFERENCES wikis (id) ON DELETE CASCADE,
    page_key         TEXT NOT NULL CHECK (length(page_key) = 32),  -- public stable ID
    title            TEXT NOT NULL,
    current_revision INTEGER NOT NULL CHECK (current_revision >= 1),
    parent_key       TEXT NOT NULL,         -- normalized parent path, '' at root
    sort_key         TEXT NOT NULL,         -- normalized last segment
    deleted_revision INTEGER,               -- tombstone: revision that deleted it
    updated_at       INTEGER NOT NULL,
    UNIQUE (wiki_id, page_key)
) STRICT;
CREATE INDEX wiki_pages_children ON wiki_pages (wiki_id, parent_key, sort_key)
    WHERE deleted_revision IS NULL;

CREATE TABLE wiki_page_revisions (
    page_id      INTEGER NOT NULL REFERENCES wiki_pages (id) ON DELETE CASCADE,
    revision     INTEGER NOT NULL CHECK (revision >= 1),
    kind         TEXT NOT NULL CHECK (kind IN
                 ('create', 'edit', 'restore', 'rename', 'delete', 'undelete')),
    commit_oid   TEXT NOT NULL CHECK (length(commit_oid) = 40),
    body_oid     TEXT CHECK (body_oid IS NULL OR length(body_oid) = 40),
    title        TEXT NOT NULL,
    display_path TEXT NOT NULL,
    restored_from INTEGER,
    author_id    INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    created_at   INTEGER NOT NULL,
    PRIMARY KEY (page_id, revision)
) STRICT, WITHOUT ROWID;

CREATE TABLE wiki_paths (                  -- one path namespace per wiki
    wiki_id      INTEGER NOT NULL REFERENCES wikis (id) ON DELETE CASCADE,
    path_key     TEXT NOT NULL,            -- normalized by the domain path policy
    display_path TEXT NOT NULL,
    page_id      INTEGER NOT NULL REFERENCES wiki_pages (id) ON DELETE CASCADE,
    kind         TEXT NOT NULL CHECK (kind IN ('canonical', 'alias')),
    created_at   INTEGER NOT NULL,
    PRIMARY KEY (wiki_id, path_key)
) STRICT, WITHOUT ROWID;
CREATE UNIQUE INDEX wiki_paths_canonical ON wiki_paths (page_id)
    WHERE kind = 'canonical';
CREATE INDEX wiki_paths_by_page ON wiki_paths (page_id, kind);

CREATE TABLE wiki_mutation_intents (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    wiki_id           INTEGER NOT NULL REFERENCES wikis (id) ON DELETE CASCADE,
    request_token     BLOB NOT NULL CHECK (length(request_token) = 16),
    actor_id          INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    expected_head     TEXT CHECK (expected_head IS NULL OR length(expected_head) = 40),
    target_head       TEXT NOT NULL CHECK (length(target_head) = 40),
    expected_catalog  INTEGER NOT NULL,
    state             TEXT NOT NULL
                      CHECK (state IN ('pending', 'finalized', 'aborted')),
    resolution        TEXT CHECK (resolution IN ('applied', 'rolled_forward',
                      'aborted_missing_objects')),
    created_at        INTEGER NOT NULL,
    resolved_at       INTEGER,
    CHECK ((state = 'pending') = (resolved_at IS NULL)),
    CHECK ((state = 'pending') = (resolution IS NULL)),
    CHECK (expected_head IS NULL OR expected_head <> target_head),
    UNIQUE (wiki_id, request_token)
) STRICT;
CREATE UNIQUE INDEX wiki_intents_one_pending ON wiki_mutation_intents (wiki_id)
    WHERE state = 'pending';
CREATE INDEX wiki_intents_resolved ON wiki_mutation_intents (resolved_at)
    WHERE state <> 'pending';

CREATE TABLE wiki_intent_pages (           -- fixed redo payload, bounded per mutation
    intent_id     INTEGER NOT NULL
                  REFERENCES wiki_mutation_intents (id) ON DELETE CASCADE,
    page_key      TEXT NOT NULL CHECK (length(page_key) = 32),
    kind          TEXT NOT NULL,            -- as wiki_page_revisions.kind
    base_revision INTEGER,                  -- NULL only for create
    new_revision  INTEGER NOT NULL,
    body_oid      TEXT,
    title         TEXT NOT NULL,
    path_key      TEXT NOT NULL,
    display_path  TEXT NOT NULL,
    reclaim_path  INTEGER NOT NULL DEFAULT 0 CHECK (reclaim_path IN (0, 1)),
    PRIMARY KEY (intent_id, page_key)
) STRICT, WITHOUT ROWID;

-- No foreign keys: the row must outlive the wiki and repository rows.
CREATE TABLE wiki_cleanup_intents (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id         INTEGER NOT NULL,
    wiki_id         INTEGER NOT NULL UNIQUE,
    created_at      INTEGER NOT NULL,
    attempts        INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_attempt_at INTEGER,
    last_error      TEXT CHECK (last_error IN ('io', 'busy', 'not_empty_unknown'))
) STRICT;

CREATE TRIGGER wikis_refuse_pending_delete BEFORE DELETE ON wikis
WHEN EXISTS (SELECT 1 FROM wiki_mutation_intents
             WHERE wiki_id = old.id AND state = 'pending')
BEGIN SELECT RAISE(ABORT, 'wiki has a pending mutation'); END;

CREATE TRIGGER wikis_record_cleanup AFTER DELETE ON wikis BEGIN
    INSERT INTO wiki_cleanup_intents (repo_id, wiki_id, created_at)
    VALUES (old.repo_id, old.id, CAST(strftime('%s', 'now') AS INTEGER));
END;
```

W3 additions:

```sql
CREATE VIRTUAL TABLE wiki_search_fts USING fts5(   -- rowid = wiki_pages.id
    title, body, tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER wiki_pages_fts_delete AFTER DELETE ON wiki_pages BEGIN
    DELETE FROM wiki_search_fts WHERE rowid = old.id;
END;

CREATE TABLE wiki_media (
    wiki_id     INTEGER NOT NULL REFERENCES wikis (id) ON DELETE CASCADE,
    content_id  TEXT NOT NULL CHECK (length(content_id) = 64),   -- SHA-256 hex
    byte_len    INTEGER NOT NULL CHECK (byte_len > 0),
    media_type  TEXT NOT NULL,              -- server-sniffed, allowlisted
    state       TEXT NOT NULL CHECK (state IN ('staged', 'committed')),
    uploaded_by INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    staged_until INTEGER,
    created_at  INTEGER NOT NULL,
    CHECK ((state = 'staged') = (staged_until IS NOT NULL)),
    PRIMARY KEY (wiki_id, content_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX wiki_media_staged ON wiki_media (staged_until) WHERE state = 'staged';

CREATE TABLE wiki_revision_media (
    page_id    INTEGER NOT NULL,
    revision   INTEGER NOT NULL,
    wiki_id    INTEGER NOT NULL,
    content_id TEXT NOT NULL,
    name       TEXT NOT NULL,
    PRIMARY KEY (page_id, revision, content_id),
    FOREIGN KEY (page_id, revision)
        REFERENCES wiki_page_revisions (page_id, revision) ON DELETE CASCADE,
    FOREIGN KEY (wiki_id, content_id)
        REFERENCES wiki_media (wiki_id, content_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
```

Notes on the schema:

- `repo_archives` (`0046_repo_archives.sql`) remains the only archive authority, and
  step 2 reads it. Wikis do not copy repository visibility or ACL columns.
- Per-wiki counters (`live_pages`, `media_bytes`) change only in the finalize
  transaction. Step 2 checks quotas against them, so quota checks never race a
  read-then-write. The instance-wide pending-intent bound is a `COUNT` over the
  partial index inside step 2.
- The FTS table stores its own copy of the current text of live pages. The copy is
  derived, rebuildable from the head tree, and maintained only in the finalize
  transaction, so `indexed_catalog_revision` equals `catalog_revision` unless a
  rebuild was interrupted. If they differ, search returns an explicit
  "search unavailable" state rather than stale hits.
- Cascades from `repositories` fire the `wikis` triggers. The existing
  `0020_issue_search.sql` already relies on cascade-fired triggers. If a pending
  intent exists, the BEFORE trigger aborts the whole repository delete statement.
  This is intended (see the ADR's lifecycle ordering), and the cascade behavior needs
  its own test.

## Paths, aliases and reclaim

`path_key` is produced by the single domain path policy at `path_policy_version`.
Normalization rules belong to the domain contract, not here. One primary key gives
canonical paths and aliases a single namespace, so a clash is a constraint failure
inside step 2.

- Rename turns the old canonical row into an `alias` row for the same page and
  inserts the new canonical row. Each page and each wiki has a bounded number of
  aliases (budgets). Rename never rewrites other pages' links. Links resolve through
  `wiki_paths` at read time.
- A create or rename may claim a path held by an alias, or by a tombstoned page's
  canonical row, only with an explicit `reclaim_path`. Reclaim deletes that row in the
  finalize transaction. A tombstoned page without a canonical row needs a free path
  to be undeleted.
- Aliases are scoped by `wiki_id`. A wiki is one incarnation of one repository, so an
  alias can never resolve into another repository's or another incarnation's pages.
  A deleted and recreated repository gets a new `wiki_id` with no paths.
- A path that resolves to a tombstoned page returns not found to readers. Writers see
  an undelete link (navigation contract).

## Git layout

```text
<data_dir>/wikis/<repo_id>/<wiki_id>.git    bare, SHA-1, created by rbit only
  HEAD                                      ref: refs/rbit/wiki/head
  refs/rbit/wiki/head                       the only ref
```

The directory sits beside `repositories/`, `attachments/` and `lfs/` (the layout in
`crates/rbit-server/src/backup.rs`), never under an owner directory. Repository
transfer and rename do not touch it. There are no other refs: no branches, tags,
per-page refs or keep refs. Pending targets are protected because pruning runs only
when no intent is pending, under Git's prune grace. No transport, fork, mirror or
Actions path opens this directory.

Every commit tree (format 1):

```text
rbit-wiki-format            blob  "1\n"
pages/<k0k1>/<page_key>/    tree  <k0k1> = first two hex chars (256-way fan-out)
  body.md                   blob  page body, UTF-8, LF; absent when tombstoned
  meta                      blob  "rbit-wiki-meta 1\n" + title line + revision line
  media                     blob  manifest; absent when the page references none
```

Fan-out keeps each edit's rewritten trees small for 10,000-page wikis. Trees contain
only `100644` blobs and `040000` trees. They never contain symlinks, gitlinks or
executable modes, and ingress rejects any of these. A tombstoned page keeps `meta`
and drops `body.md`. Its history stays reachable through earlier commits, but readers
use `wiki_page_revisions`, not `git log`.

Commits have one parent (the previous head) and the fixed author and committer
`rbit wiki <wiki@rbit.invalid>`, so exported history carries no account email.
Authorship authority is `wiki_page_revisions.author_id`. The message is the bounded
edit summary, followed by the trailers `Rbit-Wiki-Intent: <intent id>` and
`Rbit-Wiki-Format: 1`. The trailers are diagnostic and never authoritative.

Media manifest blob (format 1), lines sorted by content ID:

```text
rbit-wiki-media 1
<sha256-hex> <byte_len> <media_type> <percent-encoded name>
```

## Media objects

Media bytes are stored at `<data_dir>/wiki-media/<repo_id>/<wiki_id>/<c0c1>/<sha256>`,
written to a scratch file and renamed into place once complete, as LFS and release
files are. An upload inserts or refreshes a `staged` row under the per-wiki lock
before the rename. That insert checks the per-wiki media count and byte quotas
(budgets) against committed plus staged rows, so staged uploads cannot exceed them;
step 2 rechecks. A finalize that references the object marks it `committed`. Expired
`staged` rows are swept in bounded batches by the writer path: the row goes first,
then the file, both under the lock. Committed media is kept for the life of the wiki,
because history is never rewritten. This adapter's reuse, GC and backup contracts are
unvalidated (plan); W3 must validate them before adopting it.

## redb cache tables

| Table | Key | Value |
| --- | --- | --- |
| `wiki_render_v1` | `(wiki_id u64, body_oid [u8;20], render_policy u16)` | header `1`, sanitized HTML with link slots, link target list |
| `wiki_nav_v1` | `(wiki_id u64, head_oid [u8;20], nav_policy u16, parent_key &str)` | header `1`, one bounded navigation level |
| `wiki_cache_by_age_v1` | `(written u64, table u8, key bytes)` | `()` |

Rendered bodies are keyed by content and contain no resolved link state. Link targets
are resolved per request with one batched `wiki_paths` lookup after authorization. A
rename therefore never serves stale link text, which is the catalog dependency that
`crates/rbit-web/src/render/document_cache.rs` (keyed by content only) does not
model. If a later slice caches resolved links, its key must add `catalog_revision`
and `head_oid`. Navigation keys include `head_oid` rather than `catalog_revision`
alone, because a restored snapshot can repeat a catalog number with different
content. Bounding and pruning follow `crates/rbit-storage/src/kv/insights.rs`
(`ENTRIES_BY_AGE`). This document owns the derived-cache key dimensions; the budgets,
navigation contract and threat model refer to it. Keys carry no repository access
epoch: no such counter exists, and no cached value depends on the viewer's access,
because every lookup follows authorization.

## Backup, restore and downgrade

- **Backup.** `crates/rbit-server/src/backup.rs` covers neither `wikis/` nor
  `wiki-media/` today. W4 must add both, enumerated from the SQLite snapshot as
  repositories are. A wiki repository is mirrored after the snapshot, so its ref may
  be ahead of `head_oid`. Restore resolves that case (ADR window 15). The redb tables
  are not backed up.
- **Restore.** Before writes reopen: verify each `head_oid` and every
  `body_oid`/manifest reachable from finalized revisions, run the ADR's
  reconciliation on pending intents, check that `wiki_paths` canonical rows match
  live pages, and drop all `wiki_*_v1` redb tables.
- **Downgrade.** The wiki migration makes the database unopenable by older binaries,
  whether or not wikis are enabled. The identity storage design records the older
  migrator refusing an unknown migration. Wiki migrations need their own fixture in
  `crates/rbit-storage/tests/upgrade.rs` (threat model L3: test, do not assume).
  Rollback means restoring a pre-upgrade snapshot as a unit. A binary that meets a
  `format_version` it does not support treats that wiki as unavailable (derived at
  read time; no state is written) and never writes to it.

## Open questions

1. FTS5 stores a second copy of the current text of live pages, up to the page
   ceiling for each one. Is a contentless index (`contentless_delete`, which depends
   on the bundled SQLite version) with snippets read from Git preferable?
2. Is SHA-1 for wiki repositories acceptable, or should W2 start with SHA-256 and
   accept the differing tool support?
3. How long should finalized and aborted intent rows be kept (token replay window),
   and should pruning run in the writer path or in maintenance? (Same as ADR open
   question 2.)
4. Should media deduplicate across wikis? This design stores media per wiki, which is
   simpler to clean up but uses more space.
5. Does `provisioning` creation need its own intent row, or does the `wikis` row plus
   an idempotent `init` suffice?
