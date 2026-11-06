# Password input ownership without changing acceptance policy

Status: source implemented; Rust validation pending integration. No PHC cost envelope,
MFA input bound, new password maximum, hash policy, configuration value or DB scan is
included. The finite PHC proposal remains in identity-credential-bounds.md.

Public service inputs still accept String and existing borrowed proof types; callers
continue awaiting the returned future. Password entrypoints now construct Zeroizing
owners synchronously before returning that future, including the legacy Passwords
facade, outer sign-in/Git wrappers, profile rename/email/password, recovery reset/admin
reset and password-confirmed MFA operations. Validation, throttle, authorization,
lookup, password admission, session lease and one-use proof ordering remains inside
the future. An unpolled future starts none of those effects. In particular missing
passkey lookup still precedes proof conversion, and unauthorized admin still precedes
all validation/hash/store effects.

Plaintext moves with mem::take directly into the next eagerly protected API; no await
occurs between unwrapping and re-protection. Private profile PHC helpers return
Zeroizing<String> so early admission failure and compare-and-set waits retain protected
expected-hash ownership. Verification still needs a distinct worker copy of that PHC
while the original is retained for SQL compare-and-set; the new copy enters the eager
protected verification call immediately. No plaintext clone was introduced.

The credentials feature now opts into the already-locked zeroize dependency; password-
work inherits it. Model-only and session-tokens-only feature selections remain separate.
Web form adapters explicitly depend on the existing zeroize version. Form public types,
String fields, Serde wire names/defaults and Debug trait remain available. Debug output
contains the form type only, never submitted fields. Form handlers synchronously move
passwords/confirmations and owned security proof payloads into Zeroizing before future
construction, including mismatch/throttle/lookup returns; remaining ordinary form fields
are non-password metadata. Sensitive handlers/forms are factored into feature submodules
to shrink their existing oversized parent files. Setup-token re-rendering and request
extractor buffers retain existing behavior and are not claimed as fully erased.

A pure identity classifier counts at most max(minimum, maximum).saturating_add(1)
Unicode scalar values and preserves the previous minimum-before-maximum result. Existing
callers still use maximum256 and their original error text/order. Pathological minima,
zero and usize::MAX do not swap the reported validation error. The classifier neither
truncates nor normalizes and does not replace existing 1024-byte login/profile-current
password checks. Independent full-count oracle tests cover ASCII, multi-byte scalars,
combining sequences, boundary lengths and contradictory limits.

New tests construct/drop all outer service futures through a Future+Send contract and
check zero admission/lease/proof/store effects, test existing early-error precedence,
legacy no-runtime unpolled construction, form Debug redaction and Serde compatibility.
These do not inspect freed memory or prove allocator-wide erasure: synchronous ownership
is established by the Zeroizing types/source and the existing safe drop instrumentation
around protected crypto handoffs. Existing password/session/recovery/MFA/HTTP cases,
feature-isolated checks and strict all-target Clippy must run after integration. No new
runtime validation or overall identity completion claim is made here.
