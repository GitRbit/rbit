//! Compatibility paths for the identity credential primitives.
//!
//! These are reexports, not wrapper types: constructors, errors and legacy Tokio
//! scheduling behavior are unchanged. The implementation now belongs to identity.

pub use rbit_identity::{PasswordError, Passwords};
