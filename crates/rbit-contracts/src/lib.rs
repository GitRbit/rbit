//! Shared account and repository identities, independent of feature implementations.
//!
//! These types preserve the existing signed-integer constructors and Serde newtype
//! representation. They do not introduce validation, allocation, I/O or runtime state.
//! Identity creation, persistence and authorization remain responsibilities of features.
//! Other shared contracts should enter only when a concrete dependency boundary needs
//! them; this crate is not a container for forge entities or configuration.

mod account_id;
mod repository_id;

pub use account_id::AccountId;
pub use repository_id::RepoId;
