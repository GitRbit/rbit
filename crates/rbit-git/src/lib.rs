//! # rbit-git
//!
//! Everything rbit knows about Git *data*:
//!
//! * [`RepositoryStorage`] — where bare repositories live and how they are created/removed.
//! * [`RepositoryReader`] — a read model (refs, trees, blobs, history) over one repository.
//! * Insights derived from history: [`languages`] (language breakdown) and
//!   [`last_commit`] (the last commit that changed each directory entry).
//! * [`Maintenance`] — housekeeping (`git gc --auto`, commit-graph writes) delegated to the
//!   `git` executable.
//! * [`push`] — what a push changed: [`RefSnapshot`]s, [`RefUpdate`]s and the commits
//!   an update introduced.
//!
//! gitoxide is synchronous, so every async method here runs its work on Tokio's
//! blocking pool. Repositories are opened with *isolated* options: no system or global
//! Git configuration is consulted, which keeps behaviour deterministic and closes a
//! class of configuration-injection attacks.

pub mod archive;
pub mod blame;
pub mod changes;
pub mod compare;
pub mod diff;
// Milestone 3: editing files in the browser (E).
pub mod edit;
pub mod error;
pub mod fork;
pub mod languages;
pub mod last_commit;
pub mod lease;
pub mod maintenance;
pub mod merge;
pub mod mirror;
pub mod model;
pub mod overlay;
mod process;
pub mod pull_refs;
pub mod push;
pub mod reader;
pub mod relocate;
// Milestone 3: importing and push mirrors (M).
pub mod remote;
pub mod revision;
pub mod storage;

pub use changes::DiffLimits;
pub use error::GitError;
pub use languages::{LanguageShare, LanguageStats};
pub use last_commit::{LastCommitLimits, LastCommits};
pub use lease::RepositoryLease;
pub use maintenance::{Maintenance, MaintenanceError};
pub use model::{
    BlobView, ChangeStatus, CommitDetail, CommitSummary, EntryKind, FileDiff, FileDiffBody, RefDetail, RefKind, RefSummary, TreeEntry,
    TreeView,
};
pub use push::{RefSnapshot, RefUpdate};
pub use reader::RepositoryReader;
pub use storage::RepositoryStorage;
