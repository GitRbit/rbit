# Repository social retirement acceptance

Acceptance evidence for the `repositories.social` ledger row ("Stars/stargazers,
language and permission-filtered contributions"), retired in two slices: stars, then
contributions. The row is `ported`.

## Stars (first slice)

| Behaviour | Owner after this slice |
| --- | --- |
| Read gate for star, unstar and stargazers (not found when unreadable) | `rbit_repositories::stars::RepositoryStars` |
| Idempotent commands answering with the viewer's fresh summary | `RepositoryStars::{star, unstar}` |
| Listing bound (500, newest first), viewer/administrator facts for starred lists | `RepositoryStars::{stargazers, starred_by}`, `MAX_LISTED` |
| `StarSummary` and `Stargazer` models | `rbit_repositories::stars` (re-exported unchanged by `rbit_services` and `rbit_storage::sqlite::stars`) |
| Star SQL, idempotence and the visibility predicate | `StarRepository` port implemented by `StarStore` in `rbit-storage/src/repository_stars.rs` |

Removed from `rbit-services/src/stars.rs`: `require_read`, the `MAX_LISTED` constant,
the id projection for counts, the count-to-summary mapping and the direct store calls.
`StarService` keeps its public API and maps `StarError::NotFound` to
`ServiceError::NotFound`.

Boundary: `scripts/rewrite/repository-stars-retirement.mjs` (run by `check.mjs`) rejects
`StarStore` or `sqlite::stars` in production code outside `rbit-storage`, and opening the
star store (`.stars()`) in production code anywhere but `rbit-services/src/stars.rs`.
Test fixtures may still seed stars directly. `scripts/tests/repository-stars-retirement.test.mjs`
covers the rules and the current tree.

Parity: each call opens one star store and runs the same statements in the same order
(write, then one summary query for commands); refusals run no SQL. Type identity of
`Stargazer` and `StarSummary` is preserved by re-export, so views and JSON are unchanged.


## Contributions (second slice)

| Behaviour | Owner after this slice |
| --- | --- |
| Which repositories a calendar scans: an organization's readable repositories, or every repository the viewer may read for a user | `rbit_repositories::contributions::repositories` over the `ContributionRepository` port, implemented by `RepositoryStore` (`rbit-storage/src/repository_contributions.rs`) |
| Private-email rule: noreply address always; account email only for the account itself or a site administrator; case-insensitive match | `contributions::{filters_authors, author_emails}` and `AccountTally` |
| Tallying: half-open second window, Euclidean UTC days, each commit once across repositories, repository counted when it contributed, partial history flag | `AccountTally`, `count_days` |
| Whole-day windows answered from all-time daily totals | `contributions::{whole_days, day_window}` |
| `Contributions` model | `rbit_repositories::contributions` (re-exported unchanged by `rbit_services::contributions`) |

`rbit-services::contributions` keeps the infrastructure: default-branch resolution,
history scans through the Git reader, the persistent and in-memory caches, scan
sharding and worker admission, and the noreply-domain configuration. The selection,
the email rule and all counting moved; the service unit test of day counting moved to
`rbit-repositories/tests/contributions.rs` with the same assertions.

Language statistics (`InsightsService::languages`) hold no decision beyond the loaded
repository's read access, which the caller's `RepoContext` already carries: they are
Git computation and cache only, so they stay service infrastructure.

Boundary: the same checker, `scripts/rewrite/repository-stars-retirement.mjs`, now
also rejects `contribution_repositories` in production code outside storage, and
author matching, day arithmetic, commit de-duplication or account-kind branching in
`rbit-services/src/contributions.rs` outside its test module.

Parity: the account calendar still lists repositories with one query (the same
statement per account kind), then looks up the user once for users only, in the same
order. Matching, windows and counting are the same expressions moved verbatim; the
email string is the same `to_string()` of the stored address. Cache keys, scan
admission and the per-tip daily cache are untouched.

## Results

One Linux run of the repository's own CI jobs on the final code, with tests limited to
the packages these commits change (`rbit-repositories`, `rbit-storage`,
`rbit-services`):

| Job | Result |
| --- | --- |
| Workspace formatting and all-target Clippy with `-D warnings` | pass |
| nextest for the changed packages | 1,294 run, 1,294 passed, none skipped |
| Licenses and advisories, front-end build | pass |
| Rewrite contracts | `check.mjs` 0 violations and the Node suites pass; `check-types.mjs` stops on the `identity_access_tokens.rs` resolution error already present in the base stack, fixed separately |

The passing set includes the new `rbit-repositories` fixtures (contributions 5,
stars 3, edits 5), the existing `rbit-services` integration suites unchanged
(`edits`, `repo_settings`, `access` and `transfers`, which seed and count stars)
and the contribution unit tests in `rbit-services` (default-branch-only
calendars, cancelled-scan cache reuse). A full workspace run and the Windows legs were
not run for this slice.

Locally: `node scripts/rewrite/check.mjs` reports zero violations,
`node --test scripts/tests/repository-*.test.mjs` passes, `cargo fmt --all --check`
passes and `workspace.mjs check` compiled the three packages with all targets.

### Stars slice results

Linux runs of the repository's own CI jobs (format, all-target Clippy with
`-D warnings`, nextest, rewrite contracts, licenses/advisories, front-end build):

