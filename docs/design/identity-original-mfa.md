# Inactive original-proof MFA propagation

The original password-proof port now has an explicit path through the existing
TwoFactorService pending owner. Current HTTP/password/provider sign-in orchestration
still creates legacy pending tokens, and current session issuers remain unchanged.
This is not durable fencing activation or a new passwordless/passkey login route.

`start_original_password_pending(ProvenAccount)` consumes the proven account and stores
the SAME opaque receipt alongside account, start age, failures, challenge and method
in the existing bounded clone-shared pending map. It hardcodes password provenance;
provider/signup receipt factories are not invented from User/current epoch. Legacy
start_pending records no receipt. The optional field increases each pending entry's
stored representation; no per-default-query or duplicate crypto work is added. This
is a source contract, not a measured throughput/memory claim.

Both completion APIs share one worker. Existing complete_sign_in handles only legacy
pending; complete_original_sign_in handles only original pending. Calling the wrong
API fails NotFound BEFORE throttle/query/challenge/proof consumption and leaves that
pending entry usable by its correct API. Thus the new token cannot downgrade into the
unfenced User-only completion API, and legacy state cannot be adopted as original proof.

The common worker preserves pending-age lookup -> TwoFactor throttle -> ONE account
projection -> optional challenge take -> existing verify -> single-use pending claim ->
budget reset/audit. The legacy branch uses its original find_active_user statement.
The original branch replaces that statement with find_session_user_at_epoch(account,
notifications=false, SAME captured epoch); it does not fetch the latest epoch or issue
a second account query. The pending receipt's fixed account/epoch are checked again
on claim, and reattachment uses only the authoritative user from that fenced query.
Rename/current role changes are read there; previously attached mutable User flags
are not authority. A missing/stale/suspended account ends pending and returns generic
NotFound before consuming a one-use proof or charging wrong-proof failure.

Passkey sign_in_options remains on the same pending/challenge owner and performs no
new epoch lookup or receipt mint. Recovery-code spending, TOTP step CAS and passkey
counter/challenge checks use the unchanged verifier. Wrong proof keeps the existing
failure/throttle/pending-attempt rules; cancellation/expiry before claim cannot revive
pending state. Reset racing AFTER the fenced lookup may still consume an otherwise
valid factor, as the existing proof/SQL race permits, but the returned receipt remains
at its ORIGINAL epoch. Only eventual fenced session issuance/resolution can reject it;
no atomic session/reset promise follows from this MFA port.

The receipt remains composition-trusted evidence minted by the verifier operating over
its supplied CredentialLoginRepository. It is not a cryptographic signature or a
store-instance/realm identity proof against arbitrary in-process port implementations.
Public bind_user checks account ID only; it cannot bless mutable privilege flags.
Final issuers must use the correctly composed fenced authoritative projection and
persistent active-mode owner, not trust an attached User. Cross-store/old-executable
interlocks, provider/signup origins and all remote cookie funnels still need activation
and fault acceptance together. Existing trusted-embedding APIs remain explicit
compatibility authorities, never a remote stale-proof bridge.

The large legacy two_factor module's pending owner and completion code move into
focused pending/sign_in modules; existing APIs keep their signatures. No manifest,
registry dependency, SQL schema, default account query or web route is changed.
The account locale reader may add projection data independently; this code uses only
its already-authoritative user and preserves epoch/pending predicates.

Nine temporary-store test cases are prepared, not run while the compiler lane is
occupied. They cover bidirectional API crossing without proof consumption, clone-shared
pending, unchanged completion SQL work, authoritative renamed/role-safe user data,
reset rejection before recovery-code consumption, one-use TOTP, failed-proof retry,
cancelled/expired claim, suspension/reactivation, original passkey challenge/counter
completion and stale-passkey rejection before counter update. Existing legacy MFA and
passkey suites remain the behavioral parity oracle. These are not restart or whole
SQL/redb finalizer crash tests.

```text
cargo test -p rbit-services --test integration two_factor
cargo test -p rbit-services --test integration passkeys
cargo test -p rbit-services --lib original_claim
cargo clippy -p rbit-services --all-targets -- -D warnings
```

The full issuer/ordering inventory and remaining activation graph are in
[the original-proof audit](identity-original-proof-audit.md). There is no current
production durable-enforcement, browser acceptance or measured performance claim.
