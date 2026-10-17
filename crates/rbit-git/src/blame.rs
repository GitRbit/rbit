//! Line-by-line blame of a file at a revision.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! Built on gitoxide's `blame`, which walks history and diffs each version of the file.
//! Its cost grows with the file's size and history, so a blame is bounded twice: files
//! larger than [`BlameLimits::max_file_bytes`] are not blamed at all, and a blame that
//! takes longer than [`BlameLimits::time_budget`] is abandoned. Either way the caller
//! gets [`Blame::TooLarge`] and can show a notice instead.

use std::{
    collections::{HashMap, hash_map::Entry},
    time::Duration,
};

use gix::{ObjectId, bstr::ByteSlice};

use crate::{
    CommitSummary, GitError, RepositoryReader,
    diff::is_binary,
    reader::{resolve_commit, summarize},
};

/// Bounds for one blame.
#[derive(Debug, Clone, Copy)]
pub struct BlameLimits {
    pub max_file_bytes: u64,
    pub time_budget: Duration,
}

impl Default for BlameLimits {
    fn default() -> Self {
        Self { max_file_bytes: 512 * 1024, time_budget: Duration::from_secs(5) }
    }
}

/// Consecutive lines introduced by one commit.
#[derive(Debug, Clone)]
pub struct BlameHunk {
    /// The commit that introduced the lines.
    pub commit: CommitSummary,
    /// First line of the hunk in the blamed file, 1-based.
    pub start_line: usize,
    /// Number of lines (at least 1).
    pub line_count: usize,
    /// The file's path in `commit`, when it differs (the file was renamed since).
    pub source_path: Option<String>,
}

/// A blamed file: its text and which commit introduced each run of lines.
#[derive(Debug, Clone)]
pub struct FileBlame {
    /// The commit the revision resolved to.
    pub commit: CommitSummary,
    /// The file's contents at that commit (lossily decoded).
    pub text: String,
    /// In file order, covering every line exactly once.
    pub hunks: Vec<BlameHunk>,
}

/// What a blame produced.
#[derive(Debug, Clone)]
pub enum Blame {
    Blamed(Box<FileBlame>),
    /// Binary files are not blamed.
    Binary,
    /// The file exceeds the size limit, or blaming it exceeded the time budget.
    TooLarge,
}

impl RepositoryReader {
    /// Blames the file at `path` as of `revision`.
    pub async fn blame(&self, revision: String, path: String, limits: BlameLimits) -> Result<Blame, GitError> {
        let work = self.with_repo(move |repo| {
            let commit = repo.find_commit(resolve_commit(repo, &revision)?).map_err(GitError::backend)?;
            let entry = commit
                .tree()
                .map_err(GitError::backend)?
                .lookup_entry_by_path(&path)
                .map_err(GitError::backend)?
                .ok_or(GitError::NotFound("file"))?;
            if !entry.mode().is_blob() && !entry.mode().is_link() {
                return Err(GitError::NotFound("file"));
            }
            if repo.find_header(entry.object_id()).map_err(GitError::backend)?.size() > limits.max_file_bytes {
                return Ok(Blame::TooLarge);
            }
            let data = repo.find_blob(entry.object_id()).map_err(GitError::backend)?.take_data();
            if is_binary(&data) {
                return Ok(Blame::Binary);
            }

            let options = gix::repository::blame_file::Options { rewrites: Some(gix::diff::Rewrites::default()), ..Default::default() };
            let outcome = repo.blame_file(path.as_bytes().as_bstr(), commit.id, options).map_err(GitError::backend)?;

            let mut commits: HashMap<ObjectId, CommitSummary> = HashMap::new();
            let mut hunks = Vec::with_capacity(outcome.entries.len());
            for entry in outcome.entries {
                let summary = match commits.entry(entry.commit_id) {
                    Entry::Occupied(known) => known.get().clone(),
                    Entry::Vacant(slot) => slot.insert(summarize(&repo.find_commit(entry.commit_id).map_err(GitError::backend)?)?).clone(),
                };
                hunks.push(BlameHunk {
                    commit: summary,
                    start_line: usize::try_from(entry.start_in_blamed_file).unwrap_or(usize::MAX).saturating_add(1),
                    line_count: usize::try_from(entry.len.get()).unwrap_or(usize::MAX),
                    source_path: entry.source_file_name.map(|name| name.to_str_lossy().into_owned()).filter(|name| *name != path),
                });
            }
            Ok(Blame::Blamed(Box::new(FileBlame { commit: summarize(&commit)?, text: String::from_utf8_lossy(&data).into_owned(), hunks })))
        });
        match tokio::time::timeout(limits.time_budget, work).await {
            Ok(result) => result,
            // The blocking task finishes on its own; its result is simply discarded.
            Err(_) => Ok(Blame::TooLarge),
        }
    }
}
