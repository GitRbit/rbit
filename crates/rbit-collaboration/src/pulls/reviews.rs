//! Submitting, editing and dismissing reviews: who may, in which state, and with what.

use super::ReviewState;

/// How a review is submitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewVerdict {
    Comment,
    Approve,
    RequestChanges,
}

impl ReviewVerdict {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "comment" | "COMMENT" => Some(Self::Comment),
            "approve" | "APPROVE" => Some(Self::Approve),
            "request_changes" | "REQUEST_CHANGES" => Some(Self::RequestChanges),
            _ => None,
        }
    }

    /// The state a review submitted with this verdict gets.
    pub const fn state(self) -> ReviewState {
        match self {
            Self::Comment => ReviewState::Commented,
            Self::Approve => ReviewState::Approved,
            Self::RequestChanges => ReviewState::ChangesRequested,
        }
    }
}

/// Why `verdict` may not be submitted (after the comment permission): authors only
/// comment on their own pull request, and closed pull requests take only comments.
pub fn verdict_refusal(verdict: ReviewVerdict, is_author: bool, is_open: bool) -> Option<&'static str> {
    if verdict != ReviewVerdict::Comment && is_author {
        return Some("you cannot approve or request changes on your own pull request");
    }
    if !is_open && verdict != ReviewVerdict::Comment {
        return Some("the pull request is closed");
    }
    None
}

/// Why a review has nothing to submit: no body and no pending line comments, unless it
/// approves.
pub fn empty_review_refusal(verdict: ReviewVerdict, body_is_empty: bool, has_comments: bool) -> Option<&'static str> {
    (body_is_empty && !has_comments && verdict != ReviewVerdict::Approve).then_some("write a comment or leave line comments first")
}

/// Why a review cannot be edited or dismissed by its caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewRefusal {
    /// Pending reviews are private: they do not exist for anyone else.
    NotFound,
    Forbidden,
}

/// Only the reviewer edits their own submitted review.
pub fn may_edit_review(state: ReviewState, own: bool) -> Result<(), ReviewRefusal> {
    if state == ReviewState::Pending {
        return Err(ReviewRefusal::NotFound);
    }
    if !own {
        return Err(ReviewRefusal::Forbidden);
    }
    Ok(())
}

/// Only writers dismiss submitted reviews.
pub fn may_dismiss_review(state: ReviewState, writer: bool) -> Result<(), ReviewRefusal> {
    if state == ReviewState::Pending {
        return Err(ReviewRefusal::NotFound);
    }
    if !writer {
        return Err(ReviewRefusal::Forbidden);
    }
    Ok(())
}

/// The trimmed dismissal message, or the refusal when it is empty.
pub fn dismissal_message(message: &str) -> Result<&str, &'static str> {
    let message = message.trim();
    if message.is_empty() { Err("say why the review is dismissed") } else { Ok(message) }
}

/// The refusal for dismissing a review that neither approves nor requests changes.
pub const NOT_DISMISSIBLE: &str = "only approvals and change requests can be dismissed";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submission_rules_keep_their_order_and_text() {
        use ReviewVerdict::{Approve, Comment, RequestChanges};
        assert_eq!(verdict_refusal(Approve, true, false), Some("you cannot approve or request changes on your own pull request"));
        assert_eq!(verdict_refusal(RequestChanges, false, false), Some("the pull request is closed"));
        assert_eq!(verdict_refusal(Comment, true, false), None);
        assert_eq!(empty_review_refusal(Comment, true, false), Some("write a comment or leave line comments first"));
        assert_eq!(empty_review_refusal(Approve, true, false), None);
        assert_eq!(empty_review_refusal(RequestChanges, true, true), None);
        assert_eq!(ReviewVerdict::parse("REQUEST_CHANGES").map(ReviewVerdict::state), Some(ReviewState::ChangesRequested));
        assert_eq!(ReviewVerdict::parse("dismiss"), None);
    }

    #[test]
    fn edits_and_dismissals() {
        assert_eq!(may_edit_review(ReviewState::Pending, false), Err(ReviewRefusal::NotFound), "pending first");
        assert_eq!(may_edit_review(ReviewState::Approved, false), Err(ReviewRefusal::Forbidden));
        assert_eq!(may_edit_review(ReviewState::Commented, true), Ok(()));
        assert_eq!(may_dismiss_review(ReviewState::Pending, true), Err(ReviewRefusal::NotFound));
        assert_eq!(may_dismiss_review(ReviewState::Approved, false), Err(ReviewRefusal::Forbidden));
        assert_eq!(dismissal_message("  stale  "), Ok("stale"));
        assert_eq!(dismissal_message(" "), Err("say why the review is dismissed"));
    }
}
