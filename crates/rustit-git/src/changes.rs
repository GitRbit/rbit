//! Commit details (what a commit changed) and branch/tag listings.

use gix::{ObjectId, bstr::ByteSlice, objs::tree::EntryMode};

use crate::{
    ChangeStatus, CommitDetail, FileDiff, FileDiffBody, GitError, RefDetail, RefKind, RepositoryReader,
    diff::{diff_bytes, is_binary},
    reader::{resolve_commit, summarize},
};

/// Bounds that keep huge commits from producing huge pages.
#[derive(Debug, Clone, Copy)]
pub struct DiffLimits {
    /// Files beyond this count are omitted entirely.
    pub max_files: usize,
    /// Either side of a file larger than this is not diffed.
    pub max_file_bytes: u64,
    /// Once this many diff lines have been produced, remaining files are summarised only.
    pub max_total_lines: usize,
}

impl Default for DiffLimits {
    fn default() -> Self {
        Self { max_files: 300, max_file_bytes: 512 * 1024, max_total_lines: 20_000 }
    }
}

/// One side of a change: the blob id and its mode.
type Side = Option<(EntryMode, ObjectId)>;

impl RepositoryReader {
    /// Everything `revision` changed relative to its first parent (or the empty tree for
    /// a root commit), with renames detected.
    pub async fn commit_detail(&self, revision: String, limits: DiffLimits) -> Result<CommitDetail, GitError> {
        self.with_repo(move |repo| {
            let commit = repo.find_commit(resolve_commit(repo, &revision)?).map_err(GitError::backend)?;
            let new_tree = commit.tree().map_err(GitError::backend)?;
            let old_tree = match commit.parent_ids().next() {
                Some(parent) => Some(repo.find_commit(parent).map_err(GitError::backend)?.tree().map_err(GitError::backend)?),
                None => None,
            };

            let options = gix::diff::Options::default().with_rewrites(Some(gix::diff::Rewrites::default()));
            let changes = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), options).map_err(GitError::backend)?;

            let mut files = Vec::new();
            let mut budget = limits.max_total_lines;
            let mut truncated = false;
            for change in changes {
                let Some((path, old_path, status, old, new)) = describe(change) else { continue };
                if files.len() == limits.max_files {
                    truncated = true;
                    break;
                }
                files.push(file_diff(repo, path, old_path, status, old, new, &limits, &mut budget)?);
            }

            // Files skipped once the line budget ran out make the listing incomplete too.
            let truncated = truncated || files.iter().any(|f| !f.stats_known || matches!(f.body, FileDiffBody::TooLarge));
            let committer_name = commit.committer().map_err(GitError::backend)?.trim().name.to_str_lossy().into_owned();
            Ok(CommitDetail { commit: summarize(&commit)?, committer_name, files, truncated })
        })
        .await
    }

    /// Branches or tags with the commit each points at, most recently committed first.
    pub async fn ref_details(&self, kind: RefKind) -> Result<Vec<RefDetail>, GitError> {
        self.with_repo(move |repo| {
            let platform = repo.references().map_err(GitError::backend)?;
            let iter = match kind {
                RefKind::Branch => platform.local_branches(),
                RefKind::Tag => platform.tags(),
            }
            .map_err(GitError::backend)?;

            let mut details = Vec::new();
            for mut reference in iter.flatten() {
                let name = reference.name().shorten().to_str_lossy().into_owned();
                // Tags may point at non-commit objects; those are simply not listed.
                let Ok(commit) = reference.peel_to_commit() else { continue };
                details.push(RefDetail { name, kind, commit: summarize(&commit)? });
            }
            details.sort_by(|a, b| b.commit.authored_at.cmp(&a.commit.authored_at).then_with(|| a.name.cmp(&b.name)));
            Ok(details)
        })
        .await
    }
}

