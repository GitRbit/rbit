//! Review threads: line comments grouped into resolvable conversations, who may resolve,
//! reply, dismiss or decide, which review cards show, and review requests.

use rbit_contracts::AccountId;

/// Reviewers that can be requested on one pull request.
pub const MAX_REQUESTED_REVIEWERS: usize = 15;

/// The root comment of a thread: replies point at their root, roots at themselves.
pub fn thread_root<T: Copy>(id: T, in_reply_to: Option<T>) -> T {
    in_reply_to.unwrap_or(id)
}

/// Whether `actor` may resolve or reopen a submitted thread: writers, or the root
/// comment's author. Commenting on the thread is checked first by the caller.
pub fn may_resolve(writer: bool, actor: AccountId, root_author: Option<AccountId>) -> bool {
    writer || root_author == Some(actor)
}

/// Whether `actor` may see (and reply to) a comment of a review that is `pending` and
/// written by `reviewer`; `None` when the review is gone. Pending reviews are private.
pub fn reply_visible(review: Option<(bool, Option<AccountId>)>, actor: AccountId) -> bool {
    match review {
        Some((pending, reviewer)) => !pending || reviewer == Some(actor),
        None => false,
    }
}

/// Whether a submitted review gets its own card: anything but a comment-only review
/// with an empty body whose threads all moved to their roots.
pub fn shows_review_card(threads: usize, comment_only: bool, body: &str) -> bool {
    !(threads == 0 && comment_only && body.trim().is_empty())
}

/// Who may dismiss reviews on the page: a signed-in viewer who may change the
/// repository's content.
pub const fn may_dismiss_reviews(signed_in: bool, can_write_content: bool) -> bool {
    signed_in && can_write_content
}

/// The review form a viewer gets: `None` when they may not comment, or when the pull
/// request is closed and they have no pending comments; otherwise whether they may
/// approve or request changes (open, and not the author).
pub fn review_form(can_comment: bool, is_open: bool, pending_comments: usize, is_author: bool) -> Option<bool> {
    (can_comment && (is_open || pending_comments > 0)).then_some(is_open && !is_author)
}

/// Whether `new` more requests fit next to `existing` ones; the refusal text otherwise.
pub fn requests_fit(existing: usize, new: usize) -> Result<(), String> {
    if existing + new > MAX_REQUESTED_REVIEWERS {
        return Err(format!("at most {MAX_REQUESTED_REVIEWERS} reviewers can be requested"));
    }
    Ok(())
}

/// The refusal for a requested reviewer who does not exist, is no user or cannot read
/// the repository: the same answer in every case.
pub fn reviewer_refusal(name: &str) -> String {
    format!("{name} cannot be asked to review this pull request")
}

/// The refusal for requesting the pull request's author.
pub const AUTHOR_REVIEW_REFUSAL: &str = "the author cannot review their own pull request";

/// A line comment as a conversation is assembled from it.
pub trait ThreadEntry: Sized {
    fn is_reply(&self) -> bool;
    /// The thread's root comment id (its own id for a root).
    fn root(&self) -> i64;
    fn set_replies(&mut self, replies: Vec<Self>);
}

/// Groups `comments` into conversations: each root, in order, with its replies (in
/// order), wherever they were submitted. `on_root` sees every root before its replies
/// are attached. Replies whose root is absent are dropped.
pub fn group_threads<T: ThreadEntry>(comments: Vec<T>, mut on_root: impl FnMut(&mut T)) -> Vec<T> {
    let mut replies: std::collections::HashMap<i64, Vec<T>> = std::collections::HashMap::new();
    let mut roots = Vec::new();
    for mut comment in comments {
        if comment.is_reply() {
            replies.entry(comment.root()).or_default().push(comment);
        } else {
            on_root(&mut comment);
            roots.push(comment);
        }
    }
    for root in &mut roots {
        root.set_replies(replies.remove(&root.root()).unwrap_or_default());
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Line {
        id: i64,
        root: Option<i64>,
        replies: Vec<i64>,
        seen: bool,
    }

    impl ThreadEntry for Line {
        fn is_reply(&self) -> bool {
            self.root.is_some()
        }
        fn root(&self) -> i64 {
            thread_root(self.id, self.root)
        }
        fn set_replies(&mut self, replies: Vec<Self>) {
            self.replies = replies.into_iter().map(|reply| reply.id).collect();
        }
    }

    fn line(id: i64, root: Option<i64>) -> Line {
        Line { id, root, replies: Vec::new(), seen: false }
    }

    #[test]
    fn replies_join_their_roots_in_order() {
        let grouped = group_threads(vec![line(1, None), line(3, Some(1)), line(2, None), line(4, Some(1)), line(5, Some(9))], |root| {
            root.seen = true;
        });
        let shape: Vec<_> = grouped.iter().map(|root| (root.id, root.replies.clone(), root.seen)).collect();
        assert_eq!(shape, [(1, vec![3, 4], true), (2, vec![], true)], "orphan replies are dropped");
    }

    #[test]
    fn permissions_and_cards() {
        let (ada, bob) = (AccountId(1), AccountId(2));
        assert!(may_resolve(true, ada, Some(bob)) && may_resolve(false, ada, Some(ada)));
        assert!(!may_resolve(false, ada, Some(bob)) && !may_resolve(false, ada, None));
        assert!(reply_visible(Some((false, Some(bob))), ada) && reply_visible(Some((true, Some(ada))), ada));
        assert!(!reply_visible(Some((true, Some(bob))), ada) && !reply_visible(None, ada));
        assert!(!shows_review_card(0, true, "  ") && shows_review_card(1, true, "") && shows_review_card(0, false, ""));
        assert!(shows_review_card(0, true, "lgtm"));
        assert_eq!(review_form(true, true, 0, false), Some(true));
        assert_eq!(review_form(true, true, 0, true), Some(false), "authors only comment");
        assert_eq!(review_form(true, false, 2, false), Some(false), "closed: finish pending comments");
        assert_eq!(review_form(true, false, 0, false), None);
        assert_eq!(review_form(false, true, 1, false), None);
        assert!(may_dismiss_reviews(true, true) && !may_dismiss_reviews(false, true) && !may_dismiss_reviews(true, false));
        assert_eq!(requests_fit(10, 5), Ok(()));
        assert_eq!(requests_fit(10, 6), Err("at most 15 reviewers can be requested".to_owned()));
        assert_eq!(thread_root(5, None), 5);
        assert_eq!(thread_root(5, Some(2)), 2);
    }
}
