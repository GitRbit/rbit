# Session models and token compatibility contract

Identity owns SessionRecord, StoredSession, SessionUser, ClientInfo, ActiveSession
and the fixed-width TokenDigest alias without enabling crypto or an async runtime.
Legacy storage/service paths reexport the exact types. Owner-facing projection and
12-byte lowercase digest-prefix IDs now use the same feature-owned implementation.
The SQL active-session query still reads User/password-required/unread flags in one
query; its rows, filters and projection behavior are unchanged.

SessionRecord preserves its public field order, Serde derives, constructor, expiry
check and 256-character User-Agent truncation. Timestamp reads and all supplied
expiry values keep their existing behavior. Backend-private postcard encode/decode
become storage free functions: bytes, Corrupt message prefix and undecodable-to-None
semantics remain unchanged. redb tables/index transactions, expiry cleanup, atomic
renewal, sorting and revocation stay storage-owned in this preparatory atom.

The optional session-tokens feature enables existing base64/sha2/getrandom packages.
SessionToken retains all constructors, exact32-byte base64url decoding, SHA256 digest,
Clone behavior, redacted Debug and existing fatal OS-RNG error behavior. The legacy
auth session/root paths reexport that exact type. Its private shared bearer helpers
delegate to the explicit feature-owned token::codec functions, so PAT, runner and
task token prefixes/parsers/hints/matches retain their own unchanged implementations
and share the SAME codec. Codec functions are cryptographic byte operations, not an
authentication or capability grant. TokenDigest remains exactly [u8;32].

Auth explicitly requests credentials plus session-tokens; core/storage request no
identity features. Existing default identity API stays password-work. The token-only
layer adds no Tokio, Argon2, kernel or zeroize dependency; model-only remains contracts
and Serde. A normal model graph must be verified independently of workspace-wide
feature unification. The new token integration binary requires session-tokens, so a
model-only run cannot claim to have exercised token generation/parsing.

Prepared regressions include fixed SHA256/base64url token vectors and malformed
inputs, exact postcard field-order bytes/JSON, UTF8 truncation, projection/current
markers and legacy TypeId aliases. Existing real redb tests remain intact in a
separate descriptive test module, including no resurrection and cleanup races.
Integration retains existing external versions and adds only identity's base64/sha2
lock edges. Models-only (10 tests), credentials-only (12), token-only (12) and default
(43) identity configurations pass. Combined auth/core/storage/services tests pass,
including 535 service and 250 storage tests. Strict all-target Clippy, formatting,
source/type checks and graph-policy fixtures pass. CI now compiles token-only
independently, and a read-only graph gate rejects password/runtime dependencies in
that configuration as well as crypto/runtime dependencies in model-only consumers.

The SessionApplication and tracked password-session enforcement remain the following
activation atom. This model/token extraction creates no background worker or new
shutdown obligation and does not complete session application ownership or phase2.
