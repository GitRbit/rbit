# Original authentication proof and issuer activation audit

Status: inactive password-proof port connected through identity application, storage
adapter and an explicitly named AccountService method. Existing remote sign-in,
provider, MFA, signup and cookie-issuance paths remain unchanged. The prior intent,
SQL snapshot and dormant sidecar foundations do not yet provide durable enforcement.
No Cargo or real credential/database scan was run for this atom.

## This application/port atom

The model-only CredentialLoginRepository returns a CredentialLoginSnapshot: the PHC,
suspension and original epoch/current-target pending facts from ONE SQL statement.
AccountStore implements it by the existing find_login_snapshot projection, not a later
epoch lookup. It adds no dependency or runtime/crypto feature to model-only consumers.
PasswordAuthentication's legacy and new original-proof methods share the SAME loaded
verification worker; there is no second independently evolving password verifier or
admission owner. Legacy lookup and result signatures remain intact.

Both entries synchronously zeroize their owned plaintext before constructing a future.
The existing byte-bound check precedes lookup. After lookup, PHC ownership is protected
before callback/admission early returns. Ordering stays lookup -> read-only before_verify
callback -> shared admission -> real/covering verification -> suspension. The new path
then classifies pending only for a valid active-account password, otherwise preserving
wrong-password, store, callback, Busy/Closed and crypto error precedence. No dummy work
is added beyond the existing verify_login covering path, and no additional SQL statement
is issued by the current legacy path. Buffer copies below the persistence/extractor
boundary are not claimed universally zeroized by this application contract.

Only successful verification of the captured non-pending snapshot mints ProvenAccount
and OriginalProofReceipt. The receipt's constructor and fields are private, its Debug
is redacted and it has no Serde. It contains account ID + original epoch, no PHC. Accessors
and consuming reattachment preserve that same epoch and refuse a different account ID.
A User or a fresh current epoch cannot construct the receipt. The explicit repository
port is trusted application composition, not a sandbox against malicious in-process
implementations returning fabricated accounts/hashes.

Reset after lookup, even before Argon work, can leave a successfully verified OLD receipt.
This is intentional: it never adopts the reset's newer epoch. The eventual sidecar/SQL
reader rejects that stale epoch. A new pre/post SQL lookup would add work and still not
make issuance atomic with a subsequent reset; this atom adds neither a refresh nor a
claim that the receipt can never become stale.

AccountService.authenticate_original_proof maps a valid pending snapshot to generic
Unavailable(retry1), not invalid credentials. The adapter itself performs no audit or
failure-budget charges. Future sign-in orchestration retains those existing policies.
The ordinary authenticate/sign_in APIs and all current remote routes remain unchanged.

## Exact issuer inventory and remaining propagation

| Path and current authority | Required original-proof propagation before activation |
| --- | --- |
| Services.sign_in -> AccountService.authenticate_resolved -> SignInOutcome::SignedIn(User) | Use the new receipt result while retaining client/typed/account throttle checks, resets and audit ordering. Current enum/API stays compatibility; remote fenced outcome must retain proof. |
| Password sign-in with local MFA -> TwoFactorService.start_pending | PendingSignIn must carry the SAME original first-factor receipt, account, age, failures, challenge and method. No fresh epoch after password verification. |
| TwoFactorService.sign_in_options (passkey challenge) | Use the pending owner's retained proof; generating options must not mint a receipt from User. Avoid an extra SQL read just to rediscover the epoch. |
| Services.complete_sign_in | Replace its existing active-user query with an original-epoch fenced projection in the same statement/acquisition, then carry the SAME receipt through success. Never replace it with current epoch after proof consumption. |
| OIDC/GitHub callback -> finish_external_sign_in | Provider proof is independently verified by existing state/PKCE/nonce/signature/subject checks. Capture local account epoch with the EXISTING account-acceptance projection after that independent provider proof, BEFORE optional local MFA. Reset before this fresh provider acceptance need not disable provider auth; reset after capture fences its in-flight issuance. |
| Provider trust_mfa + verified amr mfa | Preserve existing bypass of local MFA. The accepted provider-account epoch still follows this direct cookie path; no password or extra Argon job. |
| Password signup -> AccountService.sign_up/AccountRegistration -> register_user | Return the creation epoch from the SAME authoritative account/login insertion transaction. No fresh lookup after returning User; bootstrap token/closed-registration races remain SQL authority. |
| External registration -> external_identities.register | Carry epoch0 from its own insertion/link transaction and preserve provider registration, verified-email and duplicate-link conflict handling. Do not mint from User after a separate lookup. |
| web::auth::start_session -> start_session_for(User) | Add a receipt-bearing remote funnel using fixed original account/epoch, preserving cookie settings/theme restore/password-change redirect. Do not discard proof into the legacy trusted-User API. |
| Public start_session(User)/start_session_for(User), fixture/offline callers | Remain explicit trusted embedding compatibility authorities. Persistent active mode must prevent unfenced remote routes/executables/writers from using them as a stale-proof bridge. A freely supplied User cannot establish historical proof. |

Provider acceptance currently first checks suspension (with failure audit) and then
queries the active account. Preserve that order/query count initially; adding epoch
facts to that existing account projection is distinct from opportunistically optimizing
those queries. Independent provider verification is not a late refresh of old password
proof: it is a fresh independent authentication origin. Any provider receipt mint API
must be owned by that verified-callback application, not public User+epoch construction.

