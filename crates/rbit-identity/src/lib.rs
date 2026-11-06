//! Identity credential primitives, independent of forge state and protocol adapters.
//!
//! Password algorithm and error compatibility live here so future identity services
//! need not depend on the legacy authentication-policy crate. HTTP, storage, user
//! records and configuration remain outside this boundary. The compatibility facade
//! currently schedules directly on Tokio; explicit bounded work is a separate adapter.

mod credential;

pub use credential::{PasswordError, Passwords};
