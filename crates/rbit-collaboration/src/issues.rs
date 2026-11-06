//! Plain issue creation; shared thread rows also serve pull requests.

mod input;
pub use input::NewIssueInput;

/// Most labels on one thread.
pub const MAX_LABELS_PER_ISSUE: usize = 25;
/// Most assignees of one thread.
pub const MAX_ASSIGNEES: usize = 10;

#[cfg(feature = "applications")]
mod creation;
#[cfg(feature = "applications")]
mod creation_port;
#[cfg(feature = "applications")]
pub use creation::{IssueCreation, IssueCreationAccess, IssueCreationError};
#[cfg(feature = "applications")]
pub use creation_port::{IssueCreationPort, MetadataCreated};
