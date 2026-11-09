//! What pushes to a head or base, and new statuses, mean for open pull requests: whether
//! the stored trial merge is still current, which line comments became outdated, and
//! the list badges.

use std::collections::HashMap;

use rbit_contracts::AccountId;

use super::{Mergeability, Review, tally};

/// A stored trial merge is current when it is known and was computed on today's base
/// tip and head. Otherwise the pull request is queued for a new one.
pub fn trial_merge_is_current(
    stored: Mergeability,
    computed_on: (Option<&str>, Option<&str>),
    base_tip: Option<&str>,
    head_sha: &str,
) -> bool {
    stored != Mergeability::Unknown && computed_on.0 == base_tip && computed_on.1 == Some(head_sha)
}

/// A revision and the paths to look up in it.
pub type PathRequest = (String, Vec<String>);

/// The path lookups outdated line comments need, for comments given as `(commit,
/// path)`: first every comment's path at `head`, then one request per distinct commented
/// commit with the paths commented there. Also returns, per comment, where its own
/// lookup is: `(request after the head's, position in its paths)`.
pub fn outdated_requests(comments: &[(&str, &str)], head: &str) -> (Vec<PathRequest>, Vec<(usize, usize)>) {
    let mut requests = vec![(head.to_owned(), comments.iter().map(|(_, path)| (*path).to_owned()).collect())];
    let mut by_commit: HashMap<&str, usize> = HashMap::new();
    let mut then_at = Vec::with_capacity(comments.len());
    for &(commit, path) in comments {
        let request = *by_commit.entry(commit).or_insert_with(|| {
            requests.push((commit.to_owned(), Vec::new()));
            requests.len() - 2
        });
        let paths = &mut requests[request + 1].1;
        let position = paths.iter().position(|known| known == path).unwrap_or_else(|| {
            paths.push(path.to_owned());
            paths.len() - 1
        });
        then_at.push((request, position));
    }
    (requests, then_at)
}

/// The comments (by index) whose file differs at the head (`now`, one answer per
/// comment) from their own commit (`then`, per commented commit, located by `then_at`).
/// A commit that cannot be read any more knows no file: outdated unless gone now too.
pub fn outdated_comments<T: PartialEq>(now: &[Option<T>], then: &[Option<Vec<Option<T>>>], then_at: &[(usize, usize)]) -> Vec<usize> {
    then_at
        .iter()
        .enumerate()
        .filter(|(index, (request, position))| {
            let was = then[*request].as_ref().and_then(|ids| ids[*position].as_ref());
            was != now[*index].as_ref()
        })
        .map(|(index, _)| index)
        .collect()
}

/// A pull request's list badges: approvals and change requests of every reviewer's
/// latest decisive review, without the author and without dismissing stale approvals.
pub fn list_badges(reviews: &[Review], author: Option<AccountId>, head_sha: &str) -> (usize, usize) {
    let tally = tally(reviews, author, head_sha, false, |_| true);
    (tally.approvals.len(), tally.changes_requested.len())
}

/// The offset of 1-based `page` (0 counts as 1) of `page_size` items.
pub const fn list_offset(page: u32, page_size: u32) -> u32 {
    let page = if page == 0 { 1 } else { page };
    (page - 1).saturating_mul(page_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trial_merges_are_current_only_where_computed() {
        let on = (Some("b"), Some("h"));
        assert!(trial_merge_is_current(Mergeability::Clean, on, Some("b"), "h"));
        assert!(!trial_merge_is_current(Mergeability::Unknown, on, Some("b"), "h"));
        assert!(!trial_merge_is_current(Mergeability::Conflict, on, None, "h"));
        assert!(!trial_merge_is_current(Mergeability::Clean, on, Some("b"), "h2"));
    }

    #[test]
    fn outdated_comments_compare_each_file_at_its_commit() {
        let now = [Some(1), Some(2), None, None];
        let then = [Some(vec![Some(1), Some(3)]), None];
        let then_at = [(0, 0), (0, 1), (1, 0), (0, 0)];
        assert_eq!(outdated_comments(&now, &then, &then_at), [1, 3], "changed, unreadable-and-gone kept, deleted");
        assert_eq!((list_offset(0, 25), list_offset(1, 25), list_offset(3, 25)), (0, 0, 50));
    }
}
