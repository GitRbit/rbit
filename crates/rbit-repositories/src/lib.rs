//! Repository-owned models without configuration, persistence, Git or runtime work.
//!
//! Validated names preserve their original spelling. Storage implementations retain
//! authority over uniqueness, concurrency and row decoding; a model is not an access
//! capability. Account projections use identity's model-only dependency surface.

pub mod discovery;

mod listing;
mod model;
mod names;
mod visibility;

pub use listing::{ArchivedFilter, RepoSort};
pub use model::{ARCHIVED_READ_ONLY, OwnedRepository, Repository};
pub use names::RepoName;
pub use rbit_contracts::RepoId;
pub use visibility::Visibility;
