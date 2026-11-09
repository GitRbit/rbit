# Organisation and team retirement acceptance

Acceptance evidence for the `access.organizations` and `access.teams` ledger rows. Both
retire in one slice because teams reuse the organisation management rule and member
lookup. `access.policy` (permission lattice, collaborators, SQL listing parity) stays
`baseline-retained`; it is untouched here.

## What moved

| Decision | Before | After |
| --- | --- | --- |
| Creation policy (`AnyUser`/`AdminsOnly`), name and profile text rules, website scheme check | `rbit-services/src/orgs.rs` | `rbit_access::organizations::{Organizations, can_create_organization}` |
| Management authority (site admin or owner; member `Forbidden`; nonmember `NotFound`) | `OrgService::require_manager`, `TeamService::require_manager` | one `organizations::require_manager`, used by both applications |
| Membership refusals (already a member, not a member, last owner, leaving as last owner) | `orgs.rs` `explain`, `leave` | `Organizations::{add_member, change_role, remove_member, leave}` |
| Deletion (name confirmation, repositories still owned, count wording) | `OrgService::delete` | `Organizations::delete` |
| Team visibility, self-removal, name/description validation, uniqueness message, organisation-only grant targeting | `rbit-services/src/teams.rs` | `rbit_access::teams::Teams` |
| `OrgRole`, `OrgCreationPolicy` | `rbit-core` | `rbit-access`; `rbit_core` re-exports at the same paths |
| `NewOrganization`, `OrgProfileUpdate`, `MembershipChange`, `TeamChange` | `rbit-storage` | `rbit-access`; storage re-exports (`NewOrganization` is an alias) |
| `CreateOrganization`, `UpdateOrganization`, `TeamInput` | `rbit-services` | `rbit-access`; services re-export |

Ports: `MemberDirectory` (user lookup, role lookup), `OrganizationRepository` and
`TeamRepository`. Record types (`Organization`, `Team`, members, summaries, granted
repositories, `BasePermission`, `Permission`) pass through as associated types, so
`rbit-access` keeps no internal dependency. Storage adapters:
`rbit_storage::{OrganizationAccessStore, TeamAccessStore}`. The SQL is unchanged and keeps
race authority for the last-owner rule, membership uniqueness, empty-before-delete,
organisation-only team members and grants, and team name uniqueness; the adapters map
`StorageError::Conflict` to the port outcomes (`AlreadyMember`, `Unique::Taken`) and
nothing else.

`OrgService` and `TeamService` stay as composition adapters: actor/organisation facts in,
`AccessError` to `ServiceError` (`Store` errors convert exactly as before), tracing and
audit entries out. Their public API, `OrgContext` and `TeamContext` are unchanged.

## Legacy code removed

- All decision code in `rbit-services/src/orgs.rs` and `teams.rs` (`require_manager`,
  `find_user`, `explain`, `optional_text`, `website`, `validate`, `find_repository`,
  `name_taken`, `not_in_org`, the team `access` rule) and their raw `OrgStore`/`TeamStore`
  and account/repository store use.
- The type definitions listed above from `rbit-core`, `rbit-storage` and `rbit-services`.

Read-only organisation lookups elsewhere (`actions/runners`, `transfers`, `webhooks`,
`collaborators`, `protection`) are other rows' code and still read the stores directly.

## Boundaries that fail if legacy paths return

`scripts/rewrite/access-retirement.mjs`, run by `scripts/rewrite/check.mjs`:

- the moved types may be defined only in `rbit-access` (re-exports and the storage alias
  are allowed), in any crate, tests included;
- production code outside `rbit-storage`/`rbit-access` may not call an organisation or team
  store write (`create`, membership, role, profile, base permission, delete, grants);
  unit-test modules and integration fixtures may seed stores;
- the two service adapters may not touch raw stores or define the moved decisions.

Run against the pre-slice sources it reports eleven violations; on this tree, none.
`scripts/tests/access-retirement.test.mjs` covers each rule and the whole tree.

