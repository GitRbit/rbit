# Original creation producers (inactive composition)

Normal signup, provisioning, CLI and provider routes retain their existing behavior.
Explicit original counterparts now cover signup, explicit provisioning, temporary
administrator passwords, offline provisioning and verified-provider registration.
They are not selected until the complete ACTIVE startup/issuer/recovery/restore
bundle is ready. No schema, dependency, feature or lock change is introduced here.

## Transaction and authority

`AccountCreationRepository` owns three pure persistence operations: registration,
explicit creation and temporary-password creation. `CreatedAccountSnapshot` is a
plain User + checked epoch DTO, not an auth receipt. The concrete store returns it
only AFTER the account/login/initial-password/provider-link transaction commits.
No public User-to-proof factory or post-commit current-epoch query is added.

The common insertion algorithm retains the original order: insert account (taking
SQLite's writer lock), arbitrate first-administrator/closed-registration policy,
then insert login. Only original mode allocates ONE globally monotonic epoch in
that SAME transaction, after policy and before the login INSERT, and explicitly
writes it into the new login. Failed email/identity conflicts or deferred commit
failures roll back account, login, provisional allocation and link together.

Legacy insertion uses the same execute-only SQL, bindings, transaction and policy.
Original valid-path statement budgets, excluding BEGIN/COMMIT, are:

| Creation | Legacy | Original |
|---|---:|---:|
| Explicit account | 2 | 3 |
| Signup / first administrator | 3 | 4 |
| Temporary-password account | 3 | 4 |
| External account + identity | 4 | 5 |

Each owns one acquisition/transaction. Service signup also retains its preflight
count, so its total is five statements. Administrator audit and offline policy
retain their original costs; only creation adds one allocation. These are exact
source/work contracts, not latency claims. There is no allocation on the ordinary
request read path. The allocator is valid only after accepted SQL/redb floors and
exclusive compatible writers; calling original APIs alone is not activation.

## Feature ownership and secret lifetime

`AccountRegistration` shares signup policy and provisioning preparation across
both modes. The application protects owned plaintext synchronously before returning
its future, preserves validation/setup/throttle/admission/authorization ordering,
and hashes once using the same PasswordWork owner. The original branch mints opaque
ProvenAccount only from a successful creation-port result. Admin authorization still
precedes hashing/storage, and temporary-password marking remains in the insertion
transaction. New admin/offline entry points discard issuance evidence rather than
silently signing the created user in; their existing default entry points are unchanged.

Receipt definitions move into private `accounts/proof.rs` with unchanged public
aliases, redacted Debug, no Serde/constructor and ID-only bind_user semantics. The
creation port is trusted static composition, not cryptographic/store-instance
attestation against arbitrary implementations. A final issuer must reload current
flags at this captured epoch, never restamp a User with a fresh epoch.

Globally allocated creation epochs prevent numeric account-ID reuse from accepting
old account proofs, subject to the complete startup/writer/restore protocol. Actual
fixtures delete the highest account, recreate the same SQLite ID, check stale-epoch
refusal, reopen, and prove subsequent epochs increase. Rollback may reuse provisional
values because no receipt escapes a failed commit; committed values are never reused.
Overflow or malformed allocator state fails closed without an orphan account.

## Prepared gates

Prepared Rust fixtures cover shared setup/Busy/authorization ordering and unpolled
ownership, nonzero mock port facts, all creation work budgets, conflict/deferred
commit rollback, password/provider bootstrap races, global rollback/exhaustion,
ID reuse/reopen, admin temporary-password policy and offline first-admin behavior.
No Cargo/compiler/native run was performed in this checkout. Root must execute:

```text
cargo test --locked -p rbit-identity --no-default-features --features password-work --test account_workflows
cargo test --locked -p rbit-identity --no-default-features --features password-work --doc
cargo test --locked -p rbit-storage --test integration account_creation
cargo test --locked -p rbit-services --test integration password_admission::creation_proof
cargo clippy --workspace --all-targets -- -D warnings
```

Also repeat isolated models/token/session graphs and checks: pure models acquire no
runtime/password dependency. ACTIVE Services/routes, all fixed-epoch proof writes,
owned finalization, current-store restart/recovery, supported stopped restore and
rollback remain mandatory before production selection. No silent logout, PHC policy
change, durable recovery guarantee or full identity activation is claimed here.
