//! Fixed-width bearer values. Digest models do not enable any crypto dependency.

pub type TokenDigest = [u8; 32];

/// Exact 32-byte random/encoding/hash operations shared with legacy token adapters.
#[cfg(feature = "session-tokens")]
pub mod codec;
#[cfg(feature = "session-tokens")]
mod session_token;
#[cfg(feature = "session-tokens")]
pub use session_token::SessionToken;
