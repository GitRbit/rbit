# Disabled Actions acceptance

Acceptance evidence for the `actions.disabled` ledger row ("Disabled Actions has no
routes, workers or writes"), owned by `rbit-actions`. It follows the
[identity retirement](identity-retirement.md) pattern.

## What "retired" means for this row

The behaviour is a composition rule: when `[actions] enabled` is off, the Actions service
is never built, so nothing that needs it exists (runner protocol, pages, file routes, API
endpoints, background tasks, sweeps), rather than each part answering "disabled". The
legacy source, `rbit-web/src/routes/mod.rs`, already merges the Actions routers only when
the service exists. What was spread out was the decision itself: `[actions] enabled` was
read directly by the service constructor, by both webhook create and update paths
(refusing Actions events, with the refusal text inline), and twice by the API layer (the
route table and the OpenAPI document).

It now lives in `rbit_actions::availability::Availability`:

| Decision | Legacy place | Now |
| --- | --- | --- |
| The Actions service exists only while on | `ActionsService::new` (`if !enabled { return None }`) | `Availability::provide` |
| Actions webhook events are not accepted while off, and the refusal text | `webhooks::parse_events(.., bool)` | `Availability::{offers_event, refused_event}` |
| The API route table and OpenAPI document served | `routes/api/mod.rs` (`if config.actions.enabled`) | `Availability::{is_enabled, choose}` |

Everything else follows the presence of the service: `routes/mod.rs` merges the runner
protocol, file and page routers only with it; every Actions handler answers not found
without it; `lifecycle.rs` starts and stops Actions workers only with it. Stored webhook
subscriptions to Actions events are kept (nothing fires them while off).
`rbit-core`'s `WebhookEvent::subscribable(bool)` still takes the boolean: `rbit-core`
cannot depend on `rbit-actions`, and its callers pass the service's presence.

`rbit-web` reaches `Availability` through `rbit_services::actions`, so no crate edge is
added.

## Boundary that fails if legacy paths return

`scripts/rewrite/actions-disabled-retirement.mjs`, run by `check.mjs`, over production
sources of `rbit-services`, `rbit-web` and `rbit-server` (comments and closing test modules
stripped): every read of `.actions.enabled` must be the argument of `Availability::of`
(assignments, as in configuration and test fixtures, are not reads), and the refusal text
may appear only in `rbit-actions`. Against `59e6fa7b` it reports four violations
(services `actions/mod.rs` and `webhooks.rs`, web `api/mod.rs`); against this tree, none.
`scripts/tests/actions-disabled-retirement.test.mjs` pins it.

## Gates

| Gate | Evidence |
| --- | --- |
| Behaviour parity | Same conditions and text; the service is built by the same code, now inside `provide`. Existing suites run unchanged: `rbit-web` `actions_disabled` (route inventory with Actions off), the OpenAPI document tests for both settings, webhook tests; `rbit-e2e` `actions_disabled`. New: `availability` unit test. |
| Security | No route, worker or write appears while off; the gate is the single read. |
| Migration and rollback | No schema, migration, stored format or route change; latest migration 0073. |
| Performance | Unchanged; the switch is read at start-up and per webhook edit as before. |

## Results

Linux quick check at the code tip passed: format, workspace-wide clippy `-D warnings`,
rewrite contracts, licenses, front-end build, and nextest for `rbit-actions`,
`rbit-services` and `rbit-web` (1,499 passed, 1 skipped; this includes the `rbit-web`
`actions_disabled` route inventory, the OpenAPI document tests and the webhook tests).
`rbit-e2e` runs in the integrated full check.

## Cost

Both rows (`actions.runners` and this one), code commits only:

| Measure | Value |
| --- | --- |
| Code commits | 2 (plus checkers and this record) |
| Files changed by code commits | 11 |
| Lines added / removed | 1,081 / 301 |
| `rbit-actions` production | 5 files, +438 (runners 371, availability 67) |
| `rbit-actions` tests | 1 file, +260 |
| `rbit-services` and `rbit-web` | 5 files, +383 / −301 (one new 286-line adapter, `runner_ports.rs`) |
| Checker and policy scripts | two guards (47 and 37 lines), two tests, four lines in `check.mjs` |

Estimate for similar rows: where the decisions sit in one service file next to their
composition (runners), about one session: two code commits and 1,000–1,400 changed lines.
A composition rule like this one is an hour once the gate type exists.

Tricky parts: the runner registry's steps interleave the store, the runner cache, the
audit log and the scheduler's version, so the ports had to keep each in its old position;
error text is protocol, so every refusal moved byte for byte; `webhooks.rs` and
`routes/api/mod.rs` sit exactly at their size ceilings, so the edits had to keep their
line counts.