The policy checker gains one reviewed seam, `rbit-services -> rbit-access`, so the
service adapters can drive the applications; `rbit-access` itself stays dependency-free
apart from `serde` (kept for `OrgRole`'s existing derives).

## Results

Local runs at the third code commit, all through the shared worktree helper. Windows
test suites beyond these targeted crates and the Linux CI jobs were not run for this
slice.

| Command | Result |
| --- | --- |
| `check -p rbit-access -p rbit-storage -p rbit-services --all-targets --locked` | clean |
| `clippy -p rbit-access -p rbit-core -p rbit-storage -p rbit-services -p rbit-web --all-targets --locked -- -D warnings` | clean except 16 `large_futures` findings in `rbit-services/tests/pulls.rs`; the unchanged baseline commit reports the identical 16 (16,424-byte `Fixture::new` future), so they predate this slice. Re-run with that one lint allowed: clean |
| `test -p rbit-access -p rbit-core -p rbit-storage --locked` | `rbit-access` 15, `rbit-core` 141 + 6 properties, `rbit-storage` unit 190 and integration 260 (orgs 7, teams 7, collaborators 7 among them): all pass |
| `test -p rbit-services --lib --test integration` filtered to `orgs::`, `teams::`, `access::`, `permission_matrix`, `admin::`, `transfers::`, `protection::`, `collaborators` | lib 7, integration 62 (orgs 11, teams 5, access 7, permission matrix 6, admin 20, transfers 8, protection 8, wiki pages 4 by name match): all pass |
| `node scripts/rewrite/check.mjs` | 0 violations |
| `node scripts/rewrite/check-types.mjs` (strict crates, component types, password boundary) | 0 violations; `rbit-access` types well under the 400-line ceiling |
| `cargo fmt --all --check` | passes |
| `node --test scripts/tests/*.test.mjs` | 641 of 650 pass, 8 skipped; the one failure (`htmx-vendor`) needs `node_modules`, which this checkout does not have, and is unrelated |

New tests, all in the passing set: `rbit-access` organisation rules (creation policy and
validation before writes, the manager matrix with no write reached by refused actors,
membership refusals and the last owner, deletion confirmation and the repository count,
website and profile rules) and team rules (visibility, uniqueness and description rules,
organisation-only members and self-removal, organisation-only grant targeting), over
in-memory ports; `scripts/tests/access-retirement.test.mjs` (four cases, including the whole
tree).

Not run: `rbit-web` route tests and `rbit-e2e`. No `rbit-web` source changed and the
service API is identical, so they are expected to pass unchanged, but this slice has no
run to cite.

## Parity argument

- Routes, templates, localization and HTTP status mapping are untouched: no `rbit-web`
  file changed. Services keep the same signatures and public types (moved types are
  re-exported at their old paths, so `TypeId`s and struct literals are unchanged).
- Every message string moved verbatim; the moved unit tests (`websites_must_be_http_links`,
  `optional_text_trims_and_limits`) are unchanged in `rbit-access`.
- Check order is preserved: authorization before validation before writes; teams resolve
  the named user before the self-or-manager check; `ensure_member` repeats the manager and
  user checks before adding, as before. Storage calls are the same functions in the same
  order; only the conflict-to-outcome mapping moved into the adapter.
- Audit entries and tracing fields are emitted by the same service methods with the same
  text.

## Migration, rollback and performance

No migration, schema, redb table or wire format changed; 0073 remains the latest and no
historical migration was edited. Rollback is the previous binary against the same data.
Request paths run the same queries; the adapters add no I/O. No release measurement was
taken and none is claimed.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (plus this record and the ledger rows) |
| Files changed by code commits | 26 |
| Lines added / removed (all) | 1,738 / 473 |
| Production Rust (`src`, non-test) | 18 files, +1,207 / −470 |
| Tests | 2 new `rbit-access` test modules (403 lines), one 69-line Node test |
| Checker and policy scripts | one new 47-line Node guard; one reviewed dependency seam |

Most of the added production lines are the ports and the storage adapters (about 470
lines of trait signatures and one-line delegations); the rules themselves moved nearly
line for line.

Tricky parts:

- `rbit-access` may not depend on `rbit-core` (which depends on it), so records cross the
  ports as associated types and `OrgRole`/`OrgCreationPolicy` had to move down into
  `rbit-access` with re-exports, rather than the rules importing them.
- Exact error parity needed the storage `Conflict` to be read in two different ways: for
  memberships and team names it is a refusal with a message, for organisation creation it
  must still surface as `ServiceError::Conflict`. The adapters map only the former.
- Shared build-lane contention dominated wall time: each helper command waited for other
  worktrees' leases.

Estimate for similar rows: a row whose decisions sit in one service file over one or two
stores, with no new transaction or schema, is about a day of focused work and three or four
commits, as here. Rows whose decisions are spread through SQL (`access.policy`) or need new
transactional behaviour are larger and should be scoped like the identity session stack.
