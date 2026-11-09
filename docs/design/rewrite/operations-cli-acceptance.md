# Command-line retirement progress

Status: first slice; the `operations.cli` ledger row stays `baseline-retained`. This
records what moved, what still blocks the row and the cost, following the
[identity retirement](identity-retirement.md) pattern.

## Where each command decides today

| Command | Decisions and owner | State |
| --- | --- | --- |
| Argument parsing, defaults, client dispatch | `rbit-server` `cli.rs`, `main.rs` | already in the target |
| `serve`, restart relaunch, shutdown order | `rbit-server` `serve.rs`, `main.rs` | already in the target |
| `migrate` | `rbit_storage::Database::open_with_key_file` (migrations are storage-owned) | already with its owner |
| `backup`, `restore` | `rbit-server` `backup/` | in another branch; not touched |
| `user create` | `rbit-server` `provision.rs` over `rbit_identity::accounts::AccountRegistration` | **moved by this slice** |
| Configuration layering (defaults, file, saved settings, environment) | `rbit-server` `config.rs`, `config/sources.rs` | already in the target |
| Deployment configuration validity and saved-document decoding | `rbit_services::admin::configuration::{validate_configuration, open_stored, CONFIGURATION_KEY}` | **remaining**; see below |
| Remote client (`auth`, `run`, `workflow`, `artifact`, `pr`, `api`) | `rbit-server` `client/` | already in the target |

## This slice: offline provisioning

`rbit user create` previously called `rbit_services::accounts::create_user_offline`,
a services entry point used only by the CLI. It now lives in
`crates/rbit-server/src/provision.rs`:

- The command keeps its process concerns: the database handle (`serve::open_database`,
  unchanged), the prompt and the password read from standard input, and a password-work
  owner local to the command, built from `security.max_password_jobs` exactly as the
  services helper built it.
- Name, email and password policy, the explicit administrator flag and the atomic
  insert are decided by `AccountRegistration::create_user` over `rbit_storage::AccountStore`,
  the same application and adapter as before.
- Errors go through `rbit_services::accounts::registration_error`, the existing mapping
  now exported, so `rbit: …` output keeps its text; both log lines are unchanged.
- The password is held in `Zeroizing` buffers from the read onward; previously the read
  buffer and its trimmed copy were plain strings until the services call wrapped them.

Removed: `rbit_services::accounts::create_user_offline` and the services-side generic
offline wrapper. The inactive `create_user_offline_original` path stays (it belongs to
the epoch composition and is exercised by `password_admission`).

New internal edge `rbit-server -> rbit-identity` (feature `password-work` only), recorded
in `ADDED_EDGES` and pinned in `identity-features.mjs`: the CLI composes the identity
application directly instead of through a services wrapper. `zeroize` is added to
`rbit-server`.

## Boundaries that fail if legacy paths return

`scripts/rewrite/operations-cli-retirement.mjs`, run by `scripts/rewrite/check.mjs`,
over every authored Rust file below `crates/`, comments ignored:

- `create_user_offline` (not `_original`) anywhere.
- `rbit_services::accounts::create_user…` or `accounts::create_user_offline` in
  `rbit-server`.
- `AccountRegistration` in `rbit-server/src` outside `provision.rs`.
- A `create_user` function back in `serve.rs`.

`scripts/tests/operations-cli-retirement.test.mjs` covers each rule, the allowed cases
and the whole tree.

## Remaining before the row can be `ported`

Startup validation and saved-configuration decoding are decided in the legacy services
crate (`admin/configuration.rs`) because the administrator configuration editor shares
them; `rbit-server` sits above services, so it cannot own them while the editor uses
them. The fitting owners sit below both: `validate_configuration` (and the site-name
rule it calls from `instance.rs`) in `rbit-core::config`, which would need a URL parser
dependency, and `open_stored` with `CONFIGURATION_KEY` next to the instance-settings
store in `rbit-storage`. Existing paths would stay as re-exports
(`crates/rbit-services/tests/admin.rs` uses `open_stored`). This overlaps the
`operations.admin` configuration editor, which is waiting for a re-target, so it was
left for that decision. Backup and restore entry points are reviewed with their own
branch.

## Results

Linux quick runs on the code commits (format, workspace clippy and tests for the changed
packages), plus local runs:

| Gate | Result |
| --- | --- |
| `cargo fmt --all --check`, workspace clippy `-D warnings` | pass (after a formatting fix and one test lint fix) |
| Tests for the changed packages (`rbit-server`, `rbit-services`, including `password_admission` and the new `provision` test) | pass |
| Rewrite contracts, including `check-types.mjs` | pass |
| Front-end build, licenses and advisories | pass |
| `node scripts/rewrite/check.mjs` | 0 violations |
| `node --test` for the new guard, `identity-features` and `rewrite*` tests | pass |

New test: `provision::tests` checks that the explicit flag decides administrator rights,
including for the first account, that the minimum password length applies, and that a
duplicate name is refused. Existing suites ran unchanged. The full workspace suite and
the Windows legs are left to the batch run.

## Migration, rollback and performance

No migration, schema, SQL or stored data changed; 0073 remains the latest migration.
Rollback is the previous binary. The CLI still opens only SQLite and constructs one
password-work owner; no service or redb store is started.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 2 (plus this record; the base type-check fix is cherry-picked) |
| Files changed by code commits | 14 |
| Lines added / removed | 175 / 46 |
| Production Rust (`src`, non-test) | 7 files; `provision.rs` 55 lines plus its test, `serve.rs` −15, services −22 |
| Checker and policy scripts | a 32-line guard, a 37-line test, the edge entry, one identity feature pin |

Tricky parts:

- Keeping the error text. The CLI prints the anyhow chain of a `ServiceError`, so the
  server reuses the services registration error mapping (now exported) instead of
  re-deriving the identity-to-service error table.
- The new `rbit-server -> rbit-identity` edge must request exactly `password-work`, so it
  is pinned in `identity-features.mjs` as well as allowed in `ADDED_EDGES`.
- Finding what the CLI still consumes from services: the startup configuration loader's
  validation and saved-document decoding are shared with the administrator editor, which
  is what keeps the row open.

Estimate: the remaining configuration move is one focused session, about 10 files and
200–300 changed lines (a URL-parser dependency for `rbit-core`, the site-name rule,
re-exports for the existing paths and a guard), once the `operations.admin` re-target
settles who owns the configuration editor.
