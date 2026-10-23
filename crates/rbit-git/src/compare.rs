//! Comparing two revisions: merge base, commits ahead and behind, and the diff.
//!
//! Milestone 2 workstream R (repository essentials) owns this file; wave B's pull request
//! workstream (P) adds ancestry and path lookups.
//!
//! The comparison answers "what would merging `head` into `base` bring in": the commits
//! reachable from `head` but not from `base`, and the diff from their merge base to
//! `head` (so changes made on `base` meanwhile do not show up as reversions).

use gix::{ObjectId, revision::walk::Sorting, traverse::commit::simple::CommitTimeOrder};

use crate::{
    CommitSummary, DiffLimits, FileDiff, GitError, RepositoryReader,
    changes::diff_trees,
    reader::{resolve_commit, summarize},
};

/// Commits counted at most when computing ahead/behind; beyond it the count is a lower bound.
pub const MAX_COUNTED_COMMITS: usize = 10_000;

#[derive(Debug, Clone)]
pub struct Comparison {
    pub base: CommitSummary,
    pub head: CommitSummary,
    /// The best common ancestor; `None` when the histories are unrelated.
    pub merge_base: Option<String>,
    /// Commits in `head` but not in `base` (at most [`MAX_COUNTED_COMMITS`]).
    pub ahead: usize,
    /// Commits in `base` but not in `head` (at most [`MAX_COUNTED_COMMITS`]). Always 0 from
    /// [`RepositoryReader::compare_ahead`], which does not count them.
    pub behind: usize,
    /// The newest of the commits ahead, newest first.
    pub commits: Vec<CommitSummary>,
    /// What `head` changed since the merge base; empty for unrelated histories.
    pub files: Vec<FileDiff>,
    /// Some files were omitted, or not diffed, because the comparison is very large.
    pub truncated: bool,
}

impl Comparison {
    pub fn additions(&self) -> usize {
        self.files.iter().map(|f| f.additions).sum()
    }

    pub fn deletions(&self) -> usize {
        self.files.iter().map(|f| f.deletions).sum()
    }
}

impl RepositoryReader {
    /// Compares `head` against `base` (branches, tags or commit ids), listing at most
    /// `max_commits` of the commits ahead.
    pub async fn compare(&self, base: String, head: String, limits: DiffLimits, max_commits: usize) -> Result<Comparison, GitError> {
        self.compare_counting(base, head, limits, max_commits, true).await
    }

    /// [`Self::compare`] without counting the commits `head` is behind `base` (left 0):
    /// pull requests show only what the head brings in, and on a long-lived base the
    /// behind walk can visit far more history than the ahead walk.
    pub async fn compare_ahead(&self, base: String, head: String, limits: DiffLimits, max_commits: usize) -> Result<Comparison, GitError> {
        self.compare_counting(base, head, limits, max_commits, false).await
    }

    async fn compare_counting(
        &self,
        base: String,
        head: String,
        limits: DiffLimits,
        max_commits: usize,
        count_behind: bool,
    ) -> Result<Comparison, GitError> {
        self.with_repo(move |repo| {
            let base_id = resolve_commit(repo, &base)?;
            let head_id = resolve_commit(repo, &head)?;
            let merge_base = repo.merge_base(base_id, head_id).map_err(GitError::backend)?.map(gix::Id::detach);

            let only_in = |tip: ObjectId, hidden: ObjectId| -> Result<Vec<ObjectId>, GitError> {
                repo.rev_walk([tip])
                    .with_hidden([hidden])
                    .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
                    .all()
                    .map_err(GitError::backend)?
                    .take(MAX_COUNTED_COMMITS)
                    .map(|info| info.map(|info| info.id).map_err(GitError::backend))
                    .collect()
            };
            let ahead = only_in(head_id, base_id)?;
            let behind = if count_behind { only_in(base_id, head_id)?.len() } else { 0 };
            let commits = ahead
                .iter()
                .take(max_commits)
                .map(|id| summarize(&repo.find_commit(*id).map_err(GitError::backend)?))
                .collect::<Result<Vec<_>, _>>()?;

            let head_commit = repo.find_commit(head_id).map_err(GitError::backend)?;
            let (files, truncated) = match merge_base {
                Some(merge_base) if merge_base != head_id => {
                    let old_tree = repo.find_commit(merge_base).map_err(GitError::backend)?.tree().map_err(GitError::backend)?;
                    let new_tree = head_commit.tree().map_err(GitError::backend)?;
                    diff_trees(repo, Some(&old_tree), &new_tree, limits)?
                }
                _ => (Vec::new(), false),
            };

            Ok(Comparison {
                base: summarize(&repo.find_commit(base_id).map_err(GitError::backend)?)?,
                head: summarize(&head_commit)?,
                merge_base: merge_base.map(|id| id.to_hex().to_string()),
                ahead: ahead.len(),
                behind,
                commits,
                files,
                truncated,
            })
        })
        .await
    }

    /// Whether `ancestor` is reachable from `descendant` (a commit is its own ancestor).
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

    /// The object id of each of `paths` as of `revision` (`None` where the path does not
    /// exist), e.g. to tell whether a file changed between two commits.
    pub async fn path_ids(&self, revision: String, paths: Vec<String>) -> Result<Vec<Option<String>>, GitError> {
        self.with_repo(move |repo| path_ids_in(repo, &revision, &paths)).await
    }

    /// [`Self::path_ids`] for several revisions in one opening of the repository: one
    /// answer per `(revision, paths)` request, in order, `None` where that revision cannot
    /// be read (e.g. a commit that no longer exists).
    pub async fn path_ids_at(&self, requests: Vec<(String, Vec<String>)>) -> Result<Vec<Option<Vec<Option<String>>>>, GitError> {
        self.with_repo(move |repo| Ok(requests.iter().map(|(revision, paths)| path_ids_in(repo, revision, paths).ok()).collect())).await
    }
}

fn path_ids_in(repo: &gix::Repository, revision: &str, paths: &[String]) -> Result<Vec<Option<String>>, GitError> {
    let tree = repo.find_commit(resolve_commit(repo, revision)?).map_err(GitError::backend)?.tree().map_err(GitError::backend)?;
    paths
        .iter()
        .map(|path| Ok(tree.lookup_entry_by_path(path).map_err(GitError::backend)?.map(|entry| entry.object_id().to_hex().to_string())))
        .collect()
}
