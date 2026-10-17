# Milestone 3 — L: Git LFS (as built)

Workstream L of [milestone 3](../milestone-3.md): the server side of Git Large File
Storage — the batch API, the basic transfer adapter, the locks API, `git-lfs-authenticate`
over SSH, LFS objects in the repository browser, backups and orphan clean-up. Migration
`0036_lfs` only.

## As built

**Domain (`rbit-core`).** `lfs`: `LfsOid` (exactly 64 lower-case hex digits, so it can
name a path; `shards()` gives the two directory levels), `LfsOperation` (`upload` /
`download`), `LfsPointer::parse` (the pointer-file format: version line first, `oid
sha256:…`, `size …`, extension lines allowed, ≤ 1024 bytes; anything else is an ordinary
file), `LfsLock` / `LfsLockId` / `LockOwner` and `validate_lock_path`. Config section
`[lfs]` (`config/lfs.rs`) with `enabled`, `max_object_bytes`, `token_lifetime_seconds`, and
`StorageConfig::lfs_dir()` = `<data_dir>/lfs`.

**Schema (`0036_lfs.sql`).**

| Table | Purpose |
|---|---|
| `lfs_objects (oid PK, size, created_at, unlinked_at)` | One row per stored object (only once its bytes are in place). `unlinked_at` is stamped by a trigger when a link is deleted, cascades included. |
| `lfs_repo_objects (repo_id, oid, created_at)` | Which repositories may use an object. `ON DELETE CASCADE` from `repositories` — deleting a repository unlinks its objects — and from `lfs_objects`. `WITHOUT ROWID`, index on `oid`. |
| `lfs_locks (id, repo_id, path, owner_id, created_at)` | One lock per path and repository; cascades with the repository and the owner's account. |

`LfsStore` (`Database::lfs()`): `record_object` (row + link in one transaction),
`link`, `linked_objects` (one query per batch via `json_each`), `linked_in_any`,
`copy_links`, `usage`, `orphans` / `delete_orphan` (the orphan condition is re-checked in
the `DELETE`, so a link added meanwhile always wins), and the lock queries.

**On disk.** `<data_dir>/lfs/<aa>/<bb>/<oid>`, stored once however many repositories use
it. Uploads stream into `<data_dir>/tmp/lfs/<random>.part` while SHA-256 and size are
computed, are refused unless both match the announced object (and the size fits
`max_object_bytes`), and are then renamed into place — or discarded when the bytes are
already there — and linked. Bytes are placed before the row is written, under a
process-wide mutex shared with the orphan sweep, so a sweep never deletes bytes an upload
just relied on. Interrupted uploads older than a day are swept at start-up.

**Which objects a repository may use (the dedup rule).** A repository downloads the
objects linked to it. Objects linked to another repository of its *fork network* (the
`ForkStore::network_readable` query) also count when the requester may read that
repository; they are then linked to this repository too (lazily, on first use), so they
survive the other repository's deletion. New forks copy their parent's links
(`DomainEvent::RepositoryForked`, in the background; the network rule covers the moment
before). The same rule decides whether an upload can be skipped: an object stored only for
an unrelated (or unreadable) repository must be uploaded again — proving possession —
before it is linked. Knowing an object id therefore never grants its content. This is what
makes `git clone` of a fork work, and pull requests merged from forks into their base.

**Tokens.** The batch API and `git-lfs-authenticate` hand out `Authorization: Bearer
lfs1.<payload>.<mac>`: a fixed binary payload (repository id, subject — anonymous, user
or deploy key —, read or write, expiry, and for transfer actions the one object and its
size), HMAC-SHA256 under a 256-bit key drawn from the OS at start-up, verified in constant
time (`Mac::verify_slice`). Nothing is stored. `LfsService::open_granted` turns a token
back into a `RepoContext`, **recomputing** the holder's access (suspension, removed
collaborators, deleted deploy keys end a token early) and capping it at the token's read or
write. Invalid tokens count as failed Git authentication attempts (`ThrottleScope::GitAuth`).

