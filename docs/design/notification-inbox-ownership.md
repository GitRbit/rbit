# Notification inbox ownership

`rbit-notifications` owns Notification, NotificationRecipient, NotificationReason,
Subscription, InboxPage and default page size. Public storage/service paths reexport
the exact types. IDs, enum ordering, machine strings, English `label()` compatibility,
field visibility, derives and page-count behavior remain unchanged. SQLx row types,
conversion validation and every existing SQL body stay in storage. No serialization
format, schema, API response or template changes are made.

The application borrows feature-owned static persistence and access ports. Its inbox
order remains list, distinct sorted repository IDs, one batch ACL HashMap, hidden-row
deletion, count. It moves the same rows into the same Vec allocation pattern; it does
not add per-row queries, clones, boxes or a second visibility collection. Empty batch
ACL still short-circuits in the existing storage implementation. Pagination clamps and
offset arithmetic match the original; hidden rows are not refilled from later pages.

Find retains user-scoped lookup, repository lookup, normal grants, hidden deletion,
then NotFound. A missing repository returns NotFound without deletion; deletion errors
remain the returned error. Mark-read remains user-scoped without inventing an additional
repository permission query. Unsubscribe checks visibility before sampling its clock,
then changes subscription before marking read. Storage remains concurrency authority;
these existing sequences do not become a cross-statement transaction.

`NotificationInboxStore` moves the original concrete store handle and forwards exact
SQL futures. `InboxView` borrows only Database and the accepted User; it performs the
existing repository/grant calls. No ServiceContext enters the feature. The viewer
binding is a trusted composition contract, not a portable authorization capability.
Disabled badge and thread-read entry points return before even constructing the store.
Thread state retains one query when disabled and masks only its unread result.
Best-effort thread-read errors remain logged/false at the service boundary.

Fanout, mention parsing, recipients, email, digests, pruning, cancellation and shutdown
remain on NotificationService and existing mail owners. Their source tail is unchanged
from the prepared base. This slice does not complete the notification ledger or add a
new lifecycle owner. Runtime latency and allocation counts have not been measured.

## Prepared acceptance and integration

Root registers `crates/rbit-notifications` as a workspace member and workspace dependency
`rbit-notifications = { path = "crates/rbit-notifications" }`, then resolves only the
new path package and storage/service edges. No registry version/dependency is added:
normal dependencies are contracts and collaboration with defaults disabled; Tokio is
existing dev-only test infrastructure. Root Cargo.toml/lock are intentionally untouched.

Prepared tests cover independent machine/English labels and rank compatibility,
pagination extremes, distinct batch visibility and no refill, exact failure stopping
and error source, missing/hidden privacy behavior, unsubscribe clock/write order,
disabled lazy construction and one-query thread state. Real SQLite service fixtures
assert 2 empty-inbox, 3 visible-inbox and 4 hidden-cleanup statements/acquisitions,
disabled badge/read zero work and disabled thread state exactly one query. Existing
fanout and HTTP/API tests remain required. These Rust tests are prepared but unrun.

Run under the exclusive shared helper lane after registration:

- `node scripts/workspace.mjs test --package rbit-notifications --locked`
- `node scripts/workspace.mjs test --package rbit-services --test integration --locked -- notifications::`
- Existing storage notification and web notification suites, then strict affected/workspace Clippy.
- `cargo tree --package rbit-notifications --edges normal --prefix none --locked`:
  verify no core/storage/services/runtime/crypto dependencies. Workspace dev unification
  is not proof of the isolated normal graph.
- The exact AST checker on the new crate plus registered InboxView/NotificationInboxStore
  components, physical bounds and feature/dependency policy. Source-only checks do not
  substitute for compilation and native tests.
