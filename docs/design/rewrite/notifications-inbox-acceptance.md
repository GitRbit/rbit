# Notifications inbox retirement acceptance

Acceptance evidence for the `notifications.inbox` ledger row ("Subscriptions, unread
state and access-safe inbox"). It follows the [identity retirement](identity-retirement.md)
pattern: decisions move into the target crate behind ports, production selects them,
the legacy implementation is deleted and a boundary check keeps it from returning.
`notifications.mail` and `integrations.webhooks` stay `baseline-retained`.

## Scope

The row's source, `crates/rbit-services/src/notifications.rs`, held four kinds of code.
Earlier strangler slices had already moved the read side (inbox page, visibility
cleanup, single-row reads, read marks, unsubscribe, badge and thread state) into
`rbit_notifications::{application, signals}` over `NotificationInboxStore`, without
recorded acceptance. This slice finishes the row:

| Decision | Before | After |
| --- | --- | --- |
| Inbox page, find, read marks, unsubscribe, badge, thread state | `rbit_notifications::{application, signals}` (prepared) | unchanged, now accepted |
| Manual subscription read and change | direct `NotificationStore` calls in services | `rbit_notifications::subscriptions` over `InboxRepository` |
| Fan-out: participation, recipients, mentions, actor exclusion, per-batch authorization | `NotificationService::{handle_event, notify_recipients, new_mentions}` | `rbit_notifications::fanout::FanoutApplication` over `DeliveryRepository`, `ActivitySource`, `RecipientAccess` |
| Retention cutoff and read-row pruning | inline arithmetic and a direct store call | `rbit_notifications::retention::{cutoff, prune_read}` |
| Fan-out batch bound (100) | constant in the storage module | `rbit_notifications::NOTIFICATION_BATCH_SIZE`, re-exported unchanged by storage |

What stays in `rbit-services` is composition: `fanout::activity` maps `DomainEvent`s to
the feature-owned `ActivityEvent` (dropping events that never notify),
`fanout::DatabaseView` answers the content and access ports from the concrete stores
(the shared Markdown scanner finds mentions; `rbit_auth::repo_access` decides
readability, as before), errors map to `ServiceError`, and the service owns the
background lifecycle: the event subscriber, the daily prune schedule and shutdown.
The digest schedule and email delivery belong to `notifications.mail`, untouched.

The durable notification outbox described in the rewrite plan is new design, not
baseline behaviour; the broadcast subscriber is retained exactly as it was.

## Legacy code removed

1. The fan-out decision code in `NotificationService`: `handle_event`'s recipient
   logic, `notify_recipients` and `new_mentions` (163 lines deleted, 16 added).
2. Direct `NotificationStore` calls from services for `subscription`, `set_subscribed`,
   `participate`, `subscribers`, `known_participants`, `notify_many` and `prune_read`.
3. The services re-export of notification models through the storage path
   (`rbit_storage::sqlite::notifications`); services re-export them from
   `rbit_notifications`, their defining crate. The types are identical.

## Boundary that fails if legacy paths return

`scripts/rewrite/notifications-retirement.mjs`, run by `scripts/rewrite/check.mjs`,
scans production sources (`crates/*/src`) outside `rbit-storage` and
`rbit-notifications`, ignoring comments and `#[cfg(test)]` modules:

- every `.notifications()` store handle must be passed straight to
  `NotificationInboxStore::new(...)`, the port adapter;
- no `NotificationStore::` path;
- no direct `participate`, `notify_many`, `known_participants` or `prune_read` call;
- services must not import notification models from the storage path.

`scripts/tests/notifications-retirement.test.mjs` covers each rule, the exemptions and
the whole tree. The pre-slice `notifications.rs` and `notifications/inbox.rs` fail the
store-handle, delivery-call and model-path rules.

## Parity argument

- Routes, templates, status codes, error text and catalog keys are untouched: no
  `rbit-web` file changed. `NotificationService`'s public methods and signatures are
  unchanged, so every caller compiles as before.
- SQL, transactions and stored data are unchanged: the adapter forwards to the same
  `NotificationStore` functions. No schema, migration or redb change; 0073 remains
  the latest migration.
- The fan-out is a line-for-line move: the same storage calls in the same order
  (participation before subscriber reads, actor participation after merging, early
  exit when nobody is named), the same strongest-reason merge, the same actor and
  organization exclusions, and authorization refreshed for every batch of 100 with
  delivery stopping once the repository is gone.
- Who can see which notification is unchanged: per-batch readers still come from
  active users, current grants and `repo_access`; inbox reads still delete rows of
  repositories the viewer can no longer read.
- Disabled notifications still skip all fan-out work and badge queries; manual
  subscriptions remain available.

## Results

One Linux run of the repository's own CI jobs at the slice tip, rebased on the
integrated stack `4868231e`: formatting and all-target Clippy with `-D warnings`,
licenses and advisories, the front-end build and nextest passed. Nextest ran 3,281
tests: 3,281 passed, 2 skipped (the skips predate this change), none failed. The
rewrite-contracts job passed `node scripts/rewrite/check.mjs` (zero violations) and the
Node policy tests, then failed in the structural type gate on
`crates/rbit-storage/src/identity_access_tokens.rs:13` ("cannot resolve self type
AccessTokenStore"), a file this slice does not touch; it belongs to the
`identity.tokens` slice in the base stack and must be fixed there. Windows legs were
not run for this slice.

Notification-relevant groups within that run, all passing:

| Group | Tests |
| --- | --- |
| `rbit-notifications` unit (application `Debug`) | 2 |
| `rbit-notifications` integration: `fanout` 8 (new), `inbox` 6, `models` 3 | 17 |
| `rbit-services` integration: `notifications` (inbox included) 10, `notification_email` 11, `email_digests` 8 | 29 |
| `rbit-services` unit: `notification_email` | 8 |
| Permission group: services `permission_matrix` 6, web 12, SSH 1 | 19 |
| `rbit-storage` integration: `notifications` 4, `email_digests` 5, `notification_email` 4 | 13 |
| `rbit-web` integration `notification_settings` | 4 |
| `rbit-e2e`: `issues`, `notification_email`, `email_digests` | 3 |

New tests added by this slice, all in the passing set:

- `rbit-notifications` `fanout`: opening (author participation, assignees, new,
  known and organization mentions, call order), missing threads and unmentioning
  edits, actor exclusion and self-assignment, strongest-reason merge with readability
  filtering and automated comments, review reasons, per-batch reauthorization over
  207 subscribers and stopping when the repository vanishes, retention saturation,
  and manual subscription forwarding.
- `scripts/tests/notifications-retirement.test.mjs` (four cases, including the whole
  tree).

Before the change of build process, locally on Windows: `cargo check` of the three
affected crates (all targets) and the `rbit-notifications` tests passed. Windows
all-target Clippy of `rbit-services` reports 16 `large_futures` errors in
`tests/pulls.rs`, which exist without this slice and do not occur on Linux.

## Migration, rollback and performance

No migration, schema or stored format changed. Rollback is the previous binary
against the same data. The fan-out issues the same statements; mention lookup now
moves the thread body out of the row it already loaded instead of borrowing it, and
no request path gained a call. No release measurement was taken for this slice, and
none is claimed.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (plus this record and the ledger row) |
| Files changed by code commits | 16 |
| Lines added / removed (all code commits) | 795 / 176 |
| Production Rust (`src`, non-test) | 12 files, +463 / −176 |
| Tests | 1 new file, +227 |
| Checker and policy scripts | one new 54-line Node guard with a 48-line test; two lines in `check.mjs` |

Tricky parts:

- The read side was already ported, so the work was the fan-out: keeping every
  storage call in the same order (participation before subscriber reads, the actor's
  participation after merging, per-batch repository lookups) so the existing
  real-store suites keep passing unchanged.
- `rbit-notifications` may depend only on contracts and collaboration. Event mapping,
  Markdown mention scanning and the access predicate therefore stay in services
  adapters, and the ports carry only their results (accounts with an organization
  flag, the readers of a batch).
- The boundary check had to tolerate an inline `#[cfg(test)]` fixture in `rbit-web`
  that writes through the store directly, so it strips unit-test modules by brace
  matching rather than exempting whole files.
- The shared Windows build lane was mostly held by parallel slices; compile feedback
  came in short windows, and Linux CI was the test gate.

Estimate for similar rows: a row whose read side has already been strangled into its
target crate, like this one, is about one focused day: one to two hundred lines of
moved policy, ports and adapters, a fake-port test file and a small guard. A row
with no prepared application (for example `notifications.mail`, with its digest
claims, unsubscribe tokens and SMTP lifecycle) is several times that.
