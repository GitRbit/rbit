# Integrations API acceptance

Acceptance evidence for the `integrations.api` ledger row ("REST/OpenAPI paths, errors,
pagination and scoped authorization"), following the
[identity retirement](identity-retirement.md) pattern.

## What "retired" means for this row

The row's target is `rbit-web`, which the
[rewrite plan](../ground-up-rewrite-plan.md#proposed-crate-responsibilities) makes the owner
of "Axum routes, HTTP extractors, response negotiation". The REST API's own conventions
already lived there. The audit asked where each API-wide decision is made:

| Decision | Before | Now |
| --- | --- | --- |
| Paths and methods, streaming routes, the `/api` catch-all, body/time/compression layers | `api/mod.rs` | unchanged (`api/mod.rs`) |
| The served OpenAPI document: base plus area fragments, Actions fragments only when enabled, merged once | `api/mod.rs` | `api/openapi_document.rs` (split out) |
| Credentials (`Bearer`/`token`), API throttling (`ApiAuth`, `ApiRequest`), scope checks, old-name redirects | `api/auth.rs` (`Caller`) | unchanged |
| Token authentication (hashing, expiry, suspended owners) | `rbit-identity` (`AccessTokenApplication`) via `TokenService` | unchanged (`identity.tokens`, ported) |
| **Scope semantics**: the scope order, `allows`, the `RepoAccess` ceiling (`limit`), names | **`rbit-core/src/access_token.rs` (legacy)** | **`rbit_access::TokenScope`**; `rbit_core::TokenScope` re-exports it |
| Task-token allowlist and the grant each endpoint needs (§11.3) | `api/auth.rs` | `api/task_routes.rs` (split out) |
| Pagination: `page`/`per_page`, `Link`, `X-Total-Count` | `api/pagination.rs` **and** a second copy of the headers in `api/actions.rs` (`wrapped`) | `api/pagination.rs` only (`Paged::wrapped`) |
| Error JSON, status codes, `WWW-Authenticate`, `Retry-After`, moved-repository redirects | `api/error.rs` | unchanged |

The only API-wide decision outside the target was the scope semantics in legacy
`rbit-core`. The [token acceptance](identity-tokens-acceptance.md) left it there because
capping `RepoAccess` is authorization policy and identity models may not depend on
`rbit-access`. `rbit-access` owns `RepoAccess` and needs no new edge, and `rbit-core`
already depends on it, so the type moved there unchanged. Identity still stores the scope
as an opaque type parameter.

## Legacy code removed

- The `TokenScope` definition and its methods in `rbit-core`. The `rbit_core::TokenScope`
  path is now a re-export of the same type, so storage, services and web call sites compile
  unchanged; a test pins the type identity.
- The duplicated pagination response in `api/actions.rs` (`wrapped`, with its own
  `x-total-count` and `Link` handling).

The shared API modules were also brought within the 400-line limit: `mod.rs` 494 → 382
(the OpenAPI document moved to `openapi_document.rs`), and `auth.rs` 413 → 314 (the task
allowlist moved to `task_routes.rs`).

## Boundary that fails if legacy paths return

`scripts/rewrite/api-retirement.mjs`, run by `scripts/rewrite/check.mjs`, scans authored
Rust below `crates/` (comments stripped) and rejects:

- `enum TokenScope`/`impl TokenScope` outside `rbit-access`;
- inside `rbit-web/src/routes/api/`, a convention outside its owning module: `Bearer`
  parsing, `services.tokens.authenticate` or the API throttle scopes outside `auth.rs`; a
  `TASK_ROUTES` table outside `task_routes.rs`; inserting `X-Total-Count` or `Link` outside
  `pagination.rs`; inserting `WWW-Authenticate` or `Retry-After` outside `error.rs`; and
  embedding `openapi*.json` outside `openapi_document.rs`.

Reading those headers in tests is allowed, and so is `git_http.rs` authenticating tokens for
Git. Run against the four legacy files at `27c02e7a`, the checker reports 4 violations;
against this tree, none. `scripts/tests/api-retirement.test.mjs` pins each rule, the
allowed cases and the whole tree.

## Gates

| Gate | Evidence |
| --- | --- |
| Behaviour parity | Code moved verbatim. `wrapped` keeps its behaviour exactly: `X-Total-Count` is always sent (0 without a total), unlike plain pages. The `rbit-core` scope tests moved to `rbit-access` unchanged. The OpenAPI-equals-routes test moved with the document, and the task-allowlist test moved with the allowlist; both bodies are unchanged. Every other API test (authentication and scopes, hidden resources answering 404, pagination links, disabled API, Actions lists and links) stays where it was. |
| Security | Same credential parsing, throttling order, scope comparison and repository ceiling; the task allowlist and its per-endpoint grant checks are unchanged. |
| Paths and errors | Same route table and OpenAPI document; same error bodies, status codes and headers. |
| Migration and rollback | No schema, migration or stored data; the stored scope strings are unchanged. Latest migration remains 0073. Rollback is the previous binary. |
| Performance | Unchanged; one fewer function hop for wrapped lists. No measurement claimed. |

## Results

Linux quick check run 219 on the slice's final code passed all five jobs: rewrite contracts
(including the strict type checker and this boundary check), workspace-wide format and
Clippy with `-D warnings` on all targets, cargo-deny, the front-end build, and nextest for
the changed packages (`rbit-access`, `rbit-core`, `rbit-web`): 835 tests passed, 1 skipped.
Locally, `node scripts/rewrite/check.mjs` reports 0 violations and
`node --test scripts/tests/api-retirement.test.mjs` passes. The full suite runs with the
integrated batch. No Windows build or test was run for this slice.

## Not covered

- **Rate limits:** the `ApiAuth`/`ApiRequest` budgets are defined in the shared
  `rbit-services` throttle, which also serves sign-in, Git and mail. The plan's admission
  owner is `rbit-kernel` ("bounded admission"). Moving the throttle is a cross-ingress
  change, not part of this row; the API only selects the scope in `Caller`.
- **`ServiceError` to API error mapping:** this stays in `api/error.rs` until the legacy
  service façade it maps from is retired.
- **Feature endpoint files** (`actions.rs` 1,878 lines, `issues.rs`, `pulls.rs`,
  `releases.rs`, `teams.rs`): their endpoints and JSON belong to the feature rows
  (`actions.*`, `collaboration.*`, `repositories.releases`, `access.teams`). The plan lists
  the `actions.rs` split explicitly; it is left to those rows to avoid conflicting with their
  work.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (scope move, API module split, boundary check) plus this record |
| Files changed by code commits | 12 |
| Rust lines | about +390 / −330, almost all moved |
| Checker | a 40-line Node guard, a 50-line test, two lines in `check.mjs` |

The tricky part was deciding scope: the target was already the legacy crate, so the work
was an audit of where each API-wide decision is made. That found one decision in legacy
code (scope semantics) and one duplicated convention (wrapped-list pagination). Estimate
for similar "target is the adapter" rows: half a session, mostly audit.
