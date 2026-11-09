# Repository forks retirement acceptance

Acceptance evidence for the `repositories.forks` ledger row ("Fork network, object
sharing and cross-fork access"), following the [identity retirement](identity-retirement.md)
pattern. The prepared design is [fork application ownership](../repository-fork-ownership.md).

## Scope

| Behaviour | Owner after this slice |
| --- | --- |
| Fork creation: read gate, owner, name, inherited settings, copy/record/rollback order, fork event | `rbit_repositories::forks::ForkCreation` (already production) |
| Parent visibility, listing/count paging, fork flags, private-ancestor walk | `rbit_repositories::forks::ForkQueries` (already production for `ForkService`) |
| Readable fork network for cross-fork pull heads and LFS object sharing | `ForkQueries::network` (new) over `ForkReadRepository::network_readable` |
| Fork flags in Actions, webhooks, issue/pull/status hooks and pull JSON | `ForkQueries::{is_fork, forks_among}` through `rbit_services::forks` (new routing) |
| SQL (relationship edge, visibility predicate, network depth 10) | `rbit-storage` `ForkStore` behind `ForkReadStore`/`ForkCreationStore` (unchanged) |

`rbit-services::forks` remains the composition adapter: native copy under the source
lease, relationship recording, rollback diagnostics and the fork event. The rules that
were documented on the service module now live on `rbit_repositories::forks`.

## Legacy code removed

1. Eleven direct fork-store reads that bypassed `ForkQueries`: `forks_among(..).contains(..)`
   fork-flag checks in Actions chains (two), hooks, dispatch, schedules and triggers,
   issue, pull and status hooks, webhooks, and the pull JSON flag lookup. They call
   `crate::forks::{is_fork, forks_among}`, which run the same single query.
2. Two direct fork-network reads: pull head resolution (`PullService::network`) and LFS
   object sharing (`through_network`). Both call `crate::forks::network`, which runs the
   same SQL with the same viewer and administrator facts.
3. The service-local copies of the fork rules documentation.

## Boundaries that fail if legacy paths return

`scripts/rewrite/repository-forks-retirement.mjs`, run by `scripts/rewrite/check.mjs`,
over every authored Rust file below `crates/` (tests included, comments ignored):

- `.forks()`, `sqlite::forks` or `ForkStore` outside `rbit-storage` and
  `rbit-services/src/forks*`.
- `network_readable` outside `rbit-storage` and `rbit-repositories`.
- `fork_repository` (the native copy) outside `rbit-git` and `rbit-services/src/forks*`.

`scripts/tests/repository-forks-retirement.test.mjs` covers each rule, the allowed
owners and the whole current tree.

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

## Parity argument

- Every replaced call opens one fork store and runs the same statement it ran before:
  `ForkQueries::is_fork` is `forks_among(&[repo]).contains(&repo)`, `forks_among` passes
  the slice through, and `network` forwards the viewer id and administrator flag to the
  unchanged `network_readable` SQL. Errors convert to `ServiceError` exactly as the `?`
  conversions did.
- LFS keeps its per-batch network cache; pull head resolution keeps its candidate
  ordering. No route, template, catalog text or serialized field changed.

## Migration, rollback and performance

No migration, schema, SQL or stored data changed. Rollback is the previous binary
against the same data. No request path gains a query; the wrappers are borrowed
values. No release measurement was taken and none is claimed.

## Not covered

- The lifecycle gaps recorded in the ownership design remain: cancellation after
  metadata creation, failed disk cleanup and the source lease ending before the edge is
  recorded can still need repair.
- The relationship edge is still written by the service effect (`ForkStore::record`)
  rather than by the creation store port; moving it would change store-handle order.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 2 (plus the shared record commit) |
| Files changed by code commits | 21 |
| Lines added / removed | 168 / 39 |
| Production Rust (`src`, non-test) | 17 files, +66 / −39 |
| Tests and fixtures | 1 file, +18 |
| Checker and policy scripts | one 37-line Node guard, a 45-line test, two checker lines |

Tricky parts:

- Creation, parent, listing and ancestry were already production through the prepared
  application; the remaining work was finding the bypasses. Fork flags were read
  straight from the store in eleven places across Actions, hooks and webhooks, and
  the fork network (the boundary for cross-fork pull heads and LFS sharing) had no
  application method at all.
- The boundary allows the service fork module to keep the store handle for the
  relationship effect, so the rule is scoped by path rather than by type.

Estimate for similar rows: a row whose decisions are already in its target crate but
whose store is still read directly elsewhere costs about two code commits and 20 files
of mostly one-line routing changes.
