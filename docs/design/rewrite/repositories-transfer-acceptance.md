# Repository transfer retirement acceptance

Acceptance evidence for the `repositories.transfer` ledger row ("Transfers/renames and
old repository URL redirects"), following the [identity retirement](identity-retirement.md)
pattern. The design and preserved boundaries are in the
[transfer and old-address applications](../repository-transfer-applications.md).

## Scope

| Behaviour | Owner after this slice |
| --- | --- |
| Transfer authority, confirmation, destination, suspension, same-owner rejection, compensation order | `rbit_repositories::transfer::RepositoryTransfer::execute` |
| Suggested destination owners (admin gate, own account + organizations, current owner excluded) | `RepositoryTransfer::suggested_owners` (new) |
| Destination lookup, suspension, memberships, owner-change transaction, conflict mapping | `TransferRepository` port, `rbit_storage::TransferStore` (new) |
| Current-then-old address resolution | `rbit_repositories::aliases::RepositoryAliases::{resolve, open}` over `RepositoryAddresses`, implemented by `RepositoryStore` (new) |
| Browser redirect publication (permission filtered) | `RepositoryAliases::moved_to` |
| Repository rename | `rbit_repositories::settings` (already production; owned by `repositories.lifecycle`) |
| Alias recording and reclaim | Migration 0016 triggers (unchanged SQL) |

`rbit-services::TransferService` remains the composition adapter: request reader,
disk relocation, event publication, best-effort audit and error mapping. HTTP routes,
templates and catalog text in `rbit-web` are unchanged.

## Legacy code removed

1. Transfer policy in the service: `require_admin` and the suggestion filtering in
   `TransferService::suggested_owners`.
2. Storage access from the service bridge: direct `find_owner`, `is_suspended`,
   `memberships_of` and `RepositoryStore::transfer` calls and the
   `StorageError::Conflict` mapping; the bridge's `authorised_owner` wrapper and its
   error remapping (the application calls the shared `RepositoryOwnership` grant).
3. Three duplicated current-then-old lookups (`.git` strip, `find_by_path`, then
   `find_by_old_path`) in the LFS grant opener, the deploy-key opener and the Actions
   task-token opener. They call `RepositoryAliases::resolve` with the same store.
4. The alias port's own `find_current`/`find_old` storage methods on `AliasView`.

## Boundaries that fail if legacy paths return

`scripts/rewrite/repository-transfer-retirement.mjs`, run by `scripts/rewrite/check.mjs`,
over every authored Rust file below `crates/` (tests included), comments ignored:

- `find_by_old_path` outside `rbit-storage`: old addresses must resolve through
  `RepositoryAliases`.
- `.repositories().transfer(` or `RepositoryStore::transfer` outside `rbit-storage`.
- `require_admin`, `authorised_owner`, `memberships_of`, `is_suspended`, `find_owner`
  or `StorageError` in `rbit-services/src/transfers*`.
- Transfer policy text in production code outside `rbit-repositories/src`.

`scripts/tests/repository-transfer-retirement.test.mjs` covers each rule, the allowed
storage/test/comment cases and the whole current tree. `TransferStore` is registered
in `component-types.json` for the cumulative component-size gate.

## Results

Local runs on Windows through the shared worktree helper, at the third code commit
(the Windows full-suite legs remain deferred to the release build):

| Gate | Result |
| --- | --- |
| `check` for `rbit-repositories`, `rbit-storage`, `rbit-services`, `rbit-web`, all targets | pass |
| Clippy `-D warnings`, all targets: `rbit-repositories`, `rbit-storage`, `rbit-web`, `rbit-ssh`; `rbit-services` library | pass |
| Clippy `-D warnings`, `rbit-services` all targets | pass except 16 `large_futures` errors in `tests/pulls.rs` (`Fixture::new`, 16,424 bytes), code this slice does not touch; clean with that lint allowed |
| `rbit-repositories` package tests (transfer 7, aliases 5, others 35) | 47 passed |
| `rbit-services` integration: `transfers` 8, `repo_settings` 21, `lfs` 8, `access` 7, `permission_matrix` 6, `actions_tokens` 1, `wiki_pages` 4 (filter overlap) | 55 passed |
| `rbit-storage` integration `redirects` | 11 passed |
| `rbit-ssh` (unit 13, integration 15, deploy-key old names included) | 28 passed |
| `node scripts/rewrite/check.mjs` | 0 violations |
| `node scripts/rewrite/check-types.mjs` (strict crates, components, password boundary) | 0 violations; `TransferStore` 36 lines, `TransferBridge` 46 |
| `node --test scripts/tests/*.test.mjs` | 642 of 651 passed, 8 skipped; the one failure (`htmx-vendor`) needs `node_modules` and is unrelated |
| `cargo fmt --all --check` | pass |

The existing service suite `crates/rbit-services/tests/transfers.rs` passed unchanged,
including the zero-work refusal and single-query alias cases. The prepared port
fixtures in `rbit-repositories/tests/{transfer,aliases}.rs` were updated for the
split store/effect ports: the destination lookup now records the catalog's
`owner_named`/`membership` calls instead of one opaque `authorize` call. New cases:
own-account resolution without lookup, site-administrator suspension of a personal
destination, suggestion admin gate/order/exclusion, and `resolve` precedence.

Not run: `rbit-e2e`, `rbit-web` integration and the full workspace suites. The
intermediate first code commit was not compiled on its own.

## Parity argument

- Same order and count of store work: refused commands open no store and run no SQL
  (`validation_and_missing_aliases_preserve_zero_and_single_query_paths` is unchanged);
  site-administrator and non-administrator destination lookups each open one
  repositories store (`TransferRepository::owners`); suspension opens one accounts
  store only for personal destinations; suggestions run one membership query and
  project it lazily. Pool labels (`repositories`, `accounts`, `orgs`) are unchanged.
- The non-administrator path previously called `authorised_owner`, which is
  `RepositoryOwnership::resolve` plus an error mapping; the application now calls
  `resolve` directly and maps `Invalid` to the same "you cannot transfer repositories
  to …" text. No `StorageError` maps to `ServiceError::Invalid`, so storage failures
  surface identically.
- The owner-change transaction, conflict detection, disk relocation and compensation
  are the same functions in the same order; the primary error still wins.
- The three replaced lookups strip `.git`, query the current address and only then
  the old address on the same `RepositoryStore`; access decisions after the lookup are
  untouched.

## Migration, rollback and performance

No migration, schema, SQL or stored data changed; migration 0073 remains the latest
and no historical migration was edited. Rollback is the previous binary against the
same data. No request path gains a query or a store handle beyond a borrowed wrapper;
no release measurement was taken and none is claimed.

## Not covered

- Disk/metadata atomicity gaps documented in the applications design remain: a failed
  compensating move or cancellation between disk move and SQL can need reconciliation.
- Windows test legs and the full workspace suite were not run for this slice; the
  release build's full run is the remaining gate.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (plus this record) |
| Files changed by code commits | 18 |
| Lines added / removed | 456 / 173 |
| Production Rust (`src`, non-test) | 12 files, +242 / −138 (new: two storage adapters, 71 lines; transfer port, 57) |
| Tests and fixtures | 2 files, +114 / −35 |
| Checker and policy scripts | one 42-line Node guard, a 55-line test, two checker lines, one component entry |

Tricky parts:

- Most of the decision code had already been moved by the prepared transfer and alias
  applications; retirement was finding what still bypassed them: the suggestion
  filter, the storage calls and conflict mapping in the bridge, and three copies of the
  old-address fallback in LFS, SSH and Actions.
- Keeping exact store-handle behaviour while introducing a storage port: the port
  borrows the database and returns a fresh owner catalog per destination lookup,
  rather than holding a store, so refusals still allocate nothing.
- Routing the destination through `RepositoryOwnership` directly changed the shape of
  the prepared fixture (catalog calls instead of an opaque authorize step); the error
  mapping was checked against `ServiceError`'s `From<StorageError>`.
- The shared build lane was contended by four other worktrees: each helper command
  waited several lease timeouts. Wall time was dominated by queueing, not compilation.

Estimate for similar rows: where a prepared application already exists in the target
crate and production already delegates to it (as here, forks, settings), retiring a
row is one focused session: about 3 code commits, 15–20 files and 400–600 changed
lines. Rows without a prepared application (imports, mirrors, LFS, releases) need the
application, ports and adapters first and are several times larger.
