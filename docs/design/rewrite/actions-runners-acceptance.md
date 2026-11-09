# Actions runners acceptance

Acceptance evidence for the `actions.runners` ledger row ("Registration, scopes, labels,
heartbeat and hashed task credentials"), owned by `rbit-actions`. It follows the
[identity retirement](identity-retirement.md) pattern.

## What moved

`rbit-services/src/actions/runners.rs` held both the runner registry's decisions and its
composition. The decisions now live in `rbit_actions::runners`:

| Decision | Legacy place | Now |
| --- | --- | --- |
| `register`: validate, parse the registration secret, parse labels (host labels by `Backend::isolates_jobs`), one-use store transaction, refusals ("unknown, revoked, expired or used up" as `unauthenticated`, other labels than the pin as `forbidden`), admit, move the scheduler only for a runner with labels | `ActionsService::register_runner`, `Declared::new`, `invalid_registration`, `from_validation` | `RunnerRegistry::register`, `declared` |
| `declare`: validate, parse, refuse labels outside the pin (text unchanged), store; a missing row forgets the runner and is `unauthenticated`; label or host-label change (order aside) moves the scheduler; refresh with the declaration's time | `ActionsService::declare_runner` | `RunnerRegistry::declare`, `unpinned_label`, `labels_changed` |
| Registration-token terms: a minute up to 30 days, at least one use, 1 to `MAX_RUNNER_LABELS` pinned labels, expiry `now + max(lifetime, 60 s)` saturating, audit detail | `create_registration_token` | `RegistrationOptions::{expires_at, audit_detail}` (`RegistrationOptions` and its default are generic over the label; services alias it) |
| Who manages runners (administrator; own account; organisation owner, looked up only when it decides; repository administrator; readers `Forbidden`, others hidden) | `require_manager` | `management`, `ManagedTarget`, `Management` |
| Intervals (`long_poll_seconds` within the protocol's 25 s; 10 s heartbeat) | `intervals` | `runners::intervals` |
| When `last_online` is written (seen since and ≥ 60 s later) and what lists show | `flush_seen`, `RunnerCache::with_seen` | `seen_is_due`, `shown_online` |
| Instance runners listed for a repository only when they serve its owner | `runners_for_repository` | `listed_for_repository` |
| The 32 most recent claim keys per runner | `CachedRunner::{remember, remembered}` | `RecentKeys` |

Ports are static, as in `claims`: `RunnerLabels` (label parsing and stored capabilities),
`RunnerRegistration` and `RunnerDeclaration`. `rbit-actions` stays free of credentials,
storage and runtime: `rbit-services/src/actions/runner_ports.rs` implements the ports with
`rbit-auth` secrets and credentials, `actions_runners`, the runner cache, logs and audit.

Composition kept in services, unchanged in behaviour: the in-memory runner cache that
authenticates every protocol call by credential digest (`RunnerCache`, `CachedRunner`),
`RunnerError` and its mapping, the settings and REST flows (create, list and revoke
tokens; list, disable, enable and remove runners), which now call the policies above, and
`forget_runner`, which fails a removed runner's tasks through scheduling. The claim query
(scopes, labels, host labels and trust) is SQL owned by `actions.scheduling`; the claim
application is `rbit_actions::claims`.

## Legacy code removed

`DEFAULT_REGISTRATION_LIFETIME`/`MAX_REGISTRATION_LIFETIME` (re-exported from
`rbit-actions`), `SEEN_GRANULARITY_SECONDS`, `REMEMBERED_KEYS`, `RegistrationOptions`
(now an alias), `Declared`, `invalid_registration`, `from_validation`, the
register/declare bodies, the token-term checks, the manager match, the interval clamp,
the last-seen arithmetic and the recent-key ring. `random_uuid` moved to the adapter.

## Boundary that fails if legacy paths return

`scripts/rewrite/actions-runners-retirement.mjs`, run by `check.mjs`, over production
sources of `rbit-services`, `rbit-web` and `rbit-server` (comments and closing test modules
stripped; `rbit-actions` and `runner_ports.rs` exempt): an `actions_runners()`
`register`/`declare` call, a redefined runner constant, a `Declared`/`RecentKeys` type or
the retired helper functions, the registry's refusal texts, the long-poll clamp or the
`seen - written` rule. Against `59e6fa7b` it reports five violations; against this tree,
none. `scripts/tests/actions-runners-retirement.test.mjs` pins it.

## Gates

| Gate | Evidence |
| --- | --- |
| Behaviour parity | Same validation order, texts and codes; one store transaction per call as before; cache insert, warnings, info log and audit in the same order; scheduler bumps under the same conditions; the declaration's time is the one written as last seen. Existing suites run unchanged: `rbit-services` `actions_runners`, `actions_claims`; `rbit-web` `actions_runner`, `actions_tokens`, runner pages and API. The two unit tests that exercised moved code (`declarations_are_stored_with_their_host_labels`, `uuids_are_version_four`) moved with it to `runner_ports.rs`. New: `rbit-actions` `tests/runners.rs` (6). |
| Security | Credentials and registration secrets stay `rbit-auth` values, hashed as before; the pin, scope, trust and host-label rules are unchanged; the manager rule keeps its `NotFound`/`Forbidden` disclosure. |
| Migration and rollback | No schema, migration, stored format, protocol byte or route change; latest migration 0073. Rollback is the previous binary. |
| Performance | Authentication is still one digest and one map lookup; register and declare make the same store calls. One extra label-list copy per declaration for the order-insensitive comparison, as before. No release measurement claimed. |

## Results

Linux quick check at the code tip passed: format, workspace-wide clippy `-D warnings`,
rewrite contracts, and nextest for `rbit-actions`, `rbit-services` and `rbit-web` (1,499
passed, 1 skipped), which includes the runner protocol, claim, token and settings suites.
Locally: `node scripts/rewrite/check.mjs` 0 violations; script tests 6 of 6.

## Cost

Shared with [disabled Actions](actions-disabled-acceptance.md#cost).
