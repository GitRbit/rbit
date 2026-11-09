# Release and archive retirement acceptance

Acceptance evidence for the `repositories.releases` ("Releases and streamed attachments
with permissions and limits") and `repositories.archives` ("Bounded zip/tar archives and
cancellation") ledger rows, following the [identity retirement](identity-retirement.md)
pattern. Both rows retire in one slice: they share the "data and distribution" owner in
`rbit-repositories` and the same service-adapter shape.

## Scope

| Behaviour | Owner after this slice |
| --- | --- |
| Draft visibility (writers only; drafts do not exist for readers), page clamping, list and API pages | `rbit_repositories::releases::Releases::{list, page, by_tag, by_id}` |
| Download visibility (`/releases/download/{tag}/{name}`, asset by id) | `Releases::{asset_named, asset}` |
| Writer gate, archived refusal, workflow-task grant (own repository, `contents: write`) | `Releases::require_writer`, `ReleaseWriter` |
| Tag/title/notes validation, tag must exist or be created from a target, one release per tag, tag-creation refusals | `Releases::{create, update}` over `ReleasePort` |
| Publication announced once (create non-draft, first draft-to-release) | `Releases::{create, update}` → `ReleasePort::published` |
| Attached-file rules: name validation, per-release count, unique names, size limit, content-type sanitising and default | `Releases::upload` over `AssetFiles` |
| Row-before-bytes deletion order | `Releases::{delete, delete_asset}` |
| Release records, input/page/detail types, `NewRelease`/`ReleaseChanges`/`NewAsset`, validators, `human_bytes` | `rbit_repositories::releases::model` (re-exported at `rbit_core::release`, `rbit_storage::sqlite::releases`, `rbit_services::releases`) |
| Release persistence | `ReleaseRepository` port, implemented by `rbit_storage::sqlite::releases::ReleaseStore` (new adapter `repository_releases.rs`; same SQL) |
| Archive admission before any Git work, refusals and their text, revision-based safe name, file name, start order | `rbit_repositories::archives::RepositoryArchives::start` over `ArchivePort` |
| `MAX_CONCURRENT_ARCHIVES`, `archive_base_name`, `ArchiveError` | `rbit_repositories::archives` (re-exported by `rbit_services::archives`; `ArchiveError` is an alias with the same variants) |

`ReleaseService` and `ArchiveService` remain the composition adapters with unchanged
public APIs. The release service keeps the effects: tags reported like pushes
(`PushSource::Task` for tasks), `DomainEvent::ReleasePublished`, tracing, the asset lock,
the task tracker that makes placement and deletion independent of request cancellation,
and the files on disk (`releases/files.rs`: temporary uploads, placement, removal,
deleted-repository clean-up, stale-upload sweep). The archive service keeps the shared
`ConcurrencyLimiter` slots, `git archive` and the metrics counter. HTTP routes,
templates, localization and status mapping in `rbit-web` are unchanged; no `rbit-web`
file changed.

## Legacy code removed

1. All decision code from `rbit-services/src/releases.rs`: `require_writer`,
   `validated_text`, `validated`, the draft filter in `visible`, the page clamp, the
   create/update/upload sequences and their messages, `ReleaseActor::{check, account,
   source}`, the content-type sanitiser and `MAX_CONTENT_TYPE_LEN`.
2. The release type and validator definitions in `rbit-core/src/release.rs` and the
   insert/change records in `rbit-storage/src/sqlite/releases.rs` (now re-exports).
3. From `rbit-services/src/archives.rs`: `archive_base_name`, `MAX_CONCURRENT_ARCHIVES`,
   the `thiserror` `ArchiveError` enum and its `From<Refused>`, and the inline
   admit/resolve/start sequence.

The import job (`repositories.imports`) still writes imported releases through the store
and checks names with the moved validators; that is the imports row's code. Webhook
payloads still read releases from the store (`notifications`/webhooks row).

## Boundaries that fail if legacy paths return

`scripts/rewrite/repository-releases-retirement.mjs`, run by `scripts/rewrite/check.mjs`,
over every authored Rust file below `crates/` (tests included), comments ignored:

- release and archive records, validators, `human_bytes`, `archive_base_name` and the
  archive/page constants may be defined only in `rbit-repositories/src`;
- release policy text (tag, title, file-count and target messages) only in
  `rbit-repositories/src` among production sources;
- `database.releases()` in production only in `rbit-storage`, the release adapter,
  webhooks (payload reads) and the import job;
- the release adapter (code above its unit-test module) may not test drafts, call the
  validators, `count_assets`, `find_by_tag` or `ensure_writable`, or return
  `Forbidden`/`Invalid`/`Conflict`/`Archived` itself;
- the archive adapter may not hold `From<Refused>`, `thiserror` or revision sanitising.

Run against the pre-slice sources it reports seven violations; on this tree, none.
`scripts/tests/repository-releases-retirement.test.mjs` covers each rule, the allowed
cases and the whole tree.

No new internal crate edge: `rbit-core`, `rbit-storage` and `rbit-services` already
depend on `rbit-repositories` (`ADDED_EDGES`), and `rbit-repositories` still depends only
on `rbit-contracts` and `rbit-identity-models`.

## Results

Linux run of the repository's own CI jobs on the final code (quick mode, rebased on the
integrated stack), with tests limited to the packages these commits change (`rbit-core`,
`rbit-repositories`, `rbit-storage`, `rbit-services`). Every job passed:

