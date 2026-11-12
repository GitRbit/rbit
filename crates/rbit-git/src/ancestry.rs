//! Ancestry without diffs (agent plan steps 4.5 and 4.9): whether one commit contains
//! another, and the first-parent line of commits between two, as a bisection walks it.

use gix::{revision::walk::Sorting, traverse::commit::simple::CommitTimeOrder};

use crate::{GitError, RepositoryReader, reader::resolve_commit};

/// Most commits [`RepositoryReader::first_parent_range`] returns.
pub const MAX_RANGE_COMMITS: usize = 1000;

impl RepositoryReader {
    /// Whether `ancestor` is `descendant` or one of its ancestors (each a branch, tag or
    /// commit id), as `git merge-base --is-ancestor` says.
    pub async fn is_ancestor(&self, ancestor: String, descendant: String) -> Result<bool, GitError> {
        self.with_repo(move |repo| {
            let ancestor = resolve_commit(repo, &ancestor)?;
            let descendant = resolve_commit(repo, &descendant)?;
            if ancestor == descendant {
                return Ok(true);
            }
            Ok(repo.merge_base(ancestor, descendant).map_err(GitError::backend)?.is_some_and(|base| base.detach() == ancestor))
        })
        .await
    }

    /// The commits on `bad`'s first-parent line that `good` does not contain, newest first,
    /// at most [`MAX_RANGE_COMMITS`]; and whether the line was cut there.
    pub async fn first_parent_range(&self, good: String, bad: String) -> Result<(Vec<String>, bool), GitError> {
        self.with_repo(move |repo| {
            let good = resolve_commit(repo, &good)?;
            let bad = resolve_commit(repo, &bad)?;
            let mut commits = Vec::new();
            let walk = repo
                .rev_walk([bad])
                .with_hidden([good])
                .first_parent_only()
                .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
                .all()
                .map_err(GitError::backend)?;
            for info in walk {
                if commits.len() == MAX_RANGE_COMMITS {
                    return Ok((commits, true));
                }
                commits.push(info.map_err(GitError::backend)?.id.to_hex().to_string());
            }
            Ok((commits, false))
        })
        .await
    }
}