| Run | Code | Result |
| --- | --- | --- |
| Forks commits only, on the previous stack | fork routing and boundary | Clippy and formatting clean; nextest 3,277 passed, 2 skipped, none failed; front-end and licenses pass |
| Forks and stars, on the previous stack | identical to the final code except one assertion in `rbit-repositories/tests/stars.rs` | nextest 3,277 passed, 2 skipped, none failed (the new star, fork-network and boundary tests included); Clippy failed only on `clippy::assert_is_empty` in that assertion, fixed in the final code |

On both runs the rewrite-contracts job's `check-types.mjs` stopped on an
`identity_access_tokens.rs` resolution error in the base stack, not in this change;
the base fix is included in the stack these commits now sit on. A full run of the
final tip was queued but had not completed when this record was written.

Locally on the final tip: `node scripts/rewrite/check.mjs` reports zero violations,
`node --test scripts/tests/repository-*.test.mjs` passes 18 of 18 and
`cargo fmt --all --check` passes. The existing service suites (`forks::`, `transfers::`,
`lfs::`, pulls, webhooks, Actions) ran unchanged inside those nextest totals.

## Migration, rollback and performance

No migration, schema, SQL, cache format or stored data changed. Rollback is the
previous binary against the same data. No request path gains a query. No release
measurement was taken and none is claimed.

## Cost

| Measure | Stars slice | Contributions slice |
| --- | --- | --- |
| Code commits | 2 | 2 |
| Files changed by code commits | 10 | 8 |
| Lines added / removed | 337 / 43 | 333 / 77 |
| Production Rust (`src`, non-test) | 6 files, +172 / −43 | 5 files, +193 / −76 |
| Tests and fixtures | 1 file, +106 | 1 file, +112 (one service unit test moved here) |
| Checker and policy scripts | one 31-line Node guard and a 26-line test | the same guard extended, +28 / −1 |

Tricky parts:

- The contribution service mixes policy with substantial infrastructure (a persistent
  cache, an in-memory hot set, scan sharding and detached workers). Only the decisions
  moved; the cache and admission code was left byte-for-byte to keep the existing
  cancellation and warm-cache tests meaningful.
- The private-email rule is a privacy boundary; it moved as one function with tests
  for each viewer class, and the matching stays case-insensitive.
- The service file was over the 400-line ceiling, so the extraction had to shrink it
  (404 to 349 lines) rather than grow it.

Estimate: a row that mixes a few pure decisions with heavy caching or Git work is one
focused session once the decision boundary is clear; most effort is in keeping the
infrastructure untouched.
