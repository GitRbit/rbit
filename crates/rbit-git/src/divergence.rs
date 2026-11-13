//! How far a commit is from another without diffing anything: their merge base and the
//! commits each has that the other has not (agent plan step 1.6, `rbit status`). It is the
//! cheap half of [`crate::compare`], for answers that must stay small and fast.

use gix::{ObjectId, revision::walk::Sorting, traverse::commit::simple::CommitTimeOrder};

use crate::{GitError, RepositoryReader, compare::MAX_COUNTED_COMMITS, reader::resolve_commit};

/// Where `head` stands against `base`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    pub base: String,
    pub head: String,
    /// The best common ancestor; `None` when the histories are unrelated.
    pub merge_base: Option<String>,
    /// Commits in `head` but not in `base` (at most [`MAX_COUNTED_COMMITS`]).
    pub ahead: usize,
    /// Commits in `base` but not in `head` (at most [`MAX_COUNTED_COMMITS`]).
    pub behind: usize,
}

impl RepositoryReader {
    /// Where `head` stands against `base` (branches, tags or commit ids).
    pub async fn divergence(&self, base: String, head: String) -> Result<Divergence, GitError> {
        self.with_repo(move |repo| {
            let base_id = resolve_commit(repo, &base)?;
            let head_id = resolve_commit(repo, &head)?;
            let merge_base = repo.merge_base(base_id, head_id).map_err(GitError::backend)?.map(gix::Id::detach);
            let count = |tip: ObjectId, hidden: ObjectId| -> Result<usize, GitError> {
                let mut counted = 0;
                for info in repo
                    .rev_walk([tip])
                    .with_hidden([hidden])
                    .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
                    .all()
                    .map_err(GitError::backend)?
                    .take(MAX_COUNTED_COMMITS)
                {
                    info.map_err(GitError::backend)?;
                    counted += 1;
                }
                Ok(counted)
            };
            Ok(Divergence {
                base: base_id.to_hex().to_string(),
                head: head_id.to_hex().to_string(),
                merge_base: merge_base.map(|id| id.to_hex().to_string()),
                ahead: count(head_id, base_id)?,
                behind: count(base_id, head_id)?,
            })
        })
        .await
    }
}