**HTTP API (`rbit-web/src/routes/lfs.rs`, `lfs/locks.rs`).** Under both
`/{owner}/{repo}.git/info/lfs/…` and `/{owner}/{repo}/info/lfs/…` (one `{repo}` segment
takes both; old names from renames and transfers work like smart HTTP):

| Route | |
|---|---|
| `POST …/objects/batch` | `operation` `upload`/`download`, `transfers` must include `basic` (422), `hash_algo` must be `sha256` (409), ≤ 1000 objects. Per object: `actions.download`, or `actions.upload` + `actions.verify` (each with `href`, `header.Authorization`, `expires_in`), nothing when already stored, or `error` 404 (missing on download) / 422 (bad id or size) / 413 (over `max_object_bytes`). |
| `GET …/objects/{oid}` | The bytes, streamed (`application/octet-stream`, `Content-Length`, `nosniff`). |
| `PUT …/objects/{oid}` | The body is the object; its size is the transfer token's, else `Content-Length` (411 without). 413 / 422 as above. |
| `POST …/objects/{oid}/verify` | `{oid, size}` → 200 once stored and linked, 404, 422. |
| `GET …/locks` | `path`, `id`, `cursor`, `limit` (≤ 100) → `{locks, next_cursor}`. |
| `POST …/locks` | `{path}` → 201 `{lock}`, 409 `{lock, message}` when locked already. |
| `POST …/locks/verify` | → `{ours, theirs, next_cursor}` for the caller. |
| `POST …/locks/{id}/unlock` | `{force}` → `{lock}`; 403 for someone else's lock unless forced by a repository administrator. |

Merged beside smart HTTP in `router()` without the page layers (no sessions, CSRF check,
compression or timeout; JSON bodies capped at 1 MiB, uploads stream). Authentication is
smart HTTP's own (`git_http::authenticate`: Basic with password or personal access token,
throttled, two-factor accounts need a token, the token's scope caps access) or an `lfs1`
token. Downloads need read access; uploads, verify and locks need write access (and an
account for locks); pull mirrors refuse uploads (`mirrors::PUSH_REFUSED`). Hidden
repositories answer 404 (401 with `LFS-Authenticate`/`WWW-Authenticate: Basic` to anonymous
callers, so clients ask for credentials). Every error is `{"message": …}` with
`application/vnd.git-lfs+json`. With `[lfs] enabled = false` every route answers 404.

**SSH.** `git-lfs-authenticate <repo> <upload|download> [oid]` is parsed as strictly as
the Git commands (`command::parse_lfs_authenticate`) and authorised exactly like a fetch
(download) or push (upload) over SSH — deploy keys (read-only ones cannot upload), mirrors,
old names, suspension. It prints `{"href": "<public_url>/<owner>/<repo>.git/info/lfs",
"header": {"Authorization": "Bearer lfs1…"}, "expires_in": …}` and exits 0; refused with an
explanation when LFS is disabled. Clients that try `git-lfs-transfer` first fall back to it
(tested with git-lfs 3.8).

**Browser.** The blob page recognises a pointer file and, when the repository may use the
object, shows the object in its place — size, line count, text/Markdown/highlighted code,
"Binary file not shown", or "too large" past `[repository] max_render_bytes` — with a
**Stored with Git LFS** badge; LFS images render inline from the raw URL
(`BlobContent::Image`). Without the object the pointer is shown with "Object not on this
server". `raw` streams the object (images by extension, else text or octet-stream by
sniffing the first 8000 bytes; the usual sandbox CSP and `nosniff`).

