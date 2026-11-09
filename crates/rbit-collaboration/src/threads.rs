//! Shared numbered-thread models and opt-in read/create row applications.

mod enums;
mod models;
mod new_issue;
mod number;
mod permissions;
mod text;

pub use enums::{IssueKind, StateReason};
pub use models::{Issue, IssueId};
pub use new_issue::NewIssue;
pub use number::IssueNumber;
pub use permissions::{CommentPermissions, IssuePermissions, comment_permissions, issue_permissions, thread_permissions};
pub use text::{IssueText, IssueTextError, IssueTitle, MAX_TEXT_BYTES, MAX_TITLE_CHARS};

#[cfg(feature = "applications")]
mod application;
#[cfg(feature = "applications")]
mod repository;
#[cfg(feature = "applications")]
pub use application::{OpenCounts, ThreadAccess, ThreadApplication, ThreadDraft, ThreadError};
#[cfg(feature = "applications")]
pub use repository::ThreadRepository;

#[cfg(feature = "applications")]
mod text_edit;
#[cfg(feature = "applications")]
pub use text_edit::{ThreadEditAccess, ThreadEditError, ThreadEditResult, ThreadEditor, ThreadTextChange, ThreadTextRepository};

#[cfg(feature = "applications")]
mod state_change;
#[cfg(feature = "applications")]
pub use state_change::{StateChange, ThreadStateAccess, ThreadStateChanger, ThreadStateError, ThreadStateRepository, ThreadStateResult};

#[cfg(feature = "applications")]
mod lifecycle;
#[cfg(feature = "applications")]
pub use lifecycle::{SystemThreadClosure, ThreadLifecycleRepository, ThreadLockError, ThreadLocking};
