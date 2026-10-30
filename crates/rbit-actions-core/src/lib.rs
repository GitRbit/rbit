//! Pure configuration and permission policies for rbit Actions.
//!
//! This crate has no dependency on the forge host, its stores, HTTP or async runtime.
//! Host-specific identities and persisted records remain in `rbit-core` during extraction.

pub mod config;
pub mod permissions;
mod tokens;

pub use config::{ActionsConfig, ActionsStorage, EVALUATION_CONCURRENCY, MAX_LONG_POLL_SECONDS};
pub use tokens::{DefaultTokenPermission, PermissionLevel, RunTrust, TokenPermissions, token_permissions};
