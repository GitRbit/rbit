//! Fixed-width bearer values. Digest models do not enable any crypto dependency.

pub use rbit_identity_models::token::TokenDigest;

#[cfg(feature = "session-tokens")]
mod access_token;
/// Exact 32-byte random/encoding/hash operations shared with legacy token adapters.
#[cfg(feature = "session-tokens")]
pub mod codec;
#[cfg(feature = "session-tokens")]
mod session_token;
#[cfg(feature = "session-tokens")]
pub use access_token::AccessTokenSecret;
#[cfg(feature = "session-tokens")]
pub use session_token::SessionToken;