The source paths are services/accounts/{password_authentication,registration,sessions},
identity/accounts/{authentication,registration}, services/two_factor{,/password_operations},
services/external_auth, web/routes/auth{,/password_commands}, security/password_commands
and external_auth. A future structural route guard must inventory every remote use of
the legacy trusted session start methods, not merely rename the common helper.

## MFA, passkey and step-up ordering

PendingSignIn currently holds account, monotonic start age, failure count, optional
passkey challenge and method, shared across cloned TwoFactorService instances. Finishing
removes the pending row once and checks expiry; cancellation/expiry during proof cannot
recreate it. Passkey options retain the single-use challenge in that owner; completion
takes it before verification. A future receipt field must follow that same clone/claim
lifecycle and bind the original account, never be reconstructed on the second step.

The existing finish path checks pending -> TwoFactor throttle -> active-account SQL
lookup -> optional challenge take -> verify -> single-use pending finish -> budget reset
and audit. Wrong proof records failure and pending attempt count; unrelated infrastructure
errors do not. A fenced replacement for the active-user lookup must preserve one-query
work and never consume TOTP/recovery/passkey proof on an earlier stale/unknown account
rejection. A reset racing AFTER that lookup can still consume an otherwise valid second
factor, as current race semantics allow; the unchanged original receipt then remains
stale at session resolution. Precise new stale/pending response mapping must be reviewed
before activating these flows, not implemented as a hidden bad-password penalty.

TOTP uses a stored-secret conditional claim of its time step; recovery codes are spent
by account-bound digest; passkeys verify origin/RP/challenge/signature/counter and CAS
advance their count. Receipt propagation must not bypass, duplicate or reorder those
single-use operations. There is no current passwordless passkey login route.

Settings step-up has DIFFERENT owners: per-account Ceremony::Register/Confirm challenge
queues and the existing check_password_and_proof path. These authorize sensitive actions,
not cookie issuance. Its TwoFactor throttle and shared password admission occur before
challenge/proof consumption; password is looked up/verified before second-factor work.
Do not silently reuse a session-issuance receipt for these commands or alter their
admission/consumption ordering. Credential-changing commands still require their own
verified snapshot/CAS and durable intent transaction, plus tracked finalization.

## Account lifecycle and independent credentials

Accounts and receipts bind immutable ID, not username/email spelling. Rename preserves
ID; its current password-conditional rename SQL and account redirect behavior remain
unchanged. A later fenced reattachment must use current authoritative User data for the
same ID, allowing legitimate rename while refusing a different account.

Suspension is currently checked after valid password verification and by active account
projection for session/MFA resolution. Admin suspension toggles its SQL flag and audits;
it does not currently advance credential epochs or delete every session. Unsuspension
can allow still-live existing state again. This atom preserves that behavior, rather
than claiming permanent suspension revocation. Whether to additionally fence suspension
or account reactivation is a separate approved policy and migration/fault-test decision.
Deleted/recreated account-ID reuse also needs explicit persistent activation treatment;
a receipt is not proof against arbitrary restored/deleted database history.

Password reset completion redirects to the existing completion/login flow; it does not
mint a new browser session here. Profile/admin/reset SQL password writes retain their
existing proof/token/CAS transaction ordering. PAT/API bearer, Git/LFS password-or-PAT,
SSH keys, Actions/task credentials and provider linking remain separate authorities;
they do not acquire a browser-session receipt or an extra Argon job through this seam.
Retain password-change-required checks, provider-MFA trust, independent token/key rules
and unknown-user covering behavior.

## Gates and prepared tests

The dependency graph is unchanged: models -> contracts/Serde; identity password-work
-> existing credentials/kernel/zeroize; storage -> models via the narrow snapshot trait;
services -> identity through its existing explicit password-work feature. No manifest,
lockfile, registry version, SQL migration or feature edge is added. Receipt types are
password-work gated; models/credentials-only/session-token-only consumers are not
silently widened. Existing Node identity-feature graph fixtures passed; actual isolated
Cargo trees/checks remain pending the shared compiler lane.

Prepared identity tests exercise actual shared PasswordWork against the snapshot port:
preverify reset retains original epoch with one lookup, redacted receipt/foreign account
reattachment refusal, pending/wrong/suspended/covering decisions, callback/store/Busy/Closed
precedence, exact byte bound/unpolled protected ownership, and cancellation retaining
submitted worker capacity. Existing legacy authentication tests remain the parity oracle.
Compile-fail doctests reject public construction and Serde serialization. Real service
fixtures assert one SQL statement/acquisition, shared admission count, pending generic
mapping with no adapter audit, and late-reset old-epoch rejection through fenced SQL.
These tests are prepared, NOT executed in this source-only atom.

```text
cargo test -p rbit-identity --no-default-features --features password-work --test account_workflows
cargo test -p rbit-identity --no-default-features --features password-work --doc
cargo test -p rbit-services --test integration password_admission::original_proof
cargo clippy -p rbit-identity -p rbit-identity-models -p rbit-storage -p rbit-services --all-targets -- -D warnings
```

Full activation still requires persistent legacy writer/executable interlocks, all issuer
receipt propagation, redb sidecar lifecycle, fixed-intent keep-current/supersession,
startup recovery BEFORE serving sessions and the complete SQL/redb/cancellation/restart
fault matrix. No current durable enforcement, cross-store atomicity, browser acceptance
or measured throughput improvement follows from this receipt-port atom.
