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

mod dashboard;
mod filter;
mod metadata_rows;
pub mod query;
mod results;
pub use dashboard::{DEFAULT_DASHBOARD_QUERY, dashboard_query};
pub use filter::{DashboardRelation, DashboardScope, SearchFilter};
pub use metadata_rows::{MetadataRows, OneIssueMetadata};
pub use results::{ISSUES_PER_PAGE, IssueHydration, IssueMetadata, IssueSearchResults, MetadataNeeds};
#[cfg(feature = "applications")]
mod listing;
#[cfg(feature = "applications")]
mod metadata;
#[cfg(feature = "applications")]
mod query_resolution;
#[cfg(feature = "applications")]
mod read_port;
#[cfg(feature = "applications")]
pub use listing::IssueListing;
#[cfg(feature = "applications")]
pub use metadata::IssueMetadataReader;
#[cfg(feature = "applications")]
pub use read_port::{IssueMetadataPort, IssueReadAccess, IssueReadError, IssueReadPort, IssueSearchPort};

#[cfg(feature = "applications")]
mod deletion;
#[cfg(feature = "applications")]
pub use deletion::{IssueDeletion, IssueDeletionError, IssueDeletionPort};

#[cfg(feature = "applications")]
mod mentions;
#[cfg(feature = "applications")]
pub use mentions::{DashboardMentionPort, DashboardMentions, MENTION_SOURCES};
