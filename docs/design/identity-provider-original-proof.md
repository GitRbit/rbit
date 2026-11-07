# Original provider receipt propagation (inactive composition)

Current public routes retain legacy callback/registration APIs. Explicit original
counterparts share the SAME callback worker through two private static modes;
this is producer closure, not ACTIVE startup/route/session activation.

## Genuine first acceptance

ExternalAuthService::callback retains state/cookie, PKCE, OIDC signature/nonce/
issuer/audience and GitHub verification. A provider nonce is never reinterpreted
as local password proof. The existing trusted VerifiedIdentity composition boundary
is unchanged. ProviderLoginRepository is not cryptographic/store-instance proof
against arbitrary malicious in-process implementations.

The alternate FIRST linked `(provider, subject)` or verified-email acceptance
SELECT projects the fixed epoch, current-target pending and informational User
fields together. It reads no PHC and adds no second acquisition/statement. Unique
principal_* aliases avoid colliding with identity id/email/time fields. LEFT JOINs
retain a known subject even when its account/login is missing; malformed/missing
facts fail closed instead of becoming unknown-subject registration/link fallback.
Legacy mode uses the exact existing SQL and carries unit origin without a receipt.

Only the identity application can mint opaque ProviderOrigin from this trusted
first snapshot. It has no public constructor/Serde/current-User restamping API.
ProviderAuthentication::first_factor consumes that origin with its original epoch;
captured pending stays unavailable even if later completed. Its User is informational,
not final issuance authority. ProviderAuthentication::complete remains the explicit
revalidating standalone API and is not called redundantly by the callback worker.

The common callback preserves find, touch/link, suspension scalar/failed audit,
MFA/trust policy, audit and result ordering. Original mode replaces its former
success-only User lookup with the first captured fields. The sole authoritative
final cookie issuer loads SessionUser at the FIXED epoch, including current flags,
locale and pending exclusion, in one statement. It never reads a fresh epoch after
old proof. Thus callback-only original success saves one lookup, and callback plus
final issuer has the legacy total statement/acquisition count. The additional
first-query account projection and final security checks are not claimed free or
a measured performance win. Informational captured fields never determine final
suspension/admin/name/locale authority. A changed epoch rejects even if account ID
was deleted and reused. Captured pending is protocol unavailability, not a wrong
provider credential/failure surcharge.

## MFA and creation

Local MFA retains the SAME receipt and configured audit method in the existing
clone-shared bounded pending owner. Legacy completion cannot consume an original
pending token. Before a one-use recovery/TOTP/passkey write, the original-MFA path
retains its fixed-epoch projection and atomic SQL write predicates. Trusted provider
MFA bypasses local MFA only under existing configuration + verified provider claim;
it still reaches the final fixed-epoch issuer. No new crypto/admission owner is added.

External registration retains validation, throttle, pending draft, setup/open policy,
verified-email behavior, conflict identity, event/audit and next URL semantics.
Original account/login/global allocation/link commit in ONE transaction; only then
can the feature application mint creation evidence. This adds exactly one global
allocation statement, no later epoch query. Drafts precede account creation and
therefore do not pretend to contain an old local account proof.

## Prepared acceptance and remaining closure

Actual temporary SQLite/redb fixtures compare callback + final issuer work, verify
reset/pending-completion stale refusal, current flag reload, account-ID reuse,
provider MFA bypass and local MFA non-consumption, original/legacy token separation,
suspension precedence, verified-email linking, registration conflict/retry/event
behavior and missing-login corruption. Recovery fixtures use the actual paired
store cleanup/ACK path; a SQL completed_at stamp alone is not cleanup proof.

```text
cargo test --locked -p rbit-services --test integration provider_original
cargo test --locked -p rbit-storage --test integration account_creation
cargo test --locked -p rbit-identity --no-default-features --features password-work --doc
cargo clippy --workspace --all-targets -- -D warnings
```

Rust/native gates are prepared but UNRUN here. Dependency/feature graph is unchanged:
model DTO/ports stay pure; provider application stays behind password-work; storage
implements only the pure model port. No new dependency or lock change.

Private ACTIVE Services and every production issuer/resolver/settings/finalizer,
all producer/writer interlocks, startup accepted-pair/recovery/drain and explicit
stopped restore/rollback must close and pass fault gates before selecting these
APIs remotely. Existing trusted-User issuance cannot remain a remote bypass then.
Normal startup is not made ACTIVE by these producer methods, and no current crash-
safe enforcement or full identity feature completion is claimed.
