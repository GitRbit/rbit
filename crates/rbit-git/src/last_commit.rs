//! The most recent commit that changed each entry of a directory (the "last commit"
//! column of a file listing).
//!
//! History is walked newest-first from the revision. A commit whose copy of the directory
//! is identical to one of its parents' changed nothing here and is skipped after a single
//! tree-id comparison. Otherwise each still-unresolved entry is attributed to the commit
//! when its object id differs from every parent's — so an entry brought in by a merge is
//! credited to the side-branch commit that actually changed it, as `git log` does.
//!
//! The walk is bounded by a commit count and a deadline; entries still unresolved when
//! either runs out are simply absent from the result.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use gix::{ObjectId, bstr::ByteSlice, revision::walk::Sorting, traverse::commit::simple::CommitTimeOrder};

use crate::{
    CommitSummary, GitError, RepositoryReader,
    reader::{resolve_commit, summarize},
};

/// Bounds on the history walk.
#[derive(Debug, Clone, Copy)]
pub struct LastCommitLimits {
    pub max_commits: usize,
    pub deadline: Duration,
}

impl Default for LastCommitLimits {
    fn default() -> Self {
        Self { max_commits: 5_000, deadline: Duration::from_millis(1_500) }
    }
}

/// The outcome of a last-commit walk.
#[derive(Debug, Clone, Default)]
pub struct LastCommits {
    /// Entry name → the newest commit that changed it.
    pub commits: HashMap<String, CommitSummary>,
    /// The deadline cut the walk short. Such a result depends on machine load, so callers
    /// must not persist it as complete; short-lived reuse is safe. A result bounded
    /// by the commit count is deterministic.
    pub timed_out: bool,
}

impl RepositoryReader {
    /// Finds the last commit for each of `entry_names` inside `directory` (empty for the root).
    pub async fn last_commits(
        &self,
        revision: String,
        directory: String,
        entry_names: Vec<String>,
        limits: LastCommitLimits,
    ) -> Result<LastCommits, GitError> {
        self.with_repo(move |repo| {
            let started = Instant::now();
            let tip = resolve_commit(repo, &revision)?;
            let mut walk =
                HistoryWalk { repo, directory: &directory, directory_trees: HashMap::new(), entry_trees: VecDeque::new(), entry_bytes: 0 };
            let mut unresolved: HashSet<String> = entry_names.into_iter().collect();
            let mut result = LastCommits::default();

            let commits =
                repo.rev_walk([tip]).sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst)).all().map_err(GitError::backend)?;
            for (visited, info) in commits.enumerate() {
                if unresolved.is_empty() || visited == limits.max_commits {
                    break;
                }
                if started.elapsed() >= limits.deadline {
                    result.timed_out = true;
                    break;
                }
                let info = info.map_err(GitError::backend)?;
                let parents: Vec<ObjectId> = info.parent_ids().map(gix::Id::detach).collect();
                let changed = walk.changed_entries(info.id, &parents, &unresolved)?;
                if changed.is_empty() {
                    continue;
                }
                let summary = summarize(&repo.find_commit(info.id).map_err(GitError::backend)?)?;
                for name in changed {
                    unresolved.remove(&name);
                    result.commits.insert(name, summary.clone());
                }
            }
            Ok(result)
        })
        .await
    }
}

type TreeEntries = Arc<HashMap<String, ObjectId>>;
const TREE_CACHE_BYTES: usize = 4 * 1024 * 1024;

struct HistoryWalk<'a> {
    repo: &'a gix::Repository,
    directory: &'a str,
    /// Commit → the directory's tree id in it (`None` when absent). Children are visited
    /// before parents, so each parent's lookup is reused when the walk reaches it.
    directory_trees: HashMap<ObjectId, Option<ObjectId>>,
    entry_trees: VecDeque<(ObjectId, TreeEntries, usize)>,
    entry_bytes: usize,
}

impl HistoryWalk<'_> {
    /// The unresolved entries whose object id in `commit` differs from every parent's.
    fn changed_entries(&mut self, commit: ObjectId, parents: &[ObjectId], unresolved: &HashSet<String>) -> Result<Vec<String>, GitError> {
        let Some(tree) = self.directory_tree(commit)? else {
            return Ok(Vec::new());
        };
        let mut parent_trees = Vec::with_capacity(parents.len());
        for parent in parents {
            let parent_tree = self.directory_tree(*parent)?;
            if parent_tree == Some(tree) {
                // Identical to a parent: nothing in this directory changed here.
                return Ok(Vec::new());
            }
            parent_trees.push(parent_tree);
        }

        let current = self.entries(Some(tree))?;
        let previous = parent_trees.into_iter().map(|t| self.entries(t)).collect::<Result<Vec<_>, _>>()?;
        Ok(unresolved
            .iter()
            .filter(|name| current.get(*name).is_some_and(|id| previous.iter().all(|parent| parent.get(*name) != Some(id))))
            .cloned()
            .collect())
    }

    fn directory_tree(&mut self, commit: ObjectId) -> Result<Option<ObjectId>, GitError> {
        if let Some(cached) = self.directory_trees.get(&commit) {
            return Ok(*cached);
        }
        let root = self.repo.find_commit(commit).map_err(GitError::backend)?.tree_id().map_err(GitError::backend)?.detach();
        let tree = if self.directory.is_empty() {
            Some(root)
        } else {
            self.repo
                .find_tree(root)
                .map_err(GitError::backend)?
                .lookup_entry(self.directory.split('/'))
                .map_err(GitError::backend)?
                .filter(|entry| entry.mode().is_tree())
                .map(|entry| entry.object_id())
        };
        self.directory_trees.insert(commit, tree);
        Ok(tree)
    }

    /// Entry name → object id of a directory tree; empty when the directory is absent.
    fn entries(&mut self, tree: Option<ObjectId>) -> Result<TreeEntries, GitError> {
        let Some(tree) = tree else {
            return Ok(Arc::default());
        };
        if let Some(index) = self.entry_trees.iter().position(|(id, _, _)| *id == tree) {
            let entry = self.entry_trees.remove(index).expect("known tree");
            let result = Arc::clone(&entry.1);
            self.entry_trees.push_back(entry);
            return Ok(result);
        }
        let id = tree;
        let tree = self.repo.find_tree(tree).map_err(GitError::backend)?;
        let decoded = tree.decode().map_err(GitError::backend)?;
        let entries: HashMap<_, _> =
            decoded.entries.iter().map(|entry| (entry.filename.to_str_lossy().into_owned(), entry.oid.to_owned())).collect();
        let bytes =
            entries.capacity() * (std::mem::size_of::<(String, ObjectId)>() + 8) + entries.keys().map(String::capacity).sum::<usize>();
        let entries = Arc::new(entries);
        if bytes <= TREE_CACHE_BYTES {
            while self.entry_trees.len() >= 16 || self.entry_bytes + bytes > TREE_CACHE_BYTES {
                if let Some((_, _, removed)) = self.entry_trees.pop_front() {
                    self.entry_bytes -= removed;
                }
            }
            self.entry_bytes += bytes;
            self.entry_trees.push_back((id, Arc::clone(&entries), bytes));
        }
        Ok(entries)
    }
}
