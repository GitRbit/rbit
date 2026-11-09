# Webhooks retirement acceptance

Acceptance evidence for the `integrations.webhooks` ledger row ("Signed payloads,
persistent retry, delivery UI and SSRF protection"). The first part, the signed wire
contract, limits and retry decision, moved with the
[notification mail slice](notifications-mail-acceptance.md). This branch finishes the
row in the three slices proposed there: payloads, then queueing and hook selection,
then the worker. It follows the [identity retirement](identity-retirement.md) pattern.

## Scope

| Decision | Before | After |
| --- | --- | --- |
| Signature, request headers, `User-Agent`, delivery id shape, response limits | `webhooks/delivery.rs` | `rbit_notifications::webhooks::protocol` (mail slice) |
| Retry schedule and per-attempt decision, hook and secret limits, retention | `webhooks.rs`, `webhooks/delivery.rs` | `webhooks::policy` (mail slice) |
| Event envelopes: `repository` (created, deleted, transferred, archived), `fork`, `release`, `member`, `ping` and its zen, which events record an `action` | `webhooks/payloads.rs` | `webhooks::payloads` |
| The `push` contract: null ids, compare links, oldest-first commits capped at 20, head commit, pusher | `PushPayload::to_json` | `webhooks::payloads::PushEnvelope` |
| Hook selection: whose hooks an event reaches (repository, organisation, both owners on transfer, the organisation only once a repository is deleted) and only hooks that existed when the event was published | `WebhookService::{queue_event, subscribed}` | `webhooks::queue::{HookScope, select}` over `HookQueue` |
| Queueing: one delivery and guid per hook, the recorded action, waking the worker; redelivery | `WebhookService::{queue, redeliver}` | `webhooks::queue::{queue, redeliver}` |
| Worker bookkeeping: bounded batches until nothing is due, settling each attempt (observe, then record the next step) | `WebhookService::{deliver_due, run}` | `webhooks::worker::{deliver_due, settle}` over `DeliveryQueue` |
| Event list validation (trimmed, `ping` dropped, sorted, de-duplicated, Actions gate, wording), the hook limit message, delivery log paging | `webhooks.rs` | `webhooks::policy::{subscribed_events, hook_limit_error, delivery_page}` |

## What stays in `rbit-services`, and why

- **The outbound address guard is unchanged and stays where it is.** `GuardedResolver`,
  `target_url` and `may_contact` in `webhooks/delivery.rs`, with the HTTP client built
  around them, also protect external sign-in and repository imports. Moving them would
  change a shared security boundary for three features at once; they are transport,
  not notification policy. Every guard test passes unchanged.
- **Shared JSON object shapes.** `account`, `repository`, `release`, `push_commit`,
  `hook` and `timestamp` in `webhooks/payloads.rs` are the GitHub-shaped objects the
  REST API also returns (`rbit-web/src/routes/api/*`, issue, pull, status and Actions
  JSON). They belong to the API contract; the webhook envelopes take them pre-rendered.
  The module's public functions keep their signatures and delegate to the feature.
- **Composition.** `webhooks/ports.rs` implements `HookQueue` and `DeliveryQueue`
  over the hook store, the worker's wake-up, task spawning, the attempt counter and the
  debug log. `WebhookService` keeps management authorization (the caller's repository
  or organisation context), audit entries, mapping `DomainEvent`s to payloads (with the
  Git and release reads they need), and the runtime: the event subscriber, permits,
  shutdown, re-queueing interrupted deliveries and daily pruning.

## Legacy code removed

1. Envelope construction, the push contract, the zen list and the action table in
   `webhooks/payloads.rs` (the module is now delegating wrappers plus the shared shapes).
2. Hook selection, queueing, redelivery, the delivery batch loop, attempt settling,
   event validation, the hook limit and paging arithmetic in `webhooks.rs`.
3. Direct hook store calls for selection, enqueueing, claiming and recording outside
   the port adapter, including the worker's own claim.

## Boundary that fails if legacy paths return

`scripts/rewrite/notifications-mail-retirement.mjs` (run by `check.mjs`) gains two
rules for production sources: hook store `subscribed`, `enqueue`, `claim_due` and
`record_attempt` calls and `AttemptOutcome` appear only in `webhooks/ports.rs`; and the
services webhook modules must not define the zen list, `NULL_SHA`, `MAX_PUSH_COMMITS`
or the newest-hook filter. The pre-branch `webhooks.rs` and `webhooks/payloads.rs`
fail them; `scripts/tests/notifications-mail-retirement.test.mjs` covers both rules.

## Parity argument

- Payload bytes: `serde_json` objects are ordered maps without `preserve_order`, so
  building the same keys and values yields the same body. The existing payload unit
  tests (GitHub field shapes, push lists, null ids, member changes, releases) run
  against the delegating wrappers unchanged; a new test pins which events record an
  action for every event the catalog knows.
- Selection and queueing: the same store query, the same `<= newest` filter, owner
  order on transfer, a guid per delivery generated in hook order, no store call and no
  wake-up for an empty hook list.
- Worker: batches of `concurrency` (at least one), attempts run concurrently with the
  same join, the counter and debug line before the record, the same outcome mapping;
  the clock and randomness are read only for retries.
- Management: identical error text for event lists, secrets and the hook limit
  (the original "a organization" wording is kept as released).
- Signing, headers and the address guard are unchanged. No schema, migration or stored
  format changed.

## Results

One Linux run of the repository's own CI jobs at the slice's last code commit
(changed packages only, base `19fb0cf1`): the rewrite contracts (the policy checker,
the structural type gate and the Node policy tests), workspace-wide formatting and
all-target Clippy with `-D warnings`, licenses and advisories, and the front-end build
passed. Nextest for `rbit-notifications`, `rbit-storage` and `rbit-services` ran 1,274
tests, all passed. An earlier run at the pre-validation work-in-progress state was
also fully green.

Webhook-relevant groups, all passing:

| Group | Tests |
| --- | --- |
| `rbit-notifications` unit: webhook protocol, policy (event lists, limits, paging, retry, next step) and envelopes | 11 |
| `rbit-notifications` integration `webhooks` (fake ports, new): selection, queueing, worker batches, settling | 4 |
| `rbit-services` unit `webhooks`: payload shapes through the delegating wrappers, the action table (new), event and secret validation, the address guard | 14 |
| `rbit-services` unit `outbound` (shared client, guard, limits, proxies) | 26 |
| `rbit-services` integration `webhooks` | 12 |
| `rbit-storage` integration `webhooks` | 8 |

Not run for this branch: Windows legs, and the `rbit-web` and `rbit-e2e` suites (the
REST API and end-to-end webhook flows), which quick mode skips because no file in
those crates changed; the payload functions they call keep their signatures and are
covered by the unchanged services tests above.

## Migration, rollback and performance

No migration, schema or stored format changed; rollback is the previous binary. The
push envelope renders only the listed commits and the head, as before. Queueing and
delivery issue the same statements. No release measurement was taken, and none is
claimed.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (plus this record and the ledger row) |
| Files changed by code commits | 16 |
| Lines added / removed (all code commits) | 930 / 241 |
| Production Rust (`src`, non-test files) | 8 files, +672 / −240 |
| Tests | 3 new or extended Rust test files (+232), 14 lines of Node guard tests |

Tricky parts:

- The JSON object shapes are shared with the REST API, so only the envelopes moved; the
  services module keeps its public signatures and delegates, which kept every existing
  payload test unchanged.
- `rbit-notifications` cannot see `WebhookEvent`, `WebhookId` or the storage rows, so
  the ports carry them as associated types and the event list check takes a small
  catalog of closures; the action table became a list of wire names, pinned by a new
  test over every event.
- The worker's runtime (spawning, permits, shutdown, re-queueing) stays composition;
  only its batching and settling moved, so the feature crate still has no runtime
  dependency.

Estimate: once the wire contract had moved, finishing the row took about one focused
day. Rows whose objects are shared with the REST API should expect the same split:
envelopes and decisions move, shared shapes stay with the API contract.
