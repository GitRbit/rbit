# Identity retirement acceptance

Acceptance evidence for the `identity.bootstrap` and `identity.sessions` ledger rows,
retired by the [identity retirement plan](identity-retirement.md). The other six
identity rows remain `baseline-retained`.

## Source under test

The code commits on top of `dc3561ec`:

1. Run expired-session cleanup through the identity application.
2. Retire the unadmitted `Passwords` facade.
3. Import session contracts from the identity models.

## Results

One Linux run of the repository's own CI jobs at the third commit: rewrite contracts,
formatting and all-target Clippy, licenses and advisories, the front-end build and
nextest passed. Nextest ran 3,174 tests across 125 binaries: 3,174 passed, 2 skipped
(the skips predate this change), none failed. Windows legs were not run for this slice.

Identity-relevant groups within that run, all passing:

| Group | Tests |
| --- | --- |
| `rbit-identity` unit (accounts, credential, password attempt/input, sessions) | 23 |
| `rbit-identity` integration (account workflows, credential inventory, model compatibility, password compatibility/login/reuse/work/cancellation, session application/effects/tokens) | 57 |
| `rbit-identity-models` | 16 |
| `rbit-services` unit: accounts, admin, profiles, recovery, two-factor | 30 |
| `rbit-services` integration: `sessions` 7, `admin` (registration) 15, `password_admission` 21, `password_policy` 1, `profiles` 6, `recovery` 14, `two_factor` 17 | 81 |
| Security group: `rbit-services` `identity_races` 7, `rbit-storage` `identity_races` 13 | 20 |
| Permission group: services 6, web 12, SSH 1 | 19 |
| `rbit-storage`: redb `kv` (sessions included) 43, `accounts` 4, `account_creation` 6, `recovery` 9 | 62 |
| `rbit-web` integration: `security` 7, `password_protocols` 6, `hostile_load` 6 | 19 |
| `rbit-e2e`: `accounts` 7, `security` 3 | 10 |
| `rbit-auth` (remaining primitives) | 44 |
| `rbit-server` serve (session purger shutdown included) | 3 |
| `rbit-rewrite-check` password boundary | 10 |

New tests added by this slice, all in the passing set:

- `rbit-services` `sessions::identity_purge_matches_the_storage_purge`: the legacy store
  purge and the identity purge run against identical fixtures (two expired, two live
  sessions) and must remove the same count and leave the same survivors.
- `rbit-identity` `session_application::purge_delegates_once_to_the_store_without_accounts_or_clock`.
- `rbit-server` `session_purger::stopping_releases_the_store_before_maintenance_owner_returns`,
  now driving the purger through `AccountService`.
- `rbit-identity` `credential::tests::{dummy_hash_is_well_formed_and_uses_the_configured_parameters, hash_then_verify}`.
- `rbit-rewrite-check` `every_crate_target_is_checked_once_the_facade_is_retired`.
- `scripts/tests/identity-retirement.test.mjs` (four cases, including the whole tree).

Locally: `node scripts/rewrite/check.mjs` reports zero violations, `cargo fmt --all
--check` passes, and `node --test scripts/tests/*.test.mjs` passes 615 of 623 with 7
skips; the one failure (`htmx-vendor.test.mjs`) needs `node_modules`, which the checkout
did not have installed, and is unrelated to identity.

## Parity argument

- Routes, status codes, cookies, CSRF, rate limits, admission and error/catalog text are
  untouched: no `rbit-web` route, middleware, template or localization file changed. The
  only web edits are two import paths for the same types.
- Session contracts moved import path only; the types are identical
  (`rbit_identity_models::sessions`), so serialization and wire bytes are unchanged
  (`model_compatibility` and `session_tokens` assert the fixed bytes).
- Password hashing keeps Argon2id v1.3, m=19456, t=2, p=1 and the same dummy hash; the
  fixed published vector and fresh-salt tests pass through `PasswordWork`.
- Purge runs the same storage function on the same schedule; the parity test proves the
  same removals.

## Migration, rollback and performance

No migration, schema, redb table or cookie format changed; migration 0072 remains the
latest and no historical migration was edited. Rollback is the previous binary against
the same data. No request path gained work; no release measurement was taken for this
slice, and none is claimed.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (plus plan and this record) |
| Files changed by code commits | 54 |
| Lines added / removed (all) | 411 / 190 (docs and checks included) |
| Production Rust (`src`, non-test) | 19 files, +82 / −92 |
| Tests and fixtures | 23 files, +91 / −57 |
| Checker and policy scripts | Rust boundary widened; one new 40-line Node guard with a test |

Tricky parts:

- Most of the work had already been done by earlier strangler slices: registration,
  password sign-in and session decisions already ran in `rbit-identity`. Retiring them
  was mostly finding the last bypass (the hourly purge) and the compatibility surfaces.
- The facade's Argon2 constructor was shared by the admitted primitives; it had to move
  to a private function and be gated on `password-work` so the credentials-only feature
  graph stays free of dead code.
- Widening the password boundary from two source roots to every crate needed the
  link/anchor semantics preserved; the existing Unix link fixtures pin them.
- The shared build lane was held by other worktrees for the whole session, so local
  compilation was not possible; the Linux CI jobs were the compile and test gate.

Estimate for the remaining six rows: unlike these two, none has an identity application
yet. Each needs ports in `rbit-identity-models`, an application in `rbit-identity`,
storage adapters and service delegation before any deletion, closer to the size of the
earlier session/account stacks (tens of commits each) than to this slice.
