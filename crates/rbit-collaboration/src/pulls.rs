//! Pull requests: which branch names and head repositories a pull request may name, and
//! the checks and order of opening one. Merging, reviews and background work are still
//! composed in `rbit-services`.

mod anchors;
mod branches;
mod head;
pub mod review_threads;

pub use anchors::{AnchorLine, DIFF_HUNK_LINES, LineMark, ReviewSide, hunk_excerpt};
pub use branches::branch_ref;
pub use head::select_head;

#[cfg(feature = "applications")]
mod creation;
#[cfg(feature = "applications")]
pub use creation::{
    BranchComparison, PullCreation, PullCreationAccess, PullCreationError, PullCreationPort, PullDraft, PullInput, PullSide,
};
#[cfg(feature = "applications")]
mod review_requests;
#[cfg(feature = "applications")]
pub use review_requests::{ReviewRequestError, ReviewRequestPlan, ReviewRequestPort, ReviewRequests, may_request_reviews, requested_names};
