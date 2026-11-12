//! The first-parent line of commits between two, as a bisection walks it (agent plan
//! step 4.9). Whether one commit contains another is [`RepositoryReader::is_ancestor`].

use gix::{revision::walk::Sorting, traverse::commit::simple::CommitTimeOrder};

use crate::{GitError, RepositoryReader, reader::resolve_commit};

/// Most commits [`RepositoryReader::first_parent_range`] returns.
pub const MAX_RANGE_COMMITS: usize = 1000;

/// The commits on `bad`'s first-parent line that `good` does not contain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstParentRange {
    /// Newest first, at most [`MAX_RANGE_COMMITS`].
    pub commits: Vec<String>,
    /// The line was cut at [`MAX_RANGE_COMMITS`].
    pub truncated: bool,
    /// The line ends on `good`: the oldest commit's first parent is `good`. False when
    /// `good` is on a side branch merged into the line, is not an ancestor of `bad` at all,
    /// or is `bad`; true when the line was cut, as that cannot be told without walking on.
    pub reaches_good: bool,
}

impl RepositoryReader {
    /// The commits on `bad`'s first-parent line that `good` does not contain (see
    /// [`FirstParentRange`]).
    pub async fn first_parent_range(&self, good: String, bad: String) -> Result<FirstParentRange, GitError> {
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
            let mut oldest = None;
            for info in walk {
                if commits.len() == MAX_RANGE_COMMITS {
                    return Ok(FirstParentRange { commits, truncated: true, reaches_good: true });
                }
                let id = info.map_err(GitError::backend)?.id;
                oldest = Some(id);
                commits.push(id.to_hex().to_string());
            }
            let reaches_good = match oldest {
                Some(id) => {
                    let commit = repo.find_commit(id).map_err(GitError::backend)?;
                    commit.parent_ids().next().is_some_and(|parent| parent.detach() == good)
                }
                None => false,
            };
            Ok(FirstParentRange { commits, truncated: false, reaches_good })
        })
        .await
    }
}
