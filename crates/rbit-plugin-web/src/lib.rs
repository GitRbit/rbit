//! Scoped request hooks and compiled Askama page contracts for trusted embedded plugins.
//! The host retains transport, authentication, body supervision, caching and security.
//! Askama, shared messages and cancellation primitives are used here; this crate
//! depends on no HTTP framework, forge services, database or Fluent implementation.

mod pages;
mod pipeline;
mod registry;
mod types;

pub use registry::{RequestHookRegistration, RequestHookRegistry, RequestHookSnapshot};
pub use types::*;

pub use pages::*;