/// Normalises gitoxide's change into (path, old path, status, old side, new side).
/// Directory entries are skipped: their contents appear as individual file changes.
fn describe(change: gix::object::tree::diff::ChangeDetached) -> Option<(String, Option<String>, ChangeStatus, Side, Side)> {
    use gix::object::tree::diff::ChangeDetached as Change;

    let text = |path: &gix::bstr::BStr| path.to_str_lossy().into_owned();
    let (path, old_path, status, old, new) = match change {
        Change::Addition { location, entry_mode, id, .. } => (text(location.as_ref()), None, ChangeStatus::Added, None, Some((entry_mode, id))),
        Change::Deletion { location, entry_mode, id, .. } => (text(location.as_ref()), None, ChangeStatus::Deleted, Some((entry_mode, id)), None),
        Change::Modification { location, previous_entry_mode, previous_id, entry_mode, id } => {
            (text(location.as_ref()), None, ChangeStatus::Modified, Some((previous_entry_mode, previous_id)), Some((entry_mode, id)))
        }
        Change::Rewrite { source_location, source_entry_mode, source_id, entry_mode, id, location, copy, .. } => (
            text(location.as_ref()),
            Some(text(source_location.as_ref())),
            if copy { ChangeStatus::Copied } else { ChangeStatus::Renamed },
            Some((source_entry_mode, source_id)),
            Some((entry_mode, id)),
        ),
    };
    let is_tree = |side: &Side| side.is_some_and(|(mode, _)| mode.is_tree());
    if is_tree(&old) || is_tree(&new) {
        return None;
    }
    Some((path, old_path, status, old, new))
}

#[allow(clippy::too_many_arguments)]
fn file_diff(
    repo: &gix::Repository,
    path: String,
    old_path: Option<String>,
    status: ChangeStatus,
    old: Side,
    new: Side,
    limits: &DiffLimits,
    budget: &mut usize,
) -> Result<FileDiff, GitError> {
    // `stats_known = false`: the file was not diffed, so its line counts are unknown.
    let summary = |body, stats_known| FileDiff { path: path.clone(), old_path: old_path.clone(), status, additions: 0, deletions: 0, stats_known, body };

    let is_submodule = |side: &Side| side.is_some_and(|(mode, _)| mode.is_commit());
    if is_submodule(&old) || is_submodule(&new) {
        let id = |side: Side| side.map(|(_, id)| id.to_hex().to_string());
        return Ok(summary(FileDiffBody::Submodule { old: id(old), new: id(new) }, true));
    }
    if *budget == 0 {
        return Ok(summary(FileDiffBody::TooLarge, false));
    }

    let (Some(old_data), Some(new_data)) = (read_side(repo, old, limits)?, read_side(repo, new, limits)?) else {
        return Ok(summary(FileDiffBody::TooLarge, false));
    };
    if is_binary(&old_data) || is_binary(&new_data) {
        return Ok(summary(FileDiffBody::Binary, true));
    }

    let changes = diff_bytes(&old_data, &new_data);
    if changes.hunks.is_empty() {
        return Ok(summary(FileDiffBody::ModeOnly, true));
    }

    let mut file = FileDiff { path, old_path, status, additions: changes.additions, deletions: changes.deletions, stats_known: true, body: FileDiffBody::TooLarge };
    let lines = changes.line_count();
    if lines > *budget {
        // Keep the counts (they are already computed) but don't render a diff that would
        // exceed the page budget; nothing after this file is diffed either.
        *budget = 0;
    } else {
        *budget -= lines;
        file.body = FileDiffBody::Text(changes.hunks);
    }
    Ok(file)
}

/// Reads one side of a change: empty for a missing side, `None` when over the size limit.
fn read_side(repo: &gix::Repository, side: Side, limits: &DiffLimits) -> Result<Option<Vec<u8>>, GitError> {
    let Some((_, id)) = side else {
        return Ok(Some(Vec::new()));
    };
    if repo.find_header(id).map_err(GitError::backend)?.size() > limits.max_file_bytes {
        return Ok(None);
    }
    Ok(Some(repo.find_blob(id).map_err(GitError::backend)?.take_data()))
}
