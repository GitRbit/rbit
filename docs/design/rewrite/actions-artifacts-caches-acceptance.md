# Actions artifacts and caches acceptance

Acceptance evidence for the `actions.artifacts-caches` ledger row ("Checkout, cache/artifact
scope, safe archive extraction and retention"). It follows the
[identity retirement](identity-retirement.md) pattern and was retired in two slices; the
first was done together with the [backends row](actions-backends-acceptance.md).

## Ledger target: `rbit-actions`, not `rbit-runner`

The row was filed under `rbit-runner`, but most of its decisions are the server's, and
`rbit-runner` is a binary with no server dependency: the server cannot call into it. The
[rewrite plan](../ground-up-rewrite-plan.md) gives `rbit-actions` "server scheduling,
runners, jobs, leases, environments, secrets, caches and artifacts" and `rbit-runner` the
"local cache". The ledger target is therefore changed to `rbit-actions`. The row's three
owners are now:

| Part | Owner |
| --- | --- |
| Checkout, the runner-local cache (layout, LRU eviction, `cache.max_bytes`), archive packing and safe extraction (no write through a link, only new files) | `rbit-runner` (unchanged) |
| Rules both sides apply: key, `restore-keys` and version format, restore probe order, scopes from the run, scope/ref naming, content digest, archive root names | runner contract `rbit-workflow` (`protocol::caches`, `spec::cache_scopes`; slice 1) |
| The server's artifact store and shared cache: upload acceptance, quotas, retention, LRU eviction, expiry, orphan rules, download conversion | `rbit-actions` (`archives`, `artifacts`, `caches`, `downloads`; slice 2) |

No internal crate edge is added. `rbit-actions` uses `rbit-workflow`, which
`FEATURE_EDGES` already allows, plus the external `flate2` and `tar` (already in the lock
file) for download conversion.

## Slice 1: shared rules into the runner contract

Moved verbatim from `rbit-services` and `rbit-runner` into `rbit-workflow`: the key and
version rules (`MAX_CACHE_KEY_CHARS`, `MAX_RESTORE_KEYS`, `cache_key_fits`,
`valid_cache_key`, `valid_cache_version`), the restore order (`restore_probes`), scope/ref
naming (`scope_ref`, `ref_to_scope`), run scopes (`CacheScopes::for_run`,
`cache_ref_scope`), `sha256_from_hex` and the archive roots `w`/`h`. The write scope of an
untrusted run is `rbit_actions::trust::Isolation::cache_write_scope`, and
`scheduler::cache_scopes` only composes the two. Enforced by
`scripts/rewrite/actions-runner-retirement.mjs`.

## Slice 2: the server store behind ports in `rbit-actions`

| Decision | Legacy place | Now |
| --- | --- | --- |
| Upload checks: name, digest, lease before and after the body; room = min(instance cap, repository quota left); the two "too large" texts; digest match; name conflict text; replaced file removed before placing; failed placement deletes the row | `services::artifacts::upload_artifact` | `rbit_actions::artifacts::ArtifactStorage::upload` |
| Retention: `retention-days` clamped to 1..instance; `expires_at = now + days·86,400` | same | `artifacts::retention_days`, `ArtifactStorage::upload` |
| Newest attempt's artifact per name, sorted by name | `services::artifacts::latest_per_name` | `artifacts::latest_per_name` |
| Removal (file, then row) and expiry (256 per pass) | `delete_artifact`, `expire_artifacts` | `ArtifactStorage::{remove, expire}` |
| Cache lookup checks and probe loop | `services::caches::find_task_cache` | `rbit_actions::caches::SharedCache::find` |
| Restore: only the task's repository and read scopes; open, then count the use | `open_task_cache` | `SharedCache::open` |
| Save: checks, existing key skipped unless replacing, quota cap and text, digest, lease after body, `Exists` race, replaced file first, failed placement rollback, then eviction | `save_task_cache` | `SharedCache::save` |
| Eviction: least recently used first, never the saved entry, until within the quota (256 at most) | `evict_caches` | `SharedCache::evict` |
| Removal and expiry after `cache_retention_days` | `remove_cache`, `expire_caches` | `SharedCache::{remove, expire}`, `archives::days_before` |
| Orphan rules: 1 h for files without rows, 24 h for unfinished uploads, `tmp`, run directories, `<id>.tar.gz` | `sweeper` constants and parsing, `orphaned_cache_file` | `archives::{ORPHAN_AGE, STALE_UPLOAD_AGE, UPLOADS_DIRECTORY, ArtifactDirectory, cache_entry_file}` |
| Download conversion (zip with descriptors and ZIP64, tar.gz; regular files under `w/` only, no `..`, absolute or drive names; mode bits only) | `services::actions::downloads` | `rbit_actions::downloads` (moved with history, split into `downloads.rs` and `downloads/zip_format.rs`; tests in `downloads/tests.rs`) |

Ports are static traits with associated types (as in `claims`): `ArchiveFiles` and
`ArchiveUploads` for files; `ArtifactRecords`, `ArtifactUploadStore`, `ArtifactRetention`;
`CacheRecords`, `CacheTaskStore`, `CacheSaveStore`, `CacheRetention`. The application
holds no I/O, runtime or clock; refusals carry the exact runner-facing text and port
errors pass through unchanged.

Adapters in `rbit-services/src/actions`: `archive_files.rs` (temporary file, counting and
hashing, rename, removal; moved from `artifacts.rs`), `artifact_ports.rs` and
`cache_ports.rs` (ports over `actions_artifacts`/`actions_caches` and the lease checks
that `actions.scheduling` owns). `artifacts.rs` and `caches.rs` keep access checks
(`require_pages`, writers), read models for pages and REST, file opening and logging.
`rbit_services::actions::downloads` re-exports `rbit_actions::downloads`, so `rbit-web`
and the log archive are unchanged. The sweep keeps its directory walk and asks the
orphan rules.

## Legacy code removed

From `rbit-services`: the upload and save bodies, `latest_per_name`, `evict_caches`,
`remove_cache`, the expiry bodies, `SECONDS_PER_DAY`, `SWEEP_ARTIFACTS`, `EVICT_ENTRIES`,
the sweeper's `ORPHAN_AGE`/`STALE_UPLOAD_AGE` and directory parsing, the cache file name
parsing, `CacheLookup`/`CacheSave`/`Upload` definitions (re-exported from `rbit-actions`),
and `actions/downloads.rs` with the crate's `tar` and dev `zip` dependencies.

## Boundary that fails if legacy paths return

`scripts/rewrite/actions-artifacts-caches-retirement.mjs`, run by `check.mjs`, over
production `src` (comments and closing test modules stripped); `rbit-actions` and the
three adapter files are exempt:

- an `actions_artifacts()` `record`/`delete`/`expired` call, or an `actions_caches()`
  `record`/`delete`/`least_used`/`unused_since`/`touch`/`exact`/`newest_with_prefix`
  call, in `rbit-services`, `rbit-web` or `rbit-server`;
- a redefined `evict_caches`, `remove_cache`, `latest_per_name` or `receive`, or an
  `EVICT_ENTRIES`/`SWEEP_ARTIFACTS`/`ORPHAN_AGE` constant, in services Actions;
- the artifact quota or retention computed outside `rbit-actions`;
- `to_zip`, `to_tar_gz`, `entry_name`, `ZipWriter` or `ZIP64_ENTRY` defined outside it;
- `tokio::fs` rename, removal or open-for-write in services `artifacts.rs`/`caches.rs`.

Against `fdc7f41f` (slice 1) it reports nine violations; against this tree, none.
`scripts/tests/actions-artifacts-caches-retirement.test.mjs` covers each rule, the
allowed cases and the current tree. `actions-runner-retirement.mjs` keeps guarding slice 1.

## Gates

| Gate | Evidence |
| --- | --- |
| Behaviour parity | Same checks in the same order, same texts, same store calls in the same order (one per probe until a hit; touch after open; replaced file before placement; eviction after placement). Existing suites run unchanged: `rbit-web` `actions_artifacts`, `actions_caches`, `actions_api`, `actions_fork_runs` (uploads, leases), `actions_runner_tasks`; `rbit-services` `actions_settings`, `actions_sweeper`; the moved download tests. New: `rbit-actions` `tests/artifact_store.rs` (7) and `tests/cache_store.rs` (7), `archives` and `artifacts` unit tests. |
| Security | Untrusted runs keep the same quota, scopes and lease checks; a task still reads only its repository's read scopes; downloads still drop links, special files, `..`, absolute and drive names, and set-ID bits. |
| Migration and rollback | No schema, migration, stored path, route, protocol byte or error text change; latest migration 0073. Rollback is the previous binary. |
| Performance | One borrowed port per request; the same queries and file operations. Each port call opens the store handle afresh (a pool clone) where the old code reused one per request. No release measurement claimed. |

## Results

Linux quick check at the code tip: format, workspace-wide clippy `-D warnings`, rewrite
contracts (including the type checks), licenses, front-end build and nextest for the
changed packages (`rbit-actions`, `rbit-services`, `rbit-storage`: 1,260 tests) all
passed. The `rbit-web` HTTP suites (`actions_artifacts`, `actions_caches`, `actions_api`,
`actions_fork_runs`) are not in the quick scope; they run in the integrated full check. Locally: compile and clippy of `rbit-actions`,
`rbit-services` and `rbit-web` (all targets), `node scripts/rewrite/check.mjs` 0
violations, the new script test 3 of 3.

## Not covered

- The runner files stay over 400 lines under their ceilings (`steps/cache.rs` 929).
- Lease checks (`leased_task`, `cache_task`) stay in services: they belong to
  `actions.scheduling`.

## Cost

| Slice | Code commits | Files | Lines added / removed |
| --- | --- | --- | --- |
| 1 (shared rules, with backends) | 3 | 20 | 434 / 184 |
| 2 (server store) | 2 (plus this record and a formatting commit) | 22 | 1,971 / 624 |

Slice 2 by area: `rbit-actions` production +1,150 (of which about 500 lines are the
moved download conversion and its tests, so roughly 650 new); `rbit-actions` tests +444;
`rbit-services` +526 / −880 (three new adapter files, 458 lines); checker and policy
scripts: a 48-line guard, a 44-line test and two lines in `check.mjs`.

Tricky parts: choosing the owner (the ledger named the runner, the plan the server crate);
an upload body type that only the adapter knows, handled as an associated `Body` type of a
per-request port; refusal text that must reach the runner byte for byte; keeping the
store-handle order while the application owns the sequence; and a private `zip` module
shadowing the `zip` test crate after the move.
