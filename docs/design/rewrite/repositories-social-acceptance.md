# Repository social: stars slice

Evidence for the first slice of the `repositories.social` ledger row ("Stars/stargazers,
language and permission-filtered contributions"). The stars part is retired here; the
row stays `baseline-retained` because contributions and language statistics remain in
`rbit-services`.

## Retired: stars

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

## Remaining for the row

- Contributions (`rbit-services/src/contributions.rs`): repository selection per
  account kind, the private-email matching rule (noreply always; the account email only
  for the account itself or a site administrator), day tallying and the history cache
  and worker admission. The privacy rule and tallying are the next extraction
  candidates for `rbit-repositories`; the cache and workers stay service infrastructure.
- Language statistics (`rbit-services/src/insights.rs`): Git computation and cache only,
  gated by the loaded repository context. No policy beyond that gate was found.

## Results

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
