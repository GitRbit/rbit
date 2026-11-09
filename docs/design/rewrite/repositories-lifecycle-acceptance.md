# Repository lifecycle and listing retirement acceptance

Acceptance evidence for two ledger rows, `repositories.lifecycle` ("Repository creation,
settings, visibility, archive and deletion") and `repositories.listing` ("Explore,
search, sorting, paging and private access filtering"), following the
[identity retirement](identity-retirement.md) pattern. The prepared designs are
[lifecycle applications](../repository-lifecycle-applications.md) and
[public discovery](../repository-discovery.md).

## Lifecycle

| Behaviour | Owner after this slice |
| --- | --- |
| Creation, shared owner authorization, creator grant, disk compensation | `rbit_repositories::creation` (already production) |
| Description, visibility, default branch, rename, archive/unarchive, settings-page deletion | `rbit_repositories::settings::RepositorySettings` (already production) |
| Site-administrator deletion: confirmation, metadata first, not-found when already gone, best-effort disk removal, event and audit | `rbit_repositories::settings::AdminRepositoryDeletion` (new) over `AdminDeletionPort`, adapted by `rbit-services/src/admin/repository_deletion.rs` |

Removed: the administrator deletion decision body from
`rbit-services/src/admin.rs` (408 to 387 lines); the disk helper stays there for user
deletion, which still removes repository directories itself. The administrator check and the
repository lookup stay in `AdminService::repository`, unchanged and in the same order.

Left with other rows, recorded rather than claimed: imports and pull mirrors update
general settings directly as system work after a fetch (`repo_settings().update_general`
in `imports/job.rs` and `mirrors.rs`), and imports create their own repository rows.
Those belong to `repositories.imports` and `repositories.mirrors`. Deleting a user
cascades to their repositories inside the account deletion path.

## Listing

| Behaviour | Owner after this slice |
| --- | --- |
| Explore search normalization, page size, page clamping and offsets | `rbit_repositories::discovery::RepositoryDiscovery` (already production) |
| Lenient explore query parsing (unknown sort/archive values default, bad page is 1, trimmed text) | `RepoSearch::from_query` (new; was in the web handler) |
| Owner listings for a viewer, owner-level management listing, dashboard (100 cap), uncapped member page (count then page), recent public | `rbit_repositories::discovery::RepositoryListings` (new) over `RepositoryListingCatalog`, implemented by `RepositoryStore` in `rbit-storage/src/repository_discovery.rs` |
| Private access filtering | Storage's shared `readable_by!` predicate with the viewer id and administrator flag, unchanged |

Removed: the service's `DASHBOARD_LIMIT` and its direct listing calls, the web
handler's query parsing, and the Actions owner-secret listing's direct store call (now
`RepositoryListings::all_for_owner` through a crate helper, the same statement).

## Boundaries that fail if legacy paths return

`scripts/rewrite/repository-lifecycle-retirement.mjs`, run by `scripts/rewrite/check.mjs`,
over production code outside storage and `rbit-repositories`:

- listing statements (`list_by_owner`, `list_for_member`, `count_for_member`,
  `list_recent_public`, `store.page_for_member`) or a `DASHBOARD_LIMIT` definition;
- `RepoSort::parse` or `ArchivedFilter::parse` in `rbit-web`;
- `store.delete_repository(` outside the administrator deletion adapter;
- the administrator deletion confirmation text.

`scripts/tests/repository-lifecycle-retirement.test.mjs` covers each rule, the allowed
owners and the current tree. `DeletionEffects` is registered in `component-types.json`.

## Parity argument

- Each listing call is the same statement with the same arguments: the viewer id and
  administrator flag, the 100 dashboard cap, count before page, the recent limit the
  caller passes. No result is filtered or re-sorted after loading.
- Explore parsing is the same expressions; the URL query text is the trimmed text, as
  before.
- Administrator deletion keeps its order (confirmation, metadata, disk, log, event,
  audit), its not-found result and its logged-only disk failure; texts are unchanged.

## Results

Linux runs of the repository's own CI jobs with tests limited to the packages these
commits change (`rbit-repositories`, `rbit-storage`, `rbit-services`, `rbit-web`),
on top of the base stack with its access-token re-export fix:

| Job | Result |
| --- | --- |
| Workspace formatting and all-target Clippy with `-D warnings` | pass |
| nextest for the changed packages | 1,982 run, 1,982 passed, 1 skipped |
| Licenses and advisories, front-end build | pass |
| Rewrite contracts | `check.mjs` 0 violations and Node suites pass; `check-types.mjs` as below |

The passing set includes the new `rbit-repositories` fixtures (`listings` 2,
`admin_deletion` 2) and the existing suites unchanged: `rbit-services` integration
(repository settings, access listings, administrator deletion, archives, forks,
transfers and Actions settings), `rbit-web` integration (explore localization,
listings, admin configuration) and the web unit tests for the home dashboard and
landing page, the explore response shape and the admin routes. An earlier run caught a wrong archive value
in the new parsing fixture (`only` instead of `true`); the parser itself was right.
The structural type check then asked for an explicit module path in
`discovery/listings.rs`, fixed in the final code.

Final run on the final code: every job passed, including rewrite contracts with
`check-types.mjs` (strict crates, components and password boundary); nextest ran 1,982
tests, all passing, 1 skipped.

A full workspace run and the Windows legs were not run for this slice.

## Migration, rollback and performance

No migration, schema, SQL or stored data changed. Rollback is the previous binary
against the same data. No request path gains a query. No release measurement was taken
and none is claimed.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (plus this record) |
| Files changed by code commits | 16 |
| Lines added / removed | 521 / 37 |
| Production Rust (`src`, non-test) | 10 files, +263 / −37 |
| Tests and fixtures | 2 files, +174 |
| Checker and policy scripts | one Node guard, its test, two checker lines, one component entry |

Tricky parts:

- Most of both rows was already production: creation, settings and explore had
  prepared applications in use. Retirement meant finding what still decided outside
  them: administrator deletion with its own (metadata-first) order, the dashboard cap
  and listing calls in the service, lenient query parsing in the web handler, and an
  Actions settings listing that called storage directly.
- Two files at their size ceiling (`admin.rs`, `actions/settings.rs`) shaped the
  changes: the deletion adapter went into its own module and the Actions call into a
  one-line crate helper.
- User deletion shares the disk-removal helper with the old administrator path, so the
  helper stays in the admin service.

Estimate: rows whose applications already exist are about half a session each; the
time goes into finding the remaining callers and keeping size-capped files flat.
