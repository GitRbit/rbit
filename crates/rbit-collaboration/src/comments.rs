//! Thread comments and opt-in human comment lifecycle policy.

mod models;
pub use models::{Comment, CommentId, CommentSource};

#[cfg(feature = "applications")]
mod application;
#[cfg(feature = "applications")]
mod repository;
#[cfg(feature = "applications")]
pub use application::{CommentEditResult, HumanCommentError, HumanComments};
#[cfg(feature = "applications")]
pub use repository::HumanCommentRepository;
