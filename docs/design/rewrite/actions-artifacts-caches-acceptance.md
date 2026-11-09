# Actions artifacts and caches: first slice

Progress record for the `actions.artifacts-caches` ledger row ("Checkout, cache/artifact
scope, safe archive extraction and retention"), ledger target `rbit-runner`. The row
stays `baseline-retained`: this slice moves the decisions the server and the runner
shared, and documents what remains. It was done together with the
[backends row](actions-backends-acceptance.md), which it retires.

## Where the decisions belong

`rbit-runner` is a binary that depends on no server crate, so the server cannot call
into it. The row's behaviour has three owners:

| Part | Owner |
| --- | --- |
| Checkout, the runner-local cache (layout, LRU eviction, `cache.max_bytes`), archive packing and safe extraction (no write through a link, only new files, `w/`/`h/` roots), artifact upload/download steps | `rbit-runner` (already there; unchanged) |
| Rules both sides apply: key/`restore-keys`/version format, restore probe order, scope derivation from the run, scope/ref naming, content-digest header, archive root names | runner contract, `rbit-workflow` (**moved by this slice**) |
| The server's shared cache and artifact store: files under `<data_dir>/actions`, quotas, eviction, retention sweep, zip/tar.gz downloads, REST API | `rbit-services` today; the [rewrite plan](../ground-up-rewrite-plan.md) assigns server "caches and artifacts" to `rbit-actions` (**remaining**) |

`rbit_workflow::protocol` is the runner's wire contract and the plan keeps
runner/workflow shared types free of server details, so it is the closest replacement
crate that both sides can use. No internal crate edge is added; `rbit-workflow` gains the
external `sha2` dependency (already in the lock file, used by the runner) so the scope
digest keeps its exact bytes.

## What moved

| Decision | Legacy place(s) | Now |
| --- | --- | --- |
| Key ≤ 512 characters, no comma; non-empty for the server | `services::caches::{MAX_CACHE_KEY_CHARS, valid_key}`; `runner::steps::cache::MAX_KEY_CHARS` and an inline check | `protocol::{MAX_CACHE_KEY_CHARS, cache_key_fits, valid_cache_key}` |
| At most 10 `restore-keys`; version is 64 lowercase hex | `services::caches::{MAX_RESTORE_KEYS, valid_version}` | `protocol::{MAX_RESTORE_KEYS, valid_cache_version}` |
| Restore order: exact key per read scope, then each prefix per read scope | loops in `services::caches::find_task_cache` and `runner::steps::cache::Store::find` | `protocol::{restore_probes, CacheProbe}` |
| Scope ↔ ref naming of the cache REST API | `services::caches::{scope_ref, ref_to_scope}` | `protocol::{scope_ref, ref_to_scope}` |
| Task scopes from run ref, trust, pull number; unsafe refs named `ref/<digest>`; scope path safety | `services::scheduler::{cache_scopes, ref_scope}`, `spec::CacheScopes::validate` closure | `spec::{CacheScopes::for_run, RunCacheScope, cache_ref_scope}`; the write scope of an untrusted run is `rbit_actions::trust::Isolation::cache_write_scope`, and `scheduler::cache_scopes` only composes the two |
| `Rbit-Content-Sha256` parsing | `services::artifacts::digest_from_hex` | `protocol::sha256_from_hex` |
| Archive roots `w` (workspace) and `h` (home) | `services::downloads::STORED_ROOT`, runner `Area::prefix` and `extract` | `protocol::{ARCHIVE_WORKSPACE_DIR, ARCHIVE_HOME_DIR}` |

All moved verbatim: same limits, comparisons, order, digest bytes and error text.

## Legacy code removed

`services::caches::{MAX_CACHE_KEY_CHARS, MAX_RESTORE_KEYS, valid_key, valid_version,
scope_ref, ref_to_scope}` and their unit tests; `services::scheduler::ref_scope` and the
body of `cache_scopes`; `services::artifacts::digest_from_hex` and its test (equivalents
now in `rbit-workflow`); the restore loops; `runner::steps::cache::MAX_KEY_CHARS`; the
`"w"`/`"h"` literals. `rbit-web` takes `scope_ref` from the contract.

## Boundary that fails if legacy paths return

`scripts/rewrite/actions-runner-retirement.mjs` (wired into `scripts/rewrite/check.mjs`)
rejects, in production `src` outside `rbit-workflow`: a redefined key or restore-key
limit; a function named `valid_key`, `valid_version`, `scope_ref`, `ref_to_scope`,
`ref_scope` or `digest_from_hex`; a `for prefix in … restore_keys` loop; a `CacheScopes {
repository_id | read | write … }` literal in a server crate (the runner's local `exec`
and fixtures may build one); and a `&str` constant equal to `"w"` or `"h"`. Baseline
`4868231e`: eleven violations; this tree: none.
`scripts/tests/actions-runner-retirement.test.mjs` covers every rule, the allowed cases
and the current tree.

## Gates

| Gate | Evidence |
| --- | --- |
| Behaviour parity | Existing suites pass unchanged: `rbit-web` `actions_caches`, `actions_artifacts`, `actions_runner_tasks` (scope strings incl. `untrusted/<run>`), `rbit-services` `actions_settings`, `actions_sweeper`, runner cache/artifact unit tests. The scheduler's scope unit test still passes through the adapter. New: `protocol::caches::tests` (limits, probe order, digest, ref mapping) and `spec::cache_scopes::tests` (trust matrix, digest naming). |
| Security | Untrusted runs still write only `pull/<n>` or `untrusted/<run>`; the server still takes scopes from the run, never the runner. Archive extraction, link checks and size/quotas are untouched. |
| Migration and rollback | No schema, migration, stored path, protocol byte or route change; latest migration 0073. Rollback is the previous binary. |
| Performance | Same store queries in the same order (one per probe until a hit); the probe iterator allocates nothing. No release measurement claimed. |

## Results

Checked on the Linux CI nodes at the final tip (format, clippy `-D warnings`, full
Linux test suite, rewrite contracts, front-end build, cargo-deny); the commit and
outcome are given in the slice report. Locally: `node scripts/rewrite/check.mjs` 0
violations, `node --test scripts/tests/actions-runner-retirement.test.mjs` 3 passed,
`cargo fmt --all --check` clean.

## What remains for this row

- Move the server cache and artifact store applications (upload acceptance, quotas,
  LRU eviction, retention expiry, orphan sweep, download conversion) out of
  `rbit-services/src/actions/{caches,artifacts,downloads,sweeper}.rs` behind ports with
  `rbit-storage` adapters. The plan names `rbit-actions` as their owner, not
  `rbit-runner`; the ledger target should be revisited (or the row split) when that
  slice starts.
- Splitting `rbit-runner/src/steps/cache.rs` (929 lines) by responsibility.

## Cost

Both rows, three code commits:

| Measure | Value |
| --- | --- |
| Files changed by code commits | 20 |
| Lines added / removed | 434 / 184 |
| `rbit-workflow` (production and tests, two new modules) | 10 files, +275 / −8 |
| `rbit-services`, `rbit-web`, `rbit-runner` | 8 files, +61 / −176 |
| Checker and policy scripts | 48-line guard, 48-line test, 2 lines in `check.mjs` |

Tricky parts: finding the target was the work. The ledger names the runner, but the
duplicated decisions sat on the server side and the runner binary cannot be a
dependency, so the shared contract crate had to take them. The runner accepted an empty
key input that the server refuses, so the key rule split into `cache_key_fits` and
`valid_cache_key` to keep both behaviours. A rustfmt-expanded import pushed
`steps/cache.rs` one line past its ceiling.

Estimate for the remaining server store move: ports plus adapters for two stores and
the sweeper, about 6–10 commits and 1,000–1,500 changed lines (several sessions), the
size of the identity rows.
