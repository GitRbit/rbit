# Repository browse retirement acceptance

Acceptance evidence for the `repositories.browse` ledger row ("Refs, trees, raw blobs,
history, graphs, diffs, compare and blame"), following the
[identity retirement](identity-retirement.md) pattern.

## Scope

The row's Git work (reading refs, trees, blobs, history, graphs, diffs, compare and
blame) already lives in `rbit-git` readers, and access is decided when the repository
is opened (`RepositoryService::open_page`/`open`). What the web routes still decided
themselves moves to `rbit_repositories::browse`:

| Behaviour | Owner after this slice |
| --- | --- |
| `<revision>/<path>` split at the longest known branch or tag that is a whole leading segment | `browse::split_known_ref` |
| Landing revision: the default branch when it exists, else the first branch | `browse::landing_revision` |
| History page size (30) and overflow-safe offsets | `browse::{COMMITS_PER_PAGE, commit_offset}` |
| Raw download bound (100 MiB) | `browse::MAX_RAW_BYTES` |
| Case-insensitive tree filtering | `browse::matching_entries` |
| Path arithmetic for links and breadcrumbs | `browse::{join_path, parent_path, file_name}` |
| Which blobs render as images, and their media types | `browse::image_content_type` |

The routes reach it through `rbit_services::repositories::browse`, so `rbit-web` gains
no new crate dependency. `RepoPage::split` keeps its fast path (known refs first, the
reader's longest-prefix search otherwise) and `RepoPage::landing_revision` delegates.
`crates/rbit-web/src/routes/repo.rs` went from 697 to 656 lines.

## Boundaries that fail if legacy paths return

`scripts/rewrite/repository-browse-retirement.mjs`, run by `check.mjs`, rejects in
`rbit-web/src/routes/repo.rs` and `routes/repo/` the paging/raw limit constants, local
path, filter or image-type helpers, and the known-ref split rule.
`scripts/tests/repository-browse-retirement.test.mjs` covers the rules and the tree.

## Parity argument

The moved functions are the same expressions over names instead of `RefSummary` or
`TreeEntry` values; the web unit tests for commit offsets and known-ref splitting run
unchanged against thin wrappers with their original signatures. Routes, templates,
caching headers and error text ("page number is too large") are unchanged.

## Results

Linux run of the repository's own CI jobs on the final code, with tests limited to the
packages these commits change (`rbit-repositories`, `rbit-services`, `rbit-web`): every
job passed. Formatting and all-target Clippy with `-D warnings` were clean; rewrite
contracts passed, including `check-types.mjs`; nextest ran 1,539 tests, all passing,
1 skipped.

The passing set includes the new `rbit-repositories` `browse` fixtures (4) and,
unchanged, the web unit tests for known-ref splitting and commit-offset overflow, the
repository-document tests (links, candidates, bounded fallbacks) and the blob page
tests. An earlier run caught two callers in `repository_documents.rs` still reaching
the removed path helpers through `super`. A full workspace run, `rbit-e2e` and the
Windows legs were not run.

## Migration, rollback and performance

Nothing stored changed. Rollback is the previous binary. No request gains Git or SQL
work; `landing_revision` now computes the first branch eagerly alongside the default
search over the same in-memory refs.

## Not covered

Graph layout, diff and blame rendering are presentation (`rbit-web` views) over
`rbit-git` results and hold no repository decision; they stay where they are.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 2 (plus the shared record) |
| Files changed by code commits | 9 |
| Lines added / removed | 197 / 58 |
| Production Rust (`src`, non-test) | 5 files, +97 / −58 |
| Tests and fixtures | 1 file, +45 |
| Checker and policy scripts | a 26-line guard, a 27-line test, two checker lines |

Tricky parts: `rbit-web` has no direct dependency on `rbit-repositories`, so the module
is reached through the services re-export instead of adding a dependency edge; the web
unit tests were kept unchanged by leaving thin wrappers with their original signatures.
Estimate: half a session; most browse work was already Git reads and presentation.
