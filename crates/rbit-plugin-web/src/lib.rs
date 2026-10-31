//! Bounded generation-scoped metadata hooks for trusted embedded plugins.
//! The host retains transport, authentication, body, caching and security authority.

mod pipeline;
mod registry;
mod types;

pub use registry::{RequestHookRegistration, RequestHookRegistry, RequestHookSnapshot};
pub use types::*;
