# Identity group retirement

Status: first retirement slice. This plan scopes which identity ledger rows can be
retired now, what legacy code goes, which prepared ports become active and which
gates must hold before a row changes from `baseline-retained` to `ported`. It also
records the cost, so the remaining groups can be estimated from evidence.

## Scope

The identity group has eight ledger rows. Only two have their decisions implemented
in `rbit-identity` and selected by production today:

| Ledger row | Replacement owner | This slice |
| --- | --- | --- |
| `identity.bootstrap` | `rbit_identity::accounts::{AccountRegistration, bootstrap_setup_token}` | Retire |
| `identity.sessions` | `rbit_identity::accounts::PasswordAuthentication`, `rbit_identity::sessions::SessionApplication` | Retire |
| `identity.profile` | `rbit_identity::profiles::ProfileApplication` | Retired later; see [profile acceptance](identity-profile-acceptance.md) |
| `identity.recovery` | `rbit_identity::recovery::{RecoveryLinks, RecoveryPasswords}` | Retired later; see [recovery acceptance](identity-recovery-acceptance.md) |
| `identity.mfa` | `rbit_identity::{two_factor, second_factor}` | Retired later; see [acceptance](identity-mfa-acceptance.md) |
| `identity.passkeys` | `rbit_identity::passkeys`, `rbit_identity::second_factor` | Retired later; see [acceptance](identity-passkeys-acceptance.md) |
| `identity.external` | none: `rbit-services/src/external_auth.rs` | Out of scope |
| `identity.tokens` | `rbit_identity::access_tokens::AccessTokenApplication` | Retired later; see [token acceptance](identity-tokens-acceptance.md) |

The six remaining rows have no replacement application in `rbit-identity` yet. Each
needs a ported application, ports and storage adapters before it can be retired; this
slice does not claim them. The inactive epoch session paths (`EpochSessionApplication`,
`OriginalProofReceipt`, `*_original*` service methods, stopped/active migrations 0067
and 0068) also stay inactive: fenced issuance needs the writer, version and recovery
interlocks described in the [progress record](progress.md), and activating them would
change session storage. They are not legacy code and are not deleted.

## What production uses after this slice

- Sign-up, first-administrator bootstrap, setup-token policy and administrator/CLI
  provisioning: `AccountRegistration` over the `AccountRepository` port, implemented by
  `rbit_storage::AccountStore`. SQL transactions keep race authority for the atomic
  first administrator.
- Password sign-in: `PasswordAuthentication` with admitted `PasswordWork`; suspension is
  disclosed only after a correct password.
- Session start, resume (renewal and the absolute maximum age), end, listing, single and
  other-session revocation: `SessionApplication` over `SessionRepository` and
  `SessionUserRepository`.
- **Newly activated:** expired-session cleanup. The hourly server task called
  `KvStore::sessions().purge_expired()` directly, the only session lifecycle operation
  that bypassed the identity application. `SessionRepository` gains `purge_expired`,
  `SessionApplication::purge_expired` owns it, and the server task reaches it through
  `AccountService::purge_expired_sessions`.

`rbit-services::AccountService` remains the composition adapter: it maps application
errors to `ServiceError`, writes audit entries and publishes domain events. It holds no
session or registration policy. HTTP routes, cookies, CSRF, rate limits, admission and
error text stay in `rbit-web`, unchanged.

## Legacy code removed

1. The unadmitted `Passwords` facade (`rbit_identity::Passwords` and its
   `rbit_auth::Passwords` / `rbit_auth::password` aliases). Production stopped using it
   when bounded password work was activated; only compatibility tests still did. Its
   Argon2id parameters move to a private function shared by `PasswordWork`. `rbit-auth`
   no longer requests the identity `credentials` feature.
2. The session contract aliases on the legacy service crate:
   `rbit_services::accounts::{SessionUser, ActiveSession, ClientInfo}`. Callers import
   them from their defining module, `rbit_identity_models::sessions`.
3. The direct storage purge call in `rbit-server`.

## Boundaries that fail if legacy paths return

- `rbit-rewrite-check --password-boundary` scans every Rust file below `crates/`
  (sources, tests, benches, examples and build scripts), not only the services and server
  sources, and rejects any `Passwords` identifier or auth/identity namespace wildcard.
  The services and server source roots stay required anchors.
- `scripts/rewrite/identity-features.mjs` requires `rbit-auth` to request exactly
  `session-tokens` from identity.
- `scripts/rewrite/identity-retirement.mjs`, run by `scripts/rewrite/check.mjs`, rejects
  the removed service aliases and a direct `purge_expired` call outside the identity
  application and its storage adapter.

## Gates

| Gate | Evidence required before `ported` |
| --- | --- |
| Behaviour parity | Existing account, session and registration suites pass unchanged: `rbit-identity` (account workflows, session application/effects/tokens, password work), `rbit-services` `sessions`, `admin/registration`, `password_admission`, `password_policy`, `rbit-web` `security` and `password_protocols`, `rbit-e2e` `accounts`. New parity test: identity purge and the storage purge remove the same rows from the same fixture. |
| Security group | `rbit-storage` and `rbit-services` `identity_races` (renewal against revocation, suspension, password change) and the permission matrix pass unchanged. |
| Migration and rollback | No schema, migration, redb table or cookie format change. Rollback is the previous binary against the same data. No historical migration is edited. |
| Performance | Hashing parameters and admission are unchanged (same private Argon2 configuration, same `PasswordWork` budget). Purge runs the same storage function once an hour. No release measurement is claimed for this slice; no request path gains a call. |

The ledger rows record the replacement paths and [acceptance evidence](identity-retirement-acceptance.md).

## Cost

Three code commits changed 54 files: production Rust +82/−92 lines in 19 files, tests
+91/−57 in 23 files, plus the checker and policy scripts. Details and the estimate for
the remaining rows are in the [acceptance evidence](identity-retirement-acceptance.md#cost).
