//! Shared numbered-thread models and opt-in read/create row applications.

mod enums;
mod models;
mod new_issue;
mod number;
mod text;

pub use enums::{IssueKind, StateReason};
pub use models::{Issue, IssueId};
pub use new_issue::NewIssue;
pub use number::IssueNumber;
pub use text::{IssueText, IssueTextError, IssueTitle, MAX_TEXT_BYTES, MAX_TITLE_CHARS};

#[cfg(feature = "applications")]
mod application;
#[cfg(feature = "applications")]
mod repository;
#[cfg(feature = "applications")]
pub use application::{OpenCounts, ThreadAccess, ThreadApplication, ThreadDraft, ThreadError};
#[cfg(feature = "applications")]
pub use repository::ThreadRepository;