**Service API (for M's importer).** `LfsService::store(&RepoContext, &LfsOid, size, body:
impl AsyncRead)` stores and links one object for a repository the context may write to,
verifying size and SHA-256 — the same path HTTP uploads take. Also public: `batch`,
`verify`, `open`, `find`, `usage`, `issue`/`verify_token`/`open_granted`, the lock
operations, `prune_orphans`.

**Maintenance.** On the `[operations] maintenance_interval_minutes` schedule,
`LfsService` removes objects no repository has linked for 24 hours (`ORPHAN_GRACE`): row
first (re-checked), then bytes.

**Backups.** `rbit backup` copies every object the database snapshot lists to
`lfs/<aa>/<bb>/<oid>` (objects are immutable, so a plain copy is consistent; uploads in
progress live in `<data_dir>/tmp`); the manifest is format **3** with `lfs_objects` and
`missing_lfs_objects`.

**Tests.** Domain and config unit tests; `rbit-storage/tests/lfs.rs` (objects, links,
fork copies, orphans with the cascade trigger, locks) and `upgrade.rs` (tables exist,
start empty, cascade from a milestone 1 repository); `rbit-services/tests/lfs.rs` (store
refusals and dedup, batch decisions, "knowing an id grants nothing", fork network and
background link copy, token re-checks incl. collaborator removal and suspension, locks,
orphan pruning after repository deletion) plus token/lock unit tests; in-process protocol
tests in `rbit-web/src/routes/lfs/tests.rs` (upload/verify/download, `.git`-less
endpoint, anonymous/401/404/403, read-scoped tokens, mirrors, forged tokens, per-request
and per-object errors, token object scope, disabled LFS, locks, blob/raw pages);
`rbit-ssh` (`git-lfs-authenticate` for users, deploy keys, refusals, disabled);
`rbit-server/tests/backup.rs`; and `rbit-e2e/tests/e2e/lfs.rs` with the real `git lfs`
client — HTTP (track `*.bin`, push, fresh anonymous clone, blob/raw, locks, a fork that
clones with its parent's objects and accepts new ones) and SSH (push and clone over SSH,
objects through `git-lfs-authenticate`) — skipping without `git lfs` or OpenSSH.

## Deviations

1. **No archived repositories.** rbit has no archive state yet, so only pull mirrors
   refuse uploads. When archiving lands, refuse uploads where mirrors are refused
   (`lfs::Authorized::require_write` in `rbit-web`, and the SSH push authorisation that
   `git-lfs-authenticate upload` reuses).
2. **Token key per process.** Tokens are signed with a key generated at start-up, so a
   restart invalidates outstanding headers (clients re-run the batch or
   `git-lfs-authenticate`). Nothing secret is stored, and rbit runs as one process.
3. **Orphan clean-up** runs in `LfsService` on the maintenance schedule rather than inside
   `JobService::run_maintenance` (whose `MaintenanceSummary` stays unchanged); the grace
   period is a constant (24 h), not a config key.
4. **Lazy links.** A download (or blob page view) that finds an object through the fork
   network links it to the repository, so a read can write a row. It only ever exposes
   content the requester could already read and re-upload.
5. **`git-lfs-transfer`** (the pure-SSH transfer protocol) is not implemented; clients
   fall back to `git-lfs-authenticate` + HTTP.
6. **Uploads have no time limit** (like Git pushes); they are bounded by the announced
   size and `max_object_bytes`.
7. **Locks ignore `ref`.** Locks are per repository, not per branch (the `ref`/`refspec`
   fields are accepted and ignored).
8. **Forced unlocks need administrator access**, so a write-scoped token cannot force
   even for the owner (scopes cap access everywhere).
9. A crash between placing an object's bytes and writing its row leaves an unreferenced
   file that nothing sweeps (the next upload of the same object adopts it).
10. Only LFS images render inline on the blob page; images committed to Git still show
    "Binary file not shown" (unchanged).
11. `git_http.rs`: `authenticate`, `GitCaller` and `Denied` became `pub(super)` so the LFS
    routes authenticate exactly like smart HTTP (visibility only).
12. Commit gates: every commit passed `cargo fmt --all --check` and clippy (`-D warnings`)
    of the crates it touches and their tests; `cargo clippy --workspace --all-targets` and
    `cargo test --workspace` ran on the final state.

## CHANGELOG entry

Under `## [Unreleased]`, `### Milestone 3`, `#### Added`:

