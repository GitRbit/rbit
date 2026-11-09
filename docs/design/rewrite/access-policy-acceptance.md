# Repository access policy retirement acceptance

Acceptance evidence for the `access.policy` ledger row: the permission lattice,
collaborators, and SQL listing parity. It completes the
[collaborator slice](access-policy-collaborators.md); together they retire the row.

## What lives in `rbit-access` now

| Part | Owner |
| --- | --- |
| Permission lattice (`RepoAccess`) and the combination of visibility, ownership, organisation role, base permission, team and collaborator grants | `rbit_access::{RepoAccess, repo_access}` (before this row's slices) |
| Which stored grants count for a viewer (not anonymous visitors, site administrators or the owner), the projection of grants onto the access facts, and grant loading | `rbit_access::grants::{grants_apply, access, viewer_grants, GrantRepository}` (this slice) |
| Collaborator management | `rbit_access::collaborators` ([collaborator slice](access-policy-collaborators.md)) |

`RepositoryGrants<P>`, `Viewer<I>` and `Repository<I>` keep identifiers and the stored
permission type generic, so `rbit-access` still has no internal dependency. The storage
adapter (`rbit-storage/src/access_grants.rs`) implements `GrantRepository` on
`RepositoryStore` with the unchanged grant query and projects the stored row onto
`RepositoryGrants<Permission>`.

`rbit_core::access` is the compatibility path for domain records: `Grants` is
`RepositoryGrants<Permission>`, `repo_access(viewer, repo, grants)` projects `User` and
`OwnedRepository` and calls `rbit_access::grants::access`, and `Permission` converts into
`RepoAccess`. Services import it directly; `rbit_auth::{Grants, repo_access}` and
`rbit_auth::policy` remain as re-exports so existing callers and suites compile unchanged.
`viewer_grants` in the repository service delegates to `rbit_access::grants::viewer_grants`
through the storage adapter; the page loader's pre-joined grants use `grants_apply` and the
same projection.

## Legacy code removed

- `crates/rbit-auth/src/policy.rs` (the adapter and its rules). Its exhaustive tests moved
  unchanged to `rbit-core/src/access.rs`, beside the projection they exercise. `rbit-auth`
  no longer depends on `rbit-access`.
- The service's `grant_holder` and `policy_grants`.

## SQL listing parity

Listings filter in SQL with the shared `readable_by!` predicate, which cannot move out of
storage. `crates/rbit-storage/tests/readable_parity.rs` now proves it against the policy
directly: a fixture with a personal owner and organisations at every base permission,
members, a team grant, outside and member collaborators, an organisation owner, a site
administrator and a stranger (12 repositories, 9 viewers, 7 distinct readable sets).
For every viewer, `contribution_repositories`, `list_by_owner` (all owners) and
`readable_among` must return exactly the repositories that `rbit_access::grants::access`
allows with grants loaded through the access port. The service-level
`access::sql_listings_agree_with_the_policy` (owner listings, stars, dashboards, bulk
grants) still runs unchanged.

## Boundaries that fail if legacy paths return

`scripts/rewrite/access-policy-retirement.mjs`, on its own line in `check.mjs`:

- `crates/rbit-auth/src/policy.rs` may not exist;
- `repo_access`, `grant_holder`, `policy_grants`, `grants_apply` and grant structs may be
  defined only in `rbit-access` (the `rbit-core` compatibility path may define only the
  record projection and `repo_access` wrapper);
- `ViewerAccessFacts`/`RepositoryAccessFacts` are built only by the record adapters
  (`rbit-core/src/access.rs`, and the wiki authorization row projection in storage);
- in `rbit-storage/src`, the readability SQL (`visibility = 'public' OR`, the base
  permission `<> 'none'` test) appears only in `repositories.rs`, which defines
  `readable_by!`.

Against the pre-slice sources it reports four violations; on this tree, none.
`scripts/tests/access-policy-retirement.test.mjs` covers each rule and the whole tree.

## Results

Linux runs of the repository's own jobs in quick mode (whole-workspace format and
all-target Clippy with `-D warnings`, licenses and advisories, front-end build, rewrite
contracts, nextest for the changed packages `rbit-access`, `rbit-auth`, `rbit-core`,
`rbit-storage` and `rbit-services`). The workspace-wide test run is left to the
integrated batch.

| Job | Result |
| --- | --- |
| Format and all-target Clippy (whole workspace) | pass |
| Licenses and advisories, front-end build | pass |
| Nextest, changed packages | 1,447 run, 1,447 passed: `rbit-access` 33, `rbit-auth` 39, `rbit-core` 142, `rbit-storage` 453, `rbit-services` unit 264 and integration 516 |
| `node scripts/rewrite/check.mjs` and the script tests | 0 violations at this record's commit |
| `scripts/rewrite/check-types.mjs` | the strict reports for this slice's crates are clean; the gate fails only on the base's `identity_access_tokens.rs` self-type resolution, fixed separately in the stack |

Access-relevant groups in that run, all passing: the moved adapter tests in `rbit-core`
(the exhaustive reference table, the public baseline, anonymous visitors, organisation
grants on personal repositories, team grants needing membership, combination by
maximum); `rbit-access` lattice, combination and the new grant-loading test; storage
`readable_parity` and `collaborators`; services `access` 7 (including
`sql_listings_agree_with_the_policy`), `permission_matrix` 6, `repo_settings` 21,
`issues` 57, `pulls` 16, `lfs` 8, `notifications` 10, `protection` 5, `orgs` 11 and
`teams` 5. `rbit-web`, `rbit-ssh` and `rbit-e2e` were outside the changed-package set and
were not run; none of their files changed.

## Remaining notes

`issue_meta.rs` filters assignable users with its own write-access SQL
(`m.role = 'owner' OR … = 'write'`). It decides write access, not readability, and stays
with the issues row; a parity test like this one should cover it when that row retires.

## Migration, rollback and performance

No migration, schema or wire format changed. Rollback is the previous binary against the
same data. Request paths run the same grant query, skipped in the same cases. No release
measurement was taken and none is claimed.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (grant rules and loading, parity test, boundary) plus this record; the collaborator slice was 1 more |
| Files changed by code commits | 31 |
| Lines added / removed (all) | 666 / 287 |
| Production Rust (`src`) | 23 files, +407 / −285, of which about 190 added lines are the adapter's tests moved into `rbit-core/src/access.rs` |
| Tests | new 145-line storage parity test, one new `rbit-access` unit test, 4-case Node test |
| Checker and policy scripts | one new 40-line Node guard on its own `check.mjs` line |

Tricky parts:

- `rbit-access` cannot see `User`, `OwnedRepository` or `Permission`, so grants and facts
  are generic and `rbit-core` (which can implement `From<Permission> for RepoAccess`) owns
  the record projection; `rbit-auth` keeps only re-exports.
- Twelve service files imported the adapter; they now import `rbit_core::access`, while
  the unchanged `permission_matrix` suite keeps using the `rbit_auth` re-export.
- The readable-by rule has to stay in SQL; the parity test and the boundary are what keep
  the two statements of the rule together.

Estimate: once the decision is already pure, retiring an adapter row like this is about
half a day of code plus verification time; the parity test is the part worth the effort.
