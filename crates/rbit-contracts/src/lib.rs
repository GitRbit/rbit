//! Small stable contracts shared across independent feature boundaries.
//!
//! IDs and Unix-second timestamps preserve signed tuple constructors and persisted
//! Serde representations. Shared naming errors/slug grammar let identity and repository
//! names retain one validation authority without importing each other's implementations.
//! Timestamp's clock and English display helpers are retained compatibility behavior,
//! not localized presentation or an injected application clock. These contracts own no
//! feature configuration, store, task, runtime registry or authorization state.

mod account_id;
pub mod names;
mod repository_id;
mod timestamp;

pub use account_id::AccountId;
pub use names::NameError;
pub use repository_id::RepoId;
pub use timestamp::Timestamp;
