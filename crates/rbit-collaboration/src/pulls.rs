//! Pull requests: their records, approval tallies and merge requirements, which branch
//! names and head repositories a pull request may name, the checks and order of opening
//! one, and review threads.

mod anchors;
mod branches;
mod head;
mod merge;
mod merging;
mod messages;
mod records;
pub mod review_threads;
mod reviews;

pub use anchors::{AnchorLine, DIFF_HUNK_LINES, LineMark, ReviewSide, hunk_excerpt};
pub use branches::branch_ref;
pub use head::select_head;
pub use merge::{MergeEvaluation, MergeFacts, MergeRules, Requirement, RequirementCheck, ReviewTally, evaluate, tally};
pub use merging::{MergeDefaults, current_mergeability, merge_gate, merge_message, merges_as_writer, needs_trial_merge, still_expected};
pub use messages::{
    CommitIdentity, HeadSpec, commit_message, default_merge_title, default_squash_body, default_squash_title, noreply_domain, noreply_email,
};
pub use records::{
    ChecksStatus, MAX_CONFLICT_PATHS, MergeStrategy, Mergeability, PullRequest, Review, ReviewComment, ReviewCommentId, ReviewId,
    ReviewState,
};
pub use review_threads::MAX_REQUESTED_REVIEWERS;
pub use reviews::{
    NOT_DISMISSIBLE, ReviewRefusal, ReviewVerdict, dismissal_message, empty_review_refusal, may_dismiss_review, may_edit_review,
    verdict_refusal,
};

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
