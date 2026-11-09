//! Pull requests: which branch names and head repositories a pull request may name, and
//! the checks and order of opening one. Merging, reviews and background work are still
//! composed in `rbit-services`.

mod branches;
mod head;
pub mod review_threads;

pub use branches::branch_ref;
pub use head::select_head;

#[cfg(feature = "applications")]
mod creation;
#[cfg(feature = "applications")]
pub use creation::{
    BranchComparison, PullCreation, PullCreationAccess, PullCreationError, PullCreationPort, PullDraft, PullInput, PullSide,
};
