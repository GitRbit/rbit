# Commit status retirement acceptance

Acceptance evidence for the `collaboration.statuses` ledger row ("External CI status API,
required contexts and commit/PR checks"), following the
[identity retirement](identity-retirement.md) pattern.

## Scope

| Behaviour | Owner after this slice |
| --- | --- |
| Status records and values: `StatusId`, `StatusState`, `StatusContext`, `StatusSource`, `CommitStatus`, `StatusError`, `parse_sha`, limits | `rbit_collaboration::statuses` (moved from `rbit_core::status`, which re-exports them) |
| Combined state, `CombinedStatus`, `StatusPage`, creator selection, required-context counting (`RequiredChecks`, `required_checks`) | `rbit_collaboration::statuses` (`checks.rs`) |
| Who may post (write access, not archived), task tokens bound to their own repository, check order, commit existence, per-context cap and its error text | `rbit_collaboration::statuses::StatusApplication` (new) |
| System statuses from rbit Actions (no checks, dropped at the cap) | `StatusApplication::record_system` |
| Reads: one page with its total, combined status of one or many commits, one report for the webhook | `StatusApplication::{page, combined, combined_of, find}` |
| Persistence | `CommitStatusRepository` port (new), implemented by `rbit_storage::sqlite::statuses::CommitStatusStore` in `rbit-storage/src/collaboration_statuses.rs` (new); the SQL is unchanged |
| Report field validation, including `target_url` | `rbit_core::status::StatusReport::parse` (unchanged; the target URL syntax is the webhook URL's) |
| Merge box summary `checks_status` | `rbit_core::status` (unchanged; it returns the pull request model's `ChecksStatus`) |

`rbit-services::StatusService` remains the composition adapter: it builds the access
from the request's `RepoContext`, resolves branch/tag/commit references through the
Git reader, parses the submitted body, maps application errors to `ServiceError`, logs
and publishes `DomainEvent::CommitStatus`. The `status` webhook payload
(`statuses/hooks.rs`) reads its report through the service. HTTP routes, JSON shapes,
templates and catalog text in `rbit-web` are unchanged.

## Legacy code removed

1. The status model and decisions in `rbit-core/src/status.rs` (state, context, source,
   record, error, SHA parsing, combined state, required checks): defined once in
   `rbit-collaboration` and re-exported under the old paths, so no caller changed.
2. Policy in `StatusService`: the write/archive checks, SHA and report validation order,
   commit lookup mapping, the `at_cap` error, the task-grant repository check, and the
   per-commit grouping and creator de-duplication of the read paths.
3. Every direct `commit_statuses()` call in the service and the webhook payload builder,
   including the `tokio::try_join!` of list and count (now the storage adapter's
   `page`).

## Boundaries that fail if legacy paths return

`scripts/rewrite/statuses-retirement.mjs`, run by `scripts/rewrite/check.mjs`, over every
authored production Rust file below `crates/*/src`, comments ignored:

- A definition of the status records or decisions (`CommitStatus`, `StatusState`,
  `StatusContext`, `StatusSource`, `StatusId`, `StatusError`, `RequiredChecks`,
  `CombinedStatus`, `StatusPage`, `combined_state`, `required_checks`, `parse_sha`,
  `creator_ids`) outside `rbit-collaboration/src`.
- A status write (`.commit_statuses().create(…)`, `create_as_system`,
  `CommitStatusStore::create…`) outside `rbit-storage`.
- In `rbit-services/src/statuses*`: any call on a status store, `parse_sha`,
  `MAX_STATUSES_PER_CONTEXT`, `combined_state`, `ensure_writable` or `at_cap`.
- The submission policy text ("reached the maximum number of statuses", "no commit
  found for SHA") outside `rbit-collaboration/src`.

`scripts/tests/statuses-retirement.test.mjs` covers each rule, the allowed
storage/test/comment cases and the whole current tree.

## Results

RESULTS_PLACEHOLDER

The existing suites ran unchanged: `rbit-services` `statuses`, `actions_statuses`,
`archives` and `pulls` (required checks in the merge box), `rbit-storage` `statuses`,
`rbit-web` status views and API routes, `rbit-e2e` `statuses`, and the `rbit-core`
status unit tests, which now exercise the re-exported model. New:
`rbit-collaboration/tests/status_policy.rs` (refusal order with no store call,
cap text, task-token repository binding, Actions authorship, combined reads, creator
selection).

## Parity argument

- Same check order and text: write access → `Forbidden`; archived → `Archived`; SHA
  syntax; report values; then the commit lookup, where a missing commit or a lookup
  returning another id is "no commit found for SHA …" and any other Git error maps as
  before. A task token for another repository is `Forbidden` before anything else.
  Refused submissions open no store (asserted by the new policy test).
- Same storage work: one capped insert per accepted status (`StatusAuthor::Account`
  selects `create`, `Actions` selects `create_as_system`); one `list` and one `count`
  joined concurrently for a page; one `latest` or `latest_of` query for combined
  status; one `find` for the webhook. Store handles are created per operation as
  before.
- Events, log lines and their fields are emitted by the service in the same places;
  the Actions bot sender and `source = actions` are unchanged.

## Migration, rollback and performance

No migration, schema, SQL or stored data changed; migration 0073 remains the latest and
no historical migration was edited. Rollback is the previous binary against the same
data. No request path gains a query or a store handle; the application is statically
dispatched and borrows nothing beyond the store it is given. No release measurement was
taken and none is claimed.

## Not covered

- Required-context configuration (branch protection rules, `MAX_REQUIRED_CONTEXTS`
  validation) belongs to `collaboration.protection`, and the merge-gate evaluation that
  consumes `required_checks` belongs to `collaboration.pulls`; both keep their current
  owners. The pull request merge box and the Actions status reporter still read
  `latest` from the store directly (reads only).
- `StatusReport` stays in `rbit-core` until the shared HTTP URL syntax has a home both
  webhooks and statuses can depend on.

## Cost

COST_PLACEHOLDER
