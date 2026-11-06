//! Pure identity models and static ports, independent of application feature union.
//!
//! Credential operations and session runtimes stay in `rbit-identity`. Model consumers
//! depend here directly so enabling those operations elsewhere cannot widen this graph.

pub mod account;
pub mod accounts;
pub mod names;
pub mod sessions;
pub mod token;

pub use account::{Account, AccountId, AccountKind, User};
pub use names::{AccountName, Email};
pub use token::TokenDigest;
