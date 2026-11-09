# Site administration retirement acceptance

Acceptance evidence for the `operations.admin` ledger row, re-targeted from `rbit-server`
to `rbit-access` and split.

## Re-targeting and split

The operations-updates assessment found that the row mixed two kinds of code. The
request-path decisions (who may administer, the last-administrator and self-action
guards, deletion confirmations, listing normalisation and settings change descriptions)
are authorization rules over site-administrator authority. They sit with the other
authorization rules in `rbit-access`, which already owns organisation, team, collaborator,
protection and repository access. `rbit-web` reaches them through `rbit-services`, and
`rbit-server` sits above both, so a server target would need every administration call
inverted through injected ports for no gain. None of the rules touch credentials or
sessions, so `rbit-identity` is not a better owner. The account-creation path stays where
it already is: `rbit_identity::accounts::AccountRegistration`, which `identity.bootstrap`
retired.

The restart-required configuration editor with sealed secrets, the audit log, the
monitoring dashboard and graceful restart are genuinely server-owned. They move to a new
`operations.configuration` row, targeting `rbit-server` and `baseline-retained`.

| Row | Behaviour | Target | Status |
| --- | --- | --- | --- |
| `operations.admin` | Rights, suspension, deletion, listings, settings change descriptions | `rbit-access` | ported |
| `operations.configuration` | Configuration editor, audit log, monitoring, graceful restart | `rbit-server` | baseline-retained |

## What moved

`rbit_access::administration`:

- `Administration` over the `AdministrationRepository` port: site administrators only;
  no demoting, suspending or deleting oneself; the last-administrator refusal message;
  the exact username confirmation; the sole-owner organisation refusal. Deleting a user
  returns the repositories the caller must remove from disk and announce. Repository
  deletion (the `owner/name` confirmation and metadata-first order) stays with
  `rbit_repositories::settings::AdminRepositoryDeletion`; `AdminService` reaches it after
  `Administration::repository` has checked the administrator and loaded the repository.
- `ListQuery`, `Paged`, `Listing`, `ListFilter`, `PAGE_SIZE` and `MAX_SEARCH_LEN`: search
  trimming and capping, 1-based pages and page arithmetic. `rbit_services::admin` and
  `rbit_storage::sqlite::admin` re-export them at their old paths.
- `Moderation`, the guarded statement outcome, re-exported by storage.
- `describe_changes(branding_changed, fields)`, the audit description. The service keeps
  only the projection of `InstanceSettings` onto named fields.

`rbit_storage::sqlite::admin::AdminStore` implements the port
(`rbit-storage/src/access_administration.rs`). Its SQL is unchanged and keeps the "never
remove the last active administrator" rule in each moderation statement. The search
`LIKE` escaping became a private function there because `ListFilter` is now a foreign
type.

`AdminService` keeps its public API and becomes the adapter: disk removal, the
`RepositoryDeleted` events, tracing and audit entries, the dashboard and the
configuration editor.

## Legacy code removed

From `rbit-services/src/admin.rs`: `refuse_self`, `check_moderation`, the confirmation
checks, the sole-owner refusal, the listing types and constants, the change-description
logic and their two unit tests (moved unchanged to `rbit-access`). From
`rbit-storage/src/sqlite/admin.rs`: the `Listing`, `ListFilter` and `Moderation`
definitions.

## Boundaries that fail if legacy paths return

`scripts/rewrite/administration-retirement.mjs`, on its own line in `check.mjs`:

- the two legacy administration files may not define the listing or moderation types or
  constants again (re-exports are allowed);
- the service adapter may not define the guards (`refuse_self`, `check_moderation`), match
  `Moderation` outcomes, check confirmations, compare changed settings or decide
  site-administrator authority itself.

Against the pre-slice sources it reports three violations; on this tree, none.
`scripts/tests/administration-retirement.test.mjs` covers each rule and the whole tree.

## Results

One Linux run of the repository's own jobs in quick mode at `e3b878a1` (the code is
identical at this record's commit): whole-workspace format and all-target Clippy with
`-D warnings`, licenses and advisories, the front-end build, the rewrite contracts
(including `check-types.mjs`) and nextest for the changed packages (`rbit-access`,
`rbit-storage`, `rbit-services`). The workspace-wide test run is left to the integrated
batch.

| Job | Result |
| --- | --- |
| Format and all-target Clippy (whole workspace) | pass |
| Licenses and advisories, front-end build | pass |
| Rewrite contracts: `check.mjs`, script tests, `check-types.mjs` strict, component and password-boundary modes | pass |
| Nextest, changed packages | 1,270 run, 1,270 passed: `rbit-access` 39, `rbit-storage` 453, `rbit-services` unit 262 and integration 516 |

Administration-relevant groups, all passing: the six new `rbit-access` administration
tests (authority, self and last-administrator guards, confirmations and sole owners,
listing normalisation, paging, change descriptions); services `admin` 15 (moderation,
deletion with disk removal and events, listings, settings audit, configuration editor),
`audit` 4 (paged audit listing through the moved types), `access` 7 and `orgs` 11; the
storage `LIKE` escaping test. `rbit-web` (administration routes) and `rbit-server`
(backup tests building `ListFilter`) were outside the changed-package set; both compile
in the workspace-wide Clippy run.

## Parity argument

- Routes, views and messages are unchanged; `rbit-web` compiles against the same service
  API and re-exported types.
- Every refusal message moved verbatim, and the check order is the same: the
  administrator check and load, then the confirmation, then the self guard, then the
  sole-owner check, then the guarded write. Repositories are listed before the user
  deletion, as before.
- Disk removal, events and audit entries happen after the same successful writes, in the
  same order and with the same text.

## Migration, rollback and performance

No migration, schema or wire format changed. Rollback is the previous binary against the
same data. Request paths run the same queries. No release measurement was taken.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 2 (rules and adapters, boundary) plus this record |
| Files changed by code commits | 13 |
| Lines added / removed (all) | 686 / 207 |
| Production Rust (`src`, non-test) | 9 files, +445 / −207 |
| Tests | 179-line `rbit-access` administration test module (two moved, four new); 35-line Node test |
| Checker and policy scripts | one new 25-line Node guard on its own `check.mjs` line |

Tricky parts:

- `ListFilter` carried a private `LIKE` escaping method used by storage; once the type
  moved, the method became a storage-private function so the SQL stays in storage.
- `require_site_admin` is generic over the error type, so call sites name it to keep `?`
  conversions unambiguous.
- The row needed re-targeting before porting. Splitting off `operations.configuration`
  keeps the server-owned editor and restart honest instead of claiming them as ported.

Estimate: one focused session for the decisions. The `operations.configuration` row is
larger: inverting the editor, monitoring and restart into `rbit-server` behind ports the
server injects into the web state touches the web administration routes, roughly the
two sessions the updates assessment estimated.