| Job | Result |
| --- | --- |
| Workspace formatting and all-target Clippy with `-D warnings` | pass |
| Rewrite contracts (`check.mjs`, Node suites, `check-types.mjs`) | pass |
| nextest for the changed packages | 1,419 run, 1,419 passed, none skipped |
| Licenses and advisories, front-end build | pass |

The passing set includes the new `rbit-repositories` fixtures (`releases` 9,
`archives` 2) and, unchanged, the `rbit-services` release unit tests (5, including the
cancellation and id-reuse clean-up tests), the `rbit-services` integration `releases` (7)
and `archives` (6) suites, and the `rbit-storage` integration `releases` (4) and
`archives` (4) suites.

The first run after rebasing found only two `usize` casts in the new release fixture
that pedantic Clippy rejects (`cast_possible_wrap`, `cast_possible_truncation`); they
became checked conversions. No production code needed a change; the error-type
inference concerns noted during writing did not reproduce once compiled.

Not run: `rbit-web` (unchanged by these commits; its release, upload and archive
routes are exercised only by the full run), `rbit-e2e`, the full workspace suite and the
Windows legs.

## Parity argument

- Same checks in the same order. Create: task grant, writer, archived, text validation,
  tag lookup, then (missing tag) empty target, existing release for the tag, tag creation;
  then insert, publication, re-read. Update: grant, writer, archived, row lookup, text
  validation, tag must exist. Upload: grant, writer, archived, release, name, count,
  duplicate name, streamed size, limited insert. Every message moved verbatim; the
  `TagError` mapping reproduces the former `GitError` match.
- Same queries: each former `self.store()` call is the same `ReleaseStore` method with the
  same arguments; `by_tag` still loads assets before `find_asset`. Constructing the store
  once per command instead of per query only clones the pool handle (no connection, same
  `releases` pool label).
- Permissions unchanged: the application receives `access.can_write()`, the archived
  flag and the task's repository and `contents` grant, exactly the former inputs. Asset
  downloads still take the read lock, decide visibility, then open the bytes and count.
- Files and cancellation unchanged: temporary upload, size abandonment, placement inside
  a tracked task under the asset write lock, row removal on failed rename, row-then-bytes
  deletion, id-reuse-safe clean-up and the start-up sweep are the same functions moved to
  `releases/files.rs`. The unchanged unit tests (`completed_asset_upload_survives_request_cancellation`,
  `cancelled_deletion_keeps_row_and_file_together_before_id_reuse`,
  `delayed_repository_cleanup_preserves_reused_ids_current_assets`) pin this.
- Archives: admission still comes first and never waits; the slot still travels with the
  body; the same `Refused` outcomes map to the same variants and `Retry-After`; streaming
  limits (idle/maximum duration in `rbit-web`), `ETag`/cache headers and the
  `rbit_archives_total` counter are unchanged. `ArchiveError` displays the same text.

## Migration, rollback and performance

No migration, schema, SQL or stored data changed; 0073 remains the latest and no
historical migration was edited. Rollback is the previous binary against the same data.
No request path gains a query, lock or allocation beyond borrowed adapter structs; no
release measurement was taken and none is claimed.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (application, service delegation, checker) plus this record |
| Files changed by code commits | 18 |
| Lines added / removed | 1,988 / 768 |
| Production Rust (`src`, non-test) | 13 files, +1,373 / −768 (`rbit-services` releases 934 to 535 lines, archives 150 to 140) |
| Tests and fixtures | 2 files, +474 |
| Checker and policy scripts | a 57-line guard, an 82-line test, two checker lines |

Tricky parts:

- The release service mixed permission, validation and tag decisions with file
  placement, locks and cancellation-safe tasks; only the decisions moved, and the file
  handling moved verbatim into `releases/files.rs` so its cancellation tests stay
  meaningful.
- The slice was first written without compiling. After rebasing onto the integrated
  stack, the only failures were two test casts, which shows the value of the
  port-shaped fixtures: the production adapters compiled and passed the existing
  suites unchanged.
- Rebasing kept the other rows' module lines in `rbit-repositories/src/lib.rs`,
  `rbit-storage/src/lib.rs`, `check.mjs` and the ledger; only this slice's lines were
  added.

Estimate: a two-row slice of this size, with file and concurrency infrastructure to keep
untouched, is about two sessions including the rebase and verification.
