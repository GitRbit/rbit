# Runtime configuration retirement acceptance

Acceptance evidence for the `operations.configuration` ledger row (split from
`operations.admin`): the restart-required configuration editor with sealed secrets, the
audit log, monitoring and graceful restart. Target `rbit-server`, with one documented
by-design exception.

## The by-design exception

Every web host gets the configuration editor, monitoring and the restart page, embedded
hosts included. `crates/rbit-web/tests/admin_configuration.rs` asserts that on purpose: it
builds `AppState::new` with no server, saves configuration sections, reads monitoring and
expects an embedded host without a restart supervisor to report restarts as unavailable.
`crates/rbit-services/tests/admin.rs` drives the editor through `AdminService`. Dependencies
run `rbit-server` → `rbit-web` → `rbit-services`, so neither suite can reach code that
lives only in the server. Those suites are unchanged, so the editor's storage, sealing and
audit adapter stays in `rbit-services`, and monitoring's request-path recorder stays in
`rbit-web` middleware. What the server owns is the runtime behind the port.

## What moved where

| Part | Before | After |
| --- | --- | --- |
| Document rules: secret-field naming, redaction, restoring secrets by provider ID, size and JSON checks, staging (sections kept, storage unmoved), validation | `rbit-services/src/admin/configuration.rs`, `rbit-storage/src/sealing.rs` | `rbit_core::config::document` |
| Site-name rule (part of validation) | `rbit_services::instance` | `rbit_core::instance::checked_site_name`; the service function delegates |
| Opening a stored, sealed document; the settings key | `rbit-services` | `rbit_storage::deployment_config` |
| Monitoring and restart decisions for the administration pages | `AppState` fields read by routes | the `rbit_web::runtime::RuntimeControl` port |
| Restart runtime for the `rbit` server | the web default | `rbit-server/src/serve/runtime.rs`, installed through `AppState::with_runtime` |

**Why `rbit-core` and `rbit-storage`.** The rules must be reachable from `rbit-services`
(the editor, for embedded hosts) and from `rbit-server` (the startup loader), without a new
dependency edge. `rbit-platform` is policy-limited to no internal dependencies and cannot
see `Config`. `rbit-core` already owns `Config` and every section validator the document
check calls, so the pure rules join them there. That adds only the external `serde_json`
and `url` crates (the latter is what `reqwest::Url` re-exports, so parsing is unchanged).
Opening a stored document needs the sealing key, which `rbit-storage` owns together with
the settings key. Both crates are existing dependencies of services and server.

**The port.** `RuntimeControl` answers monitoring snapshots, whether a restart can be
served, whether one is pending, and a coalesced restart request. `AppState::new` installs
`EmbeddedRuntime`, so embedded hosts and every existing test behave as before. The
`restart` field stays public (now an `Arc<watch::Sender<bool>>`, so `subscribe()` callers
compile unchanged). The `rbit` server builds its state with `runtime::state`, installing
`ServerRuntime`; the serve loop keeps subscribing before it accepts connections and drains
HTTP and SSH on a request, as before.

`rbit_services::admin::configuration::{CONFIGURATION_KEY, open_stored,
validate_configuration}` remain as re-exports, so the server's startup loader (the CLI
agent's files) needed no edit at all.

## Legacy code removed

- From the service editor: `redact`, `restore_secrets`, `is_secret`, the size, JSON,
  section and storage checks, `validate_configuration` and `open_stored` bodies, and
  their two unit tests (moved unchanged to `rbit-core`).
- `is_secret_field` and the site-name rule bodies from `rbit-storage` and `rbit-services`
  (re-exported or delegated).
- Direct reads of `state.restart` and `state.monitoring.snapshot()` in the administration
  routes.

## Boundaries that fail if legacy paths return

`scripts/rewrite/configuration-retirement.mjs`, on its own line in `check.mjs`:

- the service editor may not define the document rules again;
- `is_secret_field` is defined only in `rbit_core::config::document`;
- `rbit-web` routes may not read `state.restart` or monitoring snapshots directly; they
  go through `RuntimeControl`.

Against the pre-slice sources it reports three violations; on this tree, none.
`scripts/tests/configuration-retirement.test.mjs` covers each rule and the whole tree.

## Results

One Linux run of the repository's own jobs in quick mode at `e8fd75e4` (the code is
identical at this record's commit): whole-workspace format and all-target Clippy with
`-D warnings`, licenses and advisories, the front-end build, the rewrite contracts
(including `check-types.mjs`) and nextest for the changed packages (`rbit-core`,
`rbit-storage`, `rbit-services`, `rbit-web`, `rbit-server`).

| Job | Result |
| --- | --- |
| Format and all-target Clippy (whole workspace) | pass |
| Licenses and advisories, front-end build | pass |
| Rewrite contracts: `check.mjs`, script tests, `check-types.mjs` strict, component and password-boundary modes | pass |
| Nextest, changed packages | 2,085 run, 2,085 passed, 2 skipped (pre-existing): `rbit-core` 145, `rbit-storage` 453, `rbit-services` 776, `rbit-web` 684, `rbit-server` 27 |

Configuration-relevant tests, all passing and unchanged: services
`admin::configuration_stages_validated_settings_masks_secrets_and_can_reset`; every
`rbit-web` `admin_configuration` test (restart confirmation, origin and audit; the embedded
host without a restart receiver; monitoring; every section; credential handling;
validation; provider drafts); `rbit-server`
`restart::application_restart_reopens_storage_and_listeners_and_reloads_configuration`,
which restarts a real server through `ServerRuntime` and reloads a staged document through
the startup loader; the server's configuration source tests; the services instance
site-name tests. The moved document tests run in `rbit-core`, plus one new staging test.

## Parity argument

- Every message, limit and check order is unchanged: the size and JSON checks come before
  the pending document loads, then the sections, secrets, typed parse, validation and
  storage checks. Sealing, saving and audit text are untouched.
- Restart semantics are unchanged: available exactly while something subscribes,
  coalesced into one pending restart, unavailable answered before the audit entry.
- Routes, templates and localization are unchanged.

## Migration, rollback and performance

No migration, schema or wire format changed. Rollback is the previous binary against the
same data. Request paths add one virtual call on administration pages only. No release
measurement was taken.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (shared document rules, runtime port, boundary) plus this record |
| Files changed by code commits | 20 |
| Lines added / removed (all) | 432 / 181 |
| Production Rust (`src`) | 14 files, +364 / −180, including the moved document tests |
| Tests | one new `rbit-core` staging test; 4-case Node test |
| Checker and policy scripts | one new Node guard on its own `check.mjs` line |
| Edits to the CLI agent's area | none in the loader; three one-line edits in `rbit-server/src/serve.rs` (module, state constructor, removed import) |

Tricky parts:

- The embedded-host contract in the web suite rules out a server-only editor; the shared
  crate had to be one both services and server already depend on, which left `rbit-core`
  (rules) and `rbit-storage` (sealed document).
- Keeping `restart` a public field (now behind an `Arc`) let every existing subscriber
  compile unchanged while the routes moved to the port.

Estimate: one session, as planned. Moving the editor's storage and audit adapter itself
into the server would require changing the two suites' embedded-host expectations; it is
not planned.