```markdown
- **Git LFS.** The server side of Git Large File Storage: the batch API and basic
  transfers under `/{owner}/{repo}.git/info/lfs` (also without `.git`), with streamed
  uploads checked against their SHA-256 and size; the locks API (`git lfs lock`, `locks`,
  `unlock`, lock verification on push); and `git-lfs-authenticate` over SSH. The same
  credentials as Git over HTTP (password or personal access token) or SSH keys; transfers
  use short-lived signed headers. Objects are stored once under `<data_dir>/lfs` however
  many repositories use them, forks share their parent's objects, and objects no
  repository uses are removed by maintenance after a day. The blob page shows the stored
  file (with a "Stored with Git LFS" badge) instead of the pointer, and Raw/Download
  serve it. Configured in the new `[lfs]` section.
```

`#### Changed`:

```markdown
- `rbit backup` also copies Git LFS objects (`lfs/`); the manifest format is now 3
  (`lfs_objects`, `missing_lfs_objects`).
```

`#### Upgrade notes`:

```markdown
- Migration `0036_lfs` adds three tables (`lfs_objects`, `lfs_repo_objects`, `lfs_locks`)
  and runs automatically at start-up. Git LFS is on by default; set `[lfs] enabled = false`
  to keep the endpoints (and `git-lfs-authenticate`) off.
- Behind a reverse proxy, allow request bodies up to `[lfs] max_object_bytes` (default
  2 GiB) and turn off request buffering for `/*/info/lfs/objects/*` (nginx:
  `client_max_body_size`, `proxy_request_buffering off`).
```

## README

Feature list (Git hosting bullet, or a new one):

```markdown
- **Git LFS**: batch API, streamed transfers, file locking and `git-lfs-authenticate` over SSH;
  objects are stored once and shared by forks; the browser shows the stored file, not the pointer.
```

Configuration table row:

```markdown
| `[lfs]` | Git Large File Storage | `enabled`, `max_object_bytes`, `token_lifetime_seconds` |
```

`[storage]` row: "`data_dir` (SQLite, redb, repositories, release files, LFS objects)".
Commands table, `rbit backup`: "(database snapshot, repository mirrors, release files, LFS
objects, manifest)".

## `deploy/rbit.example.toml`

```toml
[lfs]
# Serve the Git LFS API (batch, transfers, locks) under /<owner>/<repo>.git/info/lfs and
# `git-lfs-authenticate` over SSH. Objects live in <data_dir>/lfs (included in backups).
enabled = true
# Largest object a client may upload, in bytes (2 GiB). Raise the reverse proxy's body
# limit to match, and turn off its request buffering for LFS uploads.
max_object_bytes = 2147483648
# How long the signed headers handed to clients stay valid, in seconds.
token_lifetime_seconds = 3600
```

## ARCHITECTURE text

§2 crate responsibilities — `rbit-core`: add "`lfs` (object ids, pointer files, locks)";
`rbit-ssh`: "…`git-upload-pack` / `git-receive-pack` exec requests and Git LFS's
`git-lfs-authenticate`…"; services table row:

```markdown
| `LfsService` (`lfs`) | Git LFS: objects stored once under `<data_dir>/lfs` and linked per repository (fork networks share where readable), the batch decisions, signed transfer tokens (`lfs::tokens`), locks (`lfs::locks`), `store` for anything that brings objects into a repository, link copies for new forks and orphan clean-up. |
```

§3.2 on-disk layout, add:

```
├── lfs/<aa>/<bb>/<oid>            # Git LFS objects, named by SHA-256, stored once
└── tmp/
    └── lfs/<random>.part          # LFS uploads in progress
```

and the paragraph:

```markdown
Git LFS objects are content-addressed: `<data_dir>/lfs/<aa>/<bb>/<oid>` holds an object
once however many repositories use it, and `lfs_repo_objects` says which may. An upload
streams into `tmp/lfs/<random>.part` while its SHA-256 and size are computed, is refused
unless both match the announced object, and is then renamed into place (or discarded when
the bytes exist) and linked; bytes are placed before the row is written. A repository may
use the objects linked to it and, where the requester can read them, those of its fork
network (linked on first use); an object stored only for an unrelated repository must be
uploaded again before it is linked, so an object id alone never grants content. Deleting
a repository unlinks its objects; maintenance removes objects unlinked for a day.
```

§3.3 schema: the three tables of `0036_lfs` (see the table above). §3.6 backups: the
`lfs/<aa>/<bb>/<oid>` line in the tree and "format **3** (2 had no `lfs/`)"; counts gain
`lfs_objects`, `missing_lfs_objects`.

§4 request lifecycle tree, beside smart HTTP:

```
  ├── /{owner}/{repo}[.git]/info/lfs/…           Git LFS API (Basic auth, token or signed lfs1 header; streaming)
```

and a paragraph:

```markdown
**Git LFS** (`routes/lfs.rs`). The batch API, basic transfers and locks API run without
the page layers, like smart HTTP, and authenticate the same way, plus the short-lived
`Authorization: Bearer lfs1.…` headers the batch API and `git-lfs-authenticate` hand out:
HMAC-SHA256 over the repository, subject, read/write, expiry and (for transfers) one
object and its size, under a per-process key; access is recomputed whenever a token is
used. Downloads need read access, uploads and locks write access; pull mirrors refuse
uploads; hidden repositories are 404 (401 to anonymous callers). `[lfs] enabled = false`
turns every route into a 404 and refuses the SSH command. The blob page shows the object a
pointer names and `raw` streams it.
```

§5 security model rows:

```markdown
| Git LFS | Object ids are validated SHA-256 hex before they name a path; uploads are checked against size and digest before they are placed; an object stored for another repository is never linked without a fresh upload (or read access through the fork network). Transfer headers are HMAC-signed, short-lived, scoped to one repository (and one object), verified in constant time, and re-check the holder's current access; failures are throttled like Git credentials. |
```

## Integration requests

1. **Documentation** above (CHANGELOG, README, ARCHITECTURE, example config) for the
   integrator; `docs/deployment.md` needs the reverse-proxy note (body size and no request
   buffering for `/*/info/lfs/objects/*`).
2. **M (importer)**: fetch LFS objects with the source's batch API and store them with
   `services.lfs.store(&ctx, &oid, size, reader)` (a context with write access to the new
   repository); it verifies size and SHA-256 and links the object.
3. **E (editing in the browser)**: the blob page now shows the LFS object for pointer
   files. Editing such a file in the browser would commit text over the pointer; E should
   refuse (or hide "Edit") when the committed blob parses as `rbit_core::lfs::LfsPointer`.
   `templates/repo/blob.html` gained one badge line in the metadata row and the
   `BlobContent::Image` arm; `BlobPage` gained `lfs_stored`; in `routes/repo.rs` the blob
   handler calls `lfs::lfs_blob` right after reading the blob and `raw` calls
   `lfs::lfs_raw` — keep both sides when rebasing.
4. **Shared registration points touched** (appended blocks): `rbit-core` `lib.rs`
   (`pub mod lfs`) and `config/mod.rs` (`lfs` module, `Config::lfs` last); `rbit-storage`
   `sqlite/mod.rs` (`pub mod lfs`, `Database::lfs()`); `rbit-services/src/lib.rs`
   (`pub mod lfs`, `LfsService`, field, construction, `spawn_background`, `shutdown`);
   `rbit-web/src/routes/mod.rs` (`mod lfs`, `.merge(lfs::routes())` in `router()` after
   the release file routes — not in the page router, it needs no page layers);
   `rbit-storage/tests/upgrade.rs`; `rbit-e2e/tests/e2e/main.rs`. Cargo blocks:
   `rbit-services` `base64`, `rbit-ssh` `serde_json`, `rbit-server` dev `sha2`,
   `rbit-e2e` dev `rbit-ssh`.
5. **Backup format**: bumped to 3; if another workstream changes the backup layout,
   bump past it and keep both.
