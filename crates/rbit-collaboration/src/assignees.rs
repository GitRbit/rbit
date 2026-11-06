//! Thread assignee changes, with active-user/grant checks supplied by a static adapter.

#[cfg(feature = "applications")]
mod application;
#[cfg(feature = "applications")]
mod port;
#[cfg(feature = "applications")]
pub use application::{AssigneeAccess, AssigneeAssignments, AssigneeError};
#[cfg(feature = "applications")]
pub use port::{AssigneeAttachmentKind, AssigneeAttachmentPort};
