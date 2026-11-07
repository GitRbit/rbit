//! Typed OIDC token and signing-key refusal reasons.

/// Why an ID token (or a key set) was refused. Messages are for logs, not for users.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OidcError {
    #[error("malformed: {0}")]
    Malformed(&'static str),
    #[error("signature algorithm {0:?} is not accepted (only RS256 and ES256)")]
    UnsupportedAlgorithm(String),
    /// No key in the set matches the token: fetch the keys again (they may have rotated).
    #[error("no matching signing key")]
    UnknownKey,
    #[error("signature does not verify")]
    BadSignature,
    #[error("claim check failed: {0}")]
    Claim(&'static str),
}
