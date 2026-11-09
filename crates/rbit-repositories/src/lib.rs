//! Repository models and statically dispatched ownership, lifecycle and discovery policy.
//! No concrete configuration, persistence, Git or runtime dependency enters this crate.
//!
//! Validated names preserve their original spelling. Storage implementations retain
//! authority over uniqueness, concurrency and row decoding; a model is not an access
//! capability. Account projections use identity's model-only dependency surface.

pub mod aliases;
pub mod archives;
pub mod browse;
pub mod contributions;
pub mod discovery;
pub mod edits;
pub mod forks;
pub mod lfs;
pub mod mirrors;
pub mod push_mirrors;
pub mod releases;
pub mod stars;
pub mod transfer;

mod listing;
mod model;
mod names;
mod visibility;

pub use listing::{ArchivedFilter, RepoSort};
pub use model::{ARCHIVED_READ_ONLY, OwnedRepository, Repository};
pub use names::RepoName;
pub use rbit_contracts::RepoId;
pub use visibility::Visibility;

pub mod settings;
pub mod ssh_keys;

pub mod creation;
