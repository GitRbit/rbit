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
        self.with_repo(move |repo| Ok(walk_last_commits(repo, &revision, &directory, entry_names, limits)?.0)).await
    }
}

/// The walk behind [`RepositoryReader::last_commits`]; also returns how many commit
/// objects were decoded just to find a root tree.
fn walk_last_commits(
    repo: &gix::Repository,
    revision: &str,
    directory: &str,
    entry_names: Vec<String>,
    limits: LastCommitLimits,
) -> Result<(LastCommits, usize), GitError> {
    let started = Instant::now();
    let tip = resolve_commit(repo, revision)?;
    let mut walk = HistoryWalk::new(repo, directory);
    let mut unresolved: HashSet<String> = entry_names.into_iter().collect();
    let mut result = LastCommits::default();

    let commits = repo.rev_walk([tip]).sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst)).all().map_err(GitError::backend)?;
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
    Ok((result, walk.commits_decoded))
}

type TreeEntries = Arc<HashMap<String, ObjectId>>;
const TREE_CACHE_BYTES: usize = 4 * 1024 * 1024;

struct HistoryWalk<'a> {
    repo: &'a gix::Repository,
    directory: &'a str,
    /// The repository's commit-graph, when it has one: root tree ids of the commits it
    /// covers are read from it instead of decoding each commit object. Commits newer than
    /// the graph fall back to the object database.
    graph: Option<gix::commitgraph::Graph>,
    /// Commit objects decoded to find a root tree (those the graph did not answer).
    commits_decoded: usize,
    /// Commit → the directory's tree id in it (`None` when absent). Children are visited
    /// before parents, so each parent's lookup is reused when the walk reaches it.
    directory_trees: HashMap<ObjectId, Option<ObjectId>>,
    entry_trees: VecDeque<(ObjectId, TreeEntries, usize)>,
    entry_bytes: usize,
}

impl<'a> HistoryWalk<'a> {
    fn new(repo: &'a gix::Repository, directory: &'a str) -> Self {
        // A missing, disabled or unreadable graph only costs speed.
        let graph = repo.commit_graph_if_enabled().ok().flatten();
        Self { repo, directory, graph, commits_decoded: 0, directory_trees: HashMap::new(), entry_trees: VecDeque::new(), entry_bytes: 0 }
    }

    /// The root tree of `commit`, from the commit-graph when it covers the commit.
    fn root_tree(&mut self, commit: ObjectId) -> Result<ObjectId, GitError> {
        if let Some(tree) = self.graph.as_ref().and_then(|graph| graph.commit_by_id(commit)).map(|entry| entry.root_tree_id().to_owned()) {
            return Ok(tree);
        }
        self.commits_decoded += 1;
        Ok(self.repo.find_commit(commit).map_err(GitError::backend)?.tree_id().map_err(GitError::backend)?.detach())
    }

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
        let root = self.root_tree(commit)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &std::path::Path, args: &[&str], date: i64) {
        let date = format!("{date} +0000");
        let status = std::process::Command::new("git")
            .current_dir(dir)
            .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main", "-c", "core.autocrlf=false"])
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    type Walked = (Vec<(String, String)>, usize);

    fn walk(root: &std::path::Path, directory: &str, names: &[&str]) -> Walked {
        let repo = gix::open_opts(root.join(".git"), gix::open::Options::isolated()).unwrap();
        let names = names.iter().map(|name| (*name).to_owned()).collect();
        let limits = LastCommitLimits { max_commits: 100, deadline: Duration::from_mins(1) };
        let (result, decoded) = walk_last_commits(&repo, "main", directory, names, limits).unwrap();
        let mut summaries: Vec<_> = result.commits.into_iter().map(|(name, commit)| (name, commit.summary)).collect();
        summaries.sort();
        (summaries, decoded)
    }

    #[test]
    fn a_commit_graph_answers_root_trees_without_changing_results() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut clock = 1_700_000_000;
        let mut run = |args: &[&str]| {
            clock += 60;
            git(root, args, clock);
        };
        run(&["init", "-q"]);
        std::fs::create_dir_all(root.join("src")).unwrap();
        for (file, message) in [("src/a.rs", "add a"), ("src/b.rs", "add b"), ("README.md", "readme"), ("src/a.rs", "edit a")] {
            std::fs::write(root.join(file), message).unwrap();
            run(&["add", "-A"]);
            run(&["commit", "-q", "-m", message]);
        }

        let (root_before, root_decoded) = walk(root, "", &["src", "README.md"]);
        let (src_before, src_decoded) = walk(root, "src", &["a.rs", "b.rs"]);
        assert!(root_decoded > 0 && src_decoded > 0, "without a graph every visited commit is decoded");

        run(&["commit-graph", "write", "--reachable", "--split"]);
        assert_eq!(walk(root, "", &["src", "README.md"]), (root_before, 0), "same answer, no commit decoded for its tree");
        assert_eq!(walk(root, "src", &["a.rs", "b.rs"]), (src_before.clone(), 0));

        // A commit made after the graph was written falls back to its object, and only it.
        std::fs::write(root.join("src/b.rs"), "edit b").unwrap();
        run(&["commit", "-q", "-am", "edit b"]);
        let (src_after, decoded) = walk(root, "src", &["a.rs", "b.rs"]);
        assert_eq!(decoded, 1);
        assert_eq!(src_after, [("a.rs".to_owned(), "edit a".to_owned()), ("b.rs".to_owned(), "edit b".to_owned())]);
        assert_eq!(src_before[0], src_after[0]);
    }
}
