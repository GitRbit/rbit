# Authentication module boundaries and diagnostic privacy

The whole `rbit-auth` crate is selected for strict physical-file and cumulative
logical-type source checks. Historical exemptions for OIDC and WebAuthn files no
longer apply. Public module paths, exported types, fields and methods remain the
existing compatibility surface.

OIDC separates sealed flow state/PKCE/session-link derivation from provider-key
selection and compact ID-token signature/claim validation, with a focused typed
error module. Existing acceptance fixtures remain alongside these responsibilities;
the RFC PKCE vector lives inside the flow module so private verifier construction
does not become a wider API. WebAuthn's existing software-authenticator fixtures
move to its bounded child test module; ceremony verification stays unchanged.

The pure extraction preserves 31 OIDC production items and 21 test/helper items,
and 24 WebAuthn production items and 11 test/helper items. Comparisons use original
UTF-8 Git objects and Rust tokens/literal spellings, ignoring only imports,
comments, whitespace and formatting-only trailing commas. No fixture file,
algorithm body, key-size requirement, signature encoding/normalization, clock
leeway, nonce comparison, counter rule, error order or allocation strategy changes.
No dependency, crypto tuning, zeroization or ownership change is included.

## Opaque Debug boundary

Flow Debug no longer discloses state, redirect destination or linked account.
ID-token-check Debug does not disclose its nonce, and verified ID-token Debug does
not disclose subjects, email addresses, names, usernames or authentication methods.
NewCredential Debug hides credential identifiers and raw key bytes; the stored
public key is not itself a private signing key, but its association is private.
Ordinary and pretty Debug use the same opaque policy.

Already-redacted PKCE, flow keys, challenges, session/access/runner/task tokens,
TOTP secrets and recovery codes retain their existing implementations. Public
relying-party/provider/JWK metadata and typed errors are unchanged. Explicit field
access, serde, cookie sealing, browser options, signatures and stored credential
bytes retain their existing data; this is not a blanket logging-redaction claim.

Three additional Rust tests use genuine sealed-flow roundtrips, a locally signed
and verified ID token, and the existing software-authenticator registration path.
They assert both preserved data access and absence of private sentinels from both
Debug formats. They use no external provider, authenticator or real identity.

## Validation boundary

All 19 Rust files are at most 392 lines at this source checkpoint. Focused Node
policy fixtures, UTF-8/source bounds, formatting and diff checks pass. Complete
Rust tests, strict all-target Clippy and the actual cumulative AST checker remain
pending under the shared owned compiler lane. Prepared fixtures and source checks
are not native execution or a whole-platform security/performance result.
