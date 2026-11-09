# Identity profile retirement acceptance

Acceptance evidence for the `identity.profile` ledger row: profile fields, email and
password changes, and account renames with redirects. It follows the
[identity retirement plan](identity-retirement.md), the
[first acceptance record](identity-retirement-acceptance.md) and the
[token retirement](identity-tokens-acceptance.md).

## What moved

| Decision | Before | Now |
| --- | --- | --- |
| Display name, bio, website and location validation (lengths, single-line and control-character rules, `http(s)://` websites with a host) | `rbit-services/src/profiles.rs` | `rbit_identity::profiles` (`fields.rs`) |
| Reading, publishing and replacing profile fields | `ProfileService` over `ProfileStore` | `ProfileApplication::{profile, public_profile, update_profile}` over `ProfileRepository` |
| Throttle order: rename bucket first, then the account password bucket; charging on failure, clearing on success; the oversized-password guard | `profiles/password_checks.rs` | `ProfileApplication` through the `CredentialThrottle` port |
| Current-password verification with admitted `PasswordWork` | `profiles/password_checks.rs` | `ProfileApplication` (private `verify_password`) |
| Rename: name parsing and the compare-and-set commit against the verified hash | `ProfileService::change_username` | `ProfileApplication::change_username` over `AccountRenameRepository` |
| Email change: parsing and the compare-and-set commit | `ProfileService::change_email` | `ProfileApplication::change_email` |
| New-password policy (configured minimum, 256 maximum, must differ), reservation timing and error precedence, verify-and-hash, the compare-and-set commit | `profiles/password_changes.rs` | `ProfileApplication::prepare_password_change` and `commit_password_change` |
| Profile records | `rbit_storage::sqlite::profiles::AccountProfile`, `rbit_services::profiles::PublicProfile`/`UpdateProfile` | `rbit_identity_models::profiles::{AccountProfile, PublicProfile}`, `rbit_identity::profiles::ProfileUpdate`; the old paths re-export or alias the same types |

`rbit-storage/src/identity_profiles.rs` implements both ports with the existing SQL. The
store keeps its `sqlx` row type private, so the identity record has no database
dependency. Every write keeps its original guard: the email and password updates and the
rename succeed only while the verified hash is current, so a concurrent reset or change
wins. Rename redirects are still written by the database (`0042_account_redirects`).
`AccountStore::find_by_old_name` and the repository alias application still resolve them;
neither changed.

`ProfileService` keeps its API and is now the adapter:

- It implements `CredentialThrottle` over the shared `ThrottleService`, using the same
  scopes and account keys.
- It maps `ProfileError` to `ServiceError`. Rejection text goes through `Display` and
  matches the old strings byte for byte.
- It logs, writes the `UsernameChanged` and `PasswordChanged` audit entries, and publishes
  `EmailChanged` and `PasswordChanged`.
- For a password change it supplies the session-finalization reservation and runs the
  commit, then `preserving_current` session revocation, inside the same tracked
  `finish_update` worker as before.

## Legacy code removed

1. `rbit-services/src/profiles/password_checks.rs` (throttle and verification helpers).
2. The validation helpers, constants and unit tests in `rbit-services/src/profiles.rs`.
   The unit tests moved verbatim into `rbit-identity/src/profiles/fields.rs`.
3. The verification, hashing, policy and compare-and-set calls in
   `profiles/password_changes.rs`.
4. The duplicate `AccountProfile` definition in `rbit-storage` and `PublicProfile` in
   `rbit-services`.

## Boundaries that fail if legacy paths return

`scripts/rewrite/profile-retirement.mjs` runs through `checkIdentityRetirement` (the same
chain as tokens). It rejects:

- `set_email_if_password`, `rename_if_password` or `set_password_hash_if_current` calls in
  production source outside the identity crates and storage;
- password-work admission or `verify`/`verify_and_hash` calls in the service profile
  adapter;
- profile limit constants (`MAX_DISPLAY_NAME_LEN`, `MAX_BIO_LEN`, `MAX_WEBSITE_LEN`,
  `MAX_LOCATION_LEN`) or the profile rejection texts in `rbit-services/src`.

`scripts/tests/profile-retirement.test.mjs` covers excerpts of the retired code, the
allowed owners and adapter handle, and the whole current tree.

## Results

Linux, at the branch tip built on the integrated stack: the repository's full check
(rewrite contracts including the structural type check, workspace-wide formatting and
Clippy with `-D warnings`, licenses and advisories, the front-end build, and the full
nextest suite) passed. Windows suites were not run for this slice. Locally:
`node scripts/rewrite/check.mjs` reports zero violations and the retirement script tests
(token, profile, recovery and identity chains) pass.

New tests, all in the passing set:

- `crates/rbit-identity/tests/profile_application.rs` (4 cases): validation before
  storage; the rename throttle first and the recorded order of throttle, hash load and
  charge; email change losing to a replaced hash; password-change length, same-password,
  reservation precedence, wrong password and a verified commit.
- `crates/rbit-identity/src/profiles/fields.rs`: the three original unit tests, moved, plus
  the exact rejection texts.
- `scripts/tests/profile-retirement.test.mjs` (3 cases).

`crates/rbit-services/tests/profiles.rs`, `identity_races` and the other existing suites
ran unchanged.

## Parity argument

- Routes, templates, catalog text and status codes are untouched; no `rbit-web` file
  changed.
- Order of effects for each operation matches the legacy code, step for step:
  - Rename: rename throttle, name parse, password throttle, oversized guard, hash load,
    admission, verification, charge or clear, guarded write.
  - Email change: parse, then the same verification sequence, then the guarded write.
  - Password change: length policy, same-password check, reservation (before the first
    await), password throttle, hash load, admission, then the deferred reservation error,
    verify-and-hash (a hashing failure clears the bucket), charge or clear, the guarded
    commit, and session finalization.
  - `crates/rbit-identity/tests/profile_application.rs` asserts these orders with a
    recording store and throttle.
- Plaintext passwords are wrapped in `Zeroizing` when the service method is called, as
  before. The identity methods wrap them again, synchronously, when they are called.
- Error mapping is variant for variant: store errors, name errors, admission and password
  errors convert as before; rejections become the same `ServiceError::Invalid` text.

## Migration, rollback and performance

No migration, schema or stored format changed; 0073 remains the latest migration. Rollback
is the previous binary against the same data. Each request performs the same storage calls
and password work in the same order. The only added work is constructing a borrowed
application value. No release measurement was taken and none is claimed.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (plus this record) |
| Files changed by code commits | 19 |
| Lines added / removed (code commits) | 921 / 205 |
| Rust under `crates/*/src` (moved unit tests included) | 14 files, +615 / −204 |
| Tests (`crates/*/tests`, `scripts/tests`) | 2 files, +264 |
| Checker and manifests | 3 files, +42 / −1; one new 36-line guard chained into the identity check |

Tricky parts:

- The password change is interleaved with infrastructure: a session-finalization
  reservation taken before the first await but reported only after admission, and a commit
  that must run inside the tracked finalization worker. The application takes the
  reservation as a closure and returns a prepared change; the commit is a separate function
  the worker calls.
- Throttle buckets are service state, but when they are checked, charged and cleared is
  policy. A small `CredentialThrottle` port keeps the order in identity.
- The profile record derived `sqlx::FromRow`; storage now maps a private row type instead.
