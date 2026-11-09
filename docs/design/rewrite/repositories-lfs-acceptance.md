# Repository LFS retirement acceptance

Acceptance evidence for the `repositories.lfs` ledger row ("LFS transfer, shared
objects, locks, authorization and quotas"), following the
[identity retirement](identity-retirement.md) pattern.

## Scope

| Behaviour | Owner after this slice |
| --- | --- |
| Batch admission: uploads need write access and an unarchived repository; at most 1,000 objects per batch | `rbit_repositories::lfs::admit_transfer`, `MAX_BATCH_OBJECTS` |
| Per-object answers: invalid id or negative size, download of a linked object (reporting the stored size), missing, upload skipped when linked with the same size and bytes on disk, too large, upload requested; objects found through the readable fork network count for later occurrences in the same batch | `lfs::decide_batch` over the `BatchPort` |
| Upload checks: write access, archive, size limit; received bytes larger than announced, size mismatch, digest mismatch | `lfs::{admit_store, upload_refusal}` |
| Verify step: linked with exactly the announced size | `lfs::verify_stored` |
| Locks: account with write access to create, verify and unlock; owners unlock their own, administrators anyone's with `force`; page size 1 to 100; cursor and id parsing; next-page cursor | `lfs::{locker, may_unlock, page_size, parse_cursor, list_lock_id, next_cursor}`, `MAX_LOCKS_PER_PAGE` |

`rbit-services::lfs` keeps the infrastructure and protocol plumbing: object paths,
streaming upload into a temporary file, placement under the object lock in a tracked
task, the object and link SQL, fork-network lookup and linking (`through_network`),
signed transfer tokens and their issuance, token-to-repository resolution
(`open_granted`, which combines account, deploy-key and repository access), the fork
link subscriber, orphan pruning and interrupted-upload sweeping. Its new
`batch_view::BatchView` supplies the port: one `linked_objects` query, the network
cached across one batch, and the bytes-on-disk check. It is registered in
`component-types.json`.

Removed from the service: the batch admission and the whole per-object decision
match, the upload size and received-bytes refusals, the verify comparison, and the
lock page-size, cursor, id-filter, locker, unlock permission and next-cursor bodies.
The public constants are re-exported unchanged.

## Boundaries that fail if legacy paths return

`scripts/rewrite/repository-lfs-retirement.mjs`, run by `check.mjs`, rejects in
production code outside `rbit-repositories/src` the LFS refusal texts, definitions of
`MAX_BATCH_OBJECTS` or `MAX_LOCKS_PER_PAGE`, and per-object batch matches on
`LfsOperation`. `scripts/tests/repository-lfs-retirement.test.mjs` covers the rules and
the current tree.

## Parity argument

- The batch protocol is unchanged: the same answers in request order, the same
  response sizes (the stored size for downloads), the same tokens issued for the same
  operation, object and size. The decision order is identical: parse, size sign, link
  lookup, network lookup only for unlinked valid objects, the disk check only for an
  upload whose linked size matches, then the size limit.
- Store and lock work is the same SQL in the same order; refusal texts and status
  mapping (`Forbidden`, `Archived`, `Invalid`, `NotFound`, `Conflict`) are unchanged.
- Object sharing keeps its rule: an unrelated repository's object is never linked
  without an upload, and the network is the viewer-readable one from
  `ForkQueries::network`.

## Results

Linux run of the repository's own CI jobs on the final code, with tests limited to the
packages these commits change (`rbit-repositories`, `rbit-services`, `rbit-web`): every
job passed. Formatting and all-target Clippy with `-D warnings` were clean; rewrite
contracts passed, including `check-types.mjs`; nextest ran 1,539 tests, all passing,
1 skipped.

The passing set includes the new `rbit-repositories` `lfs` fixtures (5) and, unchanged,
the `rbit-services` LFS unit and integration tests (8 each) and the `rbit-web` LFS route
tests: batch upload, verify and download, the stored size in download answers, per
request and per object errors, repository access, token scope, locks, disabled LFS and
the blob page serving LFS objects. An earlier run caught an unused shared fixture
helper in the new test file. A full workspace run and the Windows legs were not run.

## Migration, rollback and performance

No migration, schema, SQL, token format or stored data changed. Rollback is the
previous binary against the same data. Token issuance moved from inside the decision
loop to the answer mapping after it; it reads the clock the same number of times. No
release measurement was taken and none is claimed.

## Not covered

- `open_granted`'s access combination stays in the service: it composes repository
  access policy (`rbit-auth`), account suspension and deploy keys, which belong to the
  access and identity rows.
- Background pruning and placement concurrency are unchanged infrastructure.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 2 (plus the shared record) |
| Files changed by code commits | 12 |
| Lines added / removed | 534 / 86 |
| Production Rust (`src`, non-test) | 7 files, +329 / −86 |
| Tests and fixtures | 1 file, +143 |
| Checker and policy scripts | a 32-line guard, a 27-line test, two checker lines |

Tricky parts: object ids are a `rbit-core` type, so the batch application is generic
over the id and the port parses it; the fork-network cache had to keep "once per batch"
across a port boundary (a small mutex in the adapter); and `lfs.rs` was over the size
ceiling, so the adapter went into its own module. Estimate: about one session.
