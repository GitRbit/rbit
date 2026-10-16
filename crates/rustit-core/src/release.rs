//! Releases: named, tagged versions of a repository with notes and attached files.
//!
//! Milestone 2 workstream R (repository essentials) owns this file. The id type exists
//! already because it appears in domain events.

/// A release's database id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReleaseId(pub i64);
