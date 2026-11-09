# Notification mail and webhook policy retirement acceptance

Acceptance evidence for the `notifications.mail` ledger row ("Threaded notifications,
unsubscribe and daily/weekly digests"), retired by this slice, and for the first part
of `integrations.webhooks`, which stays `baseline-retained`. It follows the
[identity retirement](identity-retirement.md) and
[inbox retirement](notifications-inbox-acceptance.md) pattern.

## Scope

| Decision | Before | After |
| --- | --- | --- |
| Who is mailed: dedup of recipients, actor exclusion, preference, per-batch repository and access re-check, render-before-claim, one claim for immediate and digest delivery, digest queueing | `NotificationEmailService::deliver` | `rbit_notifications::mail::delivery::deliver` over `MailOutbox` |
| What each email says: headlines, dedup keys, anchors, excerpts, opening vs reply | `NotificationEmailService::activity` | `mail::activity::describe` over a feature-owned `MailEvent` |
| Threading and links: `Message-ID`, `In-Reply-To`, thread paths, digest ids, the bare host | private helpers | `mail::policy` |
| One-click unsubscribe tokens: `{id}.{HMAC-SHA256}` sign and constant-time verify | private helpers | `mail::unsubscribe` |
| Digests: periods, the per-period claim loop, re-checks, grouping, outcome counting, pending retention | `notification_email/digests.rs` | `mail::digest::{send_digests, period_start, prune_cutoff, group}` over `DigestOutbox` |
| Preference rules (`wants`, digest hour range, reason clauses) | private helpers | `mail::policy` |
| Digest records (`NewDigestItem`, `Claim`, `ClaimedDigest`, `DigestItem`, `DigestState`) | `rbit-storage` | `mail::records`, re-exported unchanged by storage |
| Webhook signature, request headers, `User-Agent`, delivery id shape, response limits | `webhooks/delivery.rs` | `rbit_notifications::webhooks::protocol` |
| Webhook retry schedule and the per-attempt decision, hook and secret limits, delivery retention | `webhooks.rs`, `webhooks/delivery.rs` | `rbit_notifications::webhooks::policy` |

What stays in `rbit-services` is composition. `notification_email/outbox.rs`
answers the mail ports: concrete stores, the shared readability predicate
(`can_read_with_grants`), Askama mail templates (`rbit-mail`), the throttle and the mail
queue for immediate email, and the direct transport plus the
`rbit_notification_digests_total` counter for digests. `NotificationEmailService` keeps
settings, the unsubscribe key (`server_secrets`, created once) and the mapping of
domain events, comments and reviews to `MailEvent`. The configured `EmailPreference` and
`EmailDelivery` map one to one onto the feature's `MailPreference` and `Cadence`; a
unit test pins the mapping and the digest hour limit.

### Webhooks: what remains

`integrations.webhooks` is too large for one slice. Hook management, `DomainEvent`
payloads (`webhooks/payloads.rs` and the issue, pull, status and Actions payload
builders), queueing and hook selection, the delivery worker, and the HTTP client with
its outbound address guard (`GuardedResolver`, `target_url`, `may_contact`) remain in
`rbit-services`. The address guard is shared with external sign-in and imports, and
it is unchanged. The next slices are payload construction, then queueing and hook
selection behind ports, then the worker. The row records these notes.

## Legacy code removed

1. Mail decisions in `notification_email.rs` (the module is +333/−659, the new 275-line
   outbox adapter included): the
   delivery loop, activity wording, threading, excerpt, link-signing and preference
   helpers, and their unit tests, which moved unchanged with the code.
2. The digest run, periods, re-checks and grouping in `notification_email/digests.rs`.
3. Digest record definitions in `rbit-storage/src/sqlite/email_digests.rs`.
4. Webhook signing, header, guid, retry, limit and retention code in `webhooks.rs` and
   `webhooks/delivery.rs`; public paths (`MAX_HOOKS_PER_OWNER`, `signature`,
   `retry_delay`, `request_headers`, `USER_AGENT`, `MAX_RESPONSE_BODY`) are
   re-exported from the feature crate, so callers are unchanged.

## Boundary that fails if legacy paths return

`scripts/rewrite/notifications-mail-retirement.mjs`, run by `scripts/rewrite/check.mjs`,
scans production sources outside `rbit-storage` and `rbit-notifications`, ignoring
comments and `#[cfg(test)]` modules:

- the digest store is reached only from `notification_email/outbox.rs`;
- the notification email store only from the outbox and the settings code, and email
  claims and recipient reads only from the outbox;
- the services mail and webhook modules must not define the moved policy functions
  (link signing, preference, wording, excerpts, threading, digest cutoffs, webhook
  signature, headers, retry, secret check, next step), use `Hmac<`, or redefine the
  retry schedule, link purpose, digest constants or webhook limits.

The pre-slice `notification_email.rs`, `notification_email/digests.rs`, `webhooks.rs`
and `webhooks/delivery.rs` each fail it. `scripts/tests/notifications-mail-retirement.test.mjs`
covers the rules, exemptions and the whole tree.

## Parity argument

- Routes, templates, catalog keys, error text and headers are untouched: no `rbit-web`
  or `rbit-mail` file changed. Public service methods keep their signatures;
  `rbit_services::notification_email::{period_start, DigestRun, SETTINGS_PATH,
  UNSUBSCRIBE_PATH}` keep their paths.
- **Digest claims:** the same store calls in the same order: candidates in pages of
  200, one claim per user and period, at most one digest per period, `finish` with a
  fresh clock reading, failures not retried, and the counter only for sent and failed.
- **Unsubscribe tokens:** the same purpose string, separator, account encoding,
  lower-case hex and constant-time comparison; a fixed vector computed independently
  pins the format, and the key row name is unchanged.
- **SMTP behaviour:** immediate email still renders the whole batch before claiming,
  claims once for immediate and digest delivery, checks the throttle per address and
  uses the fire-and-forget queue; digests use the direct transport. Message ids,
  `In-Reply-To` and `List-Unsubscribe` headers are built from the same strings.
- **Webhook signing, limits and guards:** the same HMAC-SHA256 hex signature (GitHub's
  published vector still passes), the same header order and names, the same
  `User-Agent`, retry schedule, jitter, attempt limit, response limits and retention
  arithmetic. The outbound address guard and HTTP client did not change.
- No schema, migration or stored format changed; 0073 remains the latest migration.

## Results

Two Linux runs of the repository's own CI jobs.

- **Mail commits, before the webhook part and two lint fixes** (`d9e57a40`): nextest for
  the whole workspace ran 3,288 tests, all passed, 2 skipped (the skips predate this
  change), including `rbit-web` notification settings and the `rbit-e2e` notification
  email and digest flows. Clippy found two pedantic lints in the new feature code (a
  `contains_key` followed by `insert`, and a boolean that could be simplified), fixed
  before the tip.
- **Slice tip** (`cf520061`, changed packages only): workspace-wide formatting and
  all-target Clippy with `-D warnings`, licenses and advisories, and the front-end build
  passed; nextest for `rbit-notifications`, `rbit-storage` and `rbit-services` ran 1,264
  tests, all passed. `node scripts/rewrite/check.mjs` and the Node policy tests passed.
  The structural type gate failed only on
  `crates/rbit-storage/src/identity_access_tokens.rs` ("cannot resolve self type
  AccessTokenStore"), which comes from the base stack and is fixed separately
  (`4f0b4602`); this slice does not touch that file.

Relevant groups at the tip, all passing:

| Group | Tests |
| --- | --- |
| `rbit-notifications` unit: mail policy, digest periods, unsubscribe tokens (fixed vector included) | 10 |
| `rbit-notifications` unit: webhook protocol and policy (GitHub's signature vector included) | 6 |
| `rbit-notifications` integration `mail` (fake ports, new) | 4 |
| `rbit-services` integration: `notification_email` 11, `email_digests` 8, `notifications` 10, `webhooks` 12 | 41 |
| `rbit-services` unit: `webhooks` 13 (address guard included), `outbound` 26, `notification_email` mapping 1 | 40 |
| Permission group: services `permission_matrix` | 6 |
| `rbit-storage` integration: `email_digests` 5, `notification_email` 4, `notifications` 4, `webhooks` 8 | 21 |

Not run at the tip: Windows legs, and the `rbit-web` and `rbit-e2e` suites, which the
mail commits did not change and which passed at `d9e57a40`; the webhook commit changes
no file those crates compile against except re-exported constants and functions.

## Migration, rollback and performance

No migration, schema or stored format changed. Rollback is the previous binary
against the same data. The mail and digest paths issue the same statements in the same
order; the only added work is converting digest sections to template records and the
configured preference to the feature enum, both proportional to what is mailed. No
request path gained a call. No release measurement was taken, and none is claimed.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 4 (plus this record and the ledger rows) |
| Files changed by code commits | 29 |
| Lines added / removed (all code commits) | 1,945 / 887 |
| Rust sources (`src`, unit tests that moved with their code included) | 18 files, +1,420 / −884 |
| New fake-port tests | `rbit-notifications/tests/mail.rs` (4 cases), mail policy, digest and webhook policy unit tests |
| Checker and policy scripts | one new 41-line Node guard with a 4-case test; two lines in `check.mjs`; two helpers exported from the inbox guard |

Tricky parts:

- `rbit-notifications` may depend only on contracts and collaboration, so the configured
  `EmailPreference`/`EmailDelivery`, review states and templates cannot cross into it.
  The feature owns small mirror enums (`MailPreference`, `Cadence`, `ReviewVerdict`)
  and section records, and composition maps them; a unit test pins the mapping.
- Delivery order is observable (render the whole batch before claiming, one claim for
  immediate and digest delivery, finish with a fresh clock reading), so the ports were
  shaped around the legacy call order rather than around storage tables. The fake-port
  tests assert that order.
- Webhooks are three to four slices of work. Only the transport-independent policy
  moved; the address guard, which also protects external sign-in and imports, was left
  untouched on purpose.

Estimate for similar rows: a row with no prepared application but mostly pure policy,
like this mail row, is about one to two focused days: ports shaped around the legacy
call order, a fake-port test file, an adapter, and a guard. The rest of
`integrations.webhooks` (payload builders, queueing and hook selection, the worker) is
about three more slices of that size.
