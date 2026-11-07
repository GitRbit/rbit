//! Compatibility delegation to the feature-owned fixed-width bearer codec.

pub use rbit_identity::TokenDigest;
pub(crate) use rbit_identity::token::codec::{SECRET_BYTES, decode, digest, encode, random_bytes};
