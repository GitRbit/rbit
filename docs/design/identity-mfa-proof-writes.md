# Dormant original-MFA proof writes

The original-MFA completion seam now carries its captured epoch into recovery-code, TOTP-step and passkey-counter SQL writes. Production sign-in routes and the legacy verifier/store APIs remain unchanged. This atom activates no session issuer, global epoch allocator, persisted writer mode or recovery protocol.

## Write authority and classification

`rbit-storage/src/sqlite/two_factor/claims.rs` retains the existing `claim_step`, `spend_recovery_code` and `use_passkey` method bodies/SQL. Their dormant `*_at_epoch` counterparts use one shared SQL predicate: the same account must have the supplied original epoch, remain unsuspended, and have no pending revocation intent for its **current** epoch. That predicate and the existing one-use condition participate in the exact same UPDATE. An earlier user SELECT is not treated as lasting authority.

Each dormant claim owns a short `BEGIN IMMEDIATE` transaction. Successful recovery/passkey writes use one counted statement/acquisition, as before. TOTP preserves its sealed-secret identity read and guarded step write, with the same two statements/acquisitions. Hashing, signature verification and secret decryption do not hold the write transaction. Explicit transaction begin/commit still have real overhead; existing `Work` counters exclude them, so count parity is not a latency claim. No compiler, database or browser performance measurement was run for this source atom.

On zero affected rows, a single indexed fence SELECT runs **inside the same held writer transaction**. Thus a concurrent reset cannot change the classification between a failed write and its inspection. `CredentialProofWrite` distinguishes Claimed, Rejected and Invalidated persistence facts; it is not an authentication receipt. The result is published only after a successful commit. Wrong/replayed proof with a current fence remains `InvalidCredentials` and keeps existing failure accounting. An invalidated origin becomes `NotFound`, ends its original pending entry, and does not record an incorrect-factor penalty/audit. A failed commit cannot return Claimed or consume a code.

`two_factor/verification.rs` has one cryptographic verification/control-flow implementation with two private static claim adapters. Legacy mode calls the unchanged store APIs. Original mode passes the existing receipt's account/epoch; it cannot refresh from a current User or later SQL epoch. The same configured method, pending claim, replay rules, throttle resets and audit path remain in `sign_in.rs`.

## Race boundary and remaining limits

Temporary-store unit tests use a private future parameter to pause **after** the fixed-epoch account SELECT and before verification/claim. Production supplies `Ready(())`, with no runtime setting, public hook, allocation or I/O. A second real SQLite connection commits a password+epoch/intent change, suspension, or an isolated same-epoch pending transition before the proof write proceeds. The tests check unchanged recovery use, TOTP step and passkey counter/last-used state, no incorrect-factor audit, removal of old pending and success with a newly captured proof after recovery.

A reset after a successful claim transaction may still precede the eventual issuer; that issuer must check the same original epoch again and never restamp it. Passkey challenges retain their existing in-memory one-attempt consumption before verification; this atom fences the persistent counter, not a durable challenge replay system. A newly issued challenge is required on a new pending attempt. Malformed proofs rejected before any claim retain their existing error ordering.

The test fixture's nonzero epoch and same-epoch pending insertion isolate predicates; they are not accepted migration history or an activated staging writer. Globally nonreused account creation/credential epochs, legacy epoch0 ambiguity, recordless redb orphans, persistent old-writer/version barriers, fenced issuance/resume/all mutations, exact-target keep-current recovery and crash/restore gates remain mandatory. These changes do not establish cross-store atomicity or crash-safe revocation. See `identity-activation-checklist.md` in the queued audit atom and the existing original-MFA/sidecar contracts.

## Prepared acceptance

Four private service tests cover all three factors across three deterministic post-lookup invalidations, fresh retry and success work-count parity. Three storage tests cover each successful legacy/original count and replay result, stale/suspended/current-pending versus historical-pending classification, and deferred commit failure/rollback. The existing software authenticator is shared unchanged between integration and unit fixtures; no second WebAuthn implementation or dependency is introduced.

Rust validation is pending the shared compiler lane:

```text
cargo test -p rbit-services --lib two_factor::sign_in::tests
cargo test -p rbit-storage --test integration original_proof_writes
cargo test -p rbit-services --test integration two_factor
cargo test -p rbit-services --test integration passkeys
cargo clippy -p rbit-identity-models -p rbit-storage -p rbit-services --all-targets -- -D warnings
```

Run the existing isolated model/token dependency checks and workspace gates as well. No migration, manifest, lockfile or dependency-edge update is needed. Source authors did not use Cargo or inspect user credentials/databases.
