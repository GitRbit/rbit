//! Commit details (what a commit changed) and branch/tag listings.

use std::ops::ControlFlow;

use gix::{ObjectId, bstr::ByteSlice, objs::tree::EntryMode};

use crate::{
    ChangeStatus, CommitDetail, FileDiff, FileDiffBody, GitError, RefDetail, RefKind, RepositoryReader,
    diff::{diff_bytes_bounded, is_binary},
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

            let (files, truncated) = diff_trees(repo, old_tree.as_ref(), &new_tree, limits)?;
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

/// Every file that differs between `old_tree` (the empty tree when `None`) and
/// `new_tree`, with renames detected, bounded by `limits`. The flag is true when files
/// were omitted or not diffed.
pub(crate) fn diff_trees(
    repo: &gix::Repository,
    old_tree: Option<&gix::Tree<'_>>,
    new_tree: &gix::Tree<'_>,
    limits: DiffLimits,
) -> Result<(Vec<FileDiff>, bool), GitError> {
    let (changes, omitted) = bounded_changes(repo, old_tree, new_tree, limits.max_files)?;

    let mut files = Vec::new();
    let mut budget = limits.max_total_lines;
    for change in changes {
        let Some((path, old_path, status, old, new)) = describe(change) else { continue };
        files.push(file_diff(repo, path, old_path, status, old, new, &limits, &mut budget)?);
    }

    // Files skipped once the line budget ran out make the listing incomplete too.
    let truncated = omitted || files.iter().any(|f| !f.stats_known || matches!(f.body, FileDiffBody::TooLarge));
    Ok((files, truncated))
}

/// Retain only the visible prefix of classified changes, stopping at the first omitted
/// file. Keep gitoxide's rewrite settings and emission order: rename detection can still
/// require discovering all candidates, but modifications can fill the page before that
/// work is needed. Directory changes do not consume the file budget.
fn bounded_changes(
    repo: &gix::Repository,
    old_tree: Option<&gix::Tree<'_>>,
    new_tree: &gix::Tree<'_>,
    max_files: usize,
) -> Result<(Vec<gix::object::tree::diff::ChangeDetached>, bool), GitError> {
    let mut cache = repo
        .diff_resource_cache(gix::diff::blob::pipeline::Mode::ToGit, gix::diff::blob::pipeline::WorktreeRoots::default())
        .map_err(GitError::backend)?;
    // With no output requested, only the existence of a changed file matters; its
    // rename classification and ordering cannot affect the empty result.
    let rewrites = (max_files != 0).then(gix::diff::Rewrites::default);
    let options = gix::diff::Options::default().with_rewrites(rewrites);
    let empty_tree = repo.empty_tree();
    let old_tree = old_tree.unwrap_or(&empty_tree);
    let mut changes = Vec::new();
    let mut omitted = false;
    let result = gix::diff::tree_with_rewrites(
        gix::objs::TreeRefIter::from_bytes(&old_tree.data, old_tree.id.kind()),
        gix::objs::TreeRefIter::from_bytes(&new_tree.data, new_tree.id.kind()),
        &mut cache,
        &mut gix::diff::tree::State::default(),
        &repo.objects,
        |change| {
            // Rewrite tracking can invoke the callback again in a later phase after
            // Break. Never retain or inspect further changes once the limit is known.
            if omitted {
                return Ok(ControlFlow::Break(()));
            }
            if change.entry_mode().is_tree() || change.source_entry_mode_and_id().0.is_tree() {
                return Ok(ControlFlow::Continue(()));
            }
            if changes.len() == max_files {
                omitted = true;
                return Ok(ControlFlow::Break(()));
            }
            changes.push(change.into_owned());
            Ok(ControlFlow::Continue(()))
        },
        options.into(),
    );
    match result {
        Ok(_) => {}
        Err(gix::diff::tree::Error::Cancelled) if omitted => {}
        Err(error) => return Err(GitError::backend(error)),
    }
    Ok((changes, omitted))
}

/// Normalises gitoxide's change into (path, old path, status, old side, new side).
/// Directory entries are skipped: their contents appear as individual file changes.
fn describe(change: gix::object::tree::diff::ChangeDetached) -> Option<(String, Option<String>, ChangeStatus, Side, Side)> {
    use gix::object::tree::diff::ChangeDetached as Change;

    let text = |path: &gix::bstr::BStr| path.to_str_lossy().into_owned();
    let (path, old_path, status, old, new) = match change {
        Change::Addition { location, entry_mode, id, .. } => {
            (text(location.as_ref()), None, ChangeStatus::Added, None, Some((entry_mode, id)))
        }
        Change::Deletion { location, entry_mode, id, .. } => {
            (text(location.as_ref()), None, ChangeStatus::Deleted, Some((entry_mode, id)), None)
        }
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
    let summary = |body, stats_known| FileDiff {
        path: path.clone(),
        old_path: old_path.clone(),
        status,
        additions: 0,
        deletions: 0,
        stats_known,
        body,
    };

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

    let (changes, over_budget) = diff_bytes_bounded(&old_data, &new_data, *budget);
    if !over_budget && changes.hunks.is_empty() {
        return Ok(summary(FileDiffBody::ModeOnly, true));
    }

    let mut file = FileDiff {
        path,
        old_path,
        status,
        additions: changes.additions,
        deletions: changes.deletions,
        stats_known: true,
        body: FileDiffBody::TooLarge,
    };
    if over_budget {
        // Keep exact counts without allocating display lines that cannot be shown;
        // nothing after this file is diffed either.
        *budget = 0;
    } else {
        *budget -= changes.line_count();
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

#[cfg(test)]
mod tests {
    use gix::objs::tree::EntryKind;

    use super::*;

    fn tree<'a>(repo: &'a gix::Repository, entries: &[(&str, EntryKind, ObjectId)]) -> gix::Tree<'a> {
        let mut entries = entries
            .iter()
            .map(|(name, kind, id)| gix::objs::tree::Entry { mode: (*kind).into(), filename: (*name).into(), oid: *id })
            .collect::<Vec<_>>();
        entries.sort();
        let id = repo.write_object(gix::objs::Tree { entries }).unwrap();
        repo.find_tree(id).unwrap()
    }

    #[test]
    fn bounded_changes_keep_the_full_diff_prefix_including_renames() {
        let dir = tempfile::tempdir().unwrap();
        let repo = gix::init_bare(dir.path()).unwrap();
        let blob = |text| repo.write_blob(text).unwrap().detach();
        let before = blob("one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n");
        let after = blob("one\ntwo\nthree\nfour\nfive\nsix\nseven\nEIGHT\n");
        let exact = blob("another file with entirely distinct content\n");
        let nested = tree(&repo, &[("inside.txt", EntryKind::Blob, blob("nested file\n"))]);
        let old = tree(
            &repo,
            &[
                ("edit.txt", EntryKind::Blob, blob("old\n")),
                ("exact-before.txt", EntryKind::Blob, exact),
                ("fuzzy-before.txt", EntryKind::Blob, before),
                ("old-dir", EntryKind::Tree, nested.id),
            ],
        );
        let new = tree(
            &repo,
            &[
                ("edit.txt", EntryKind::Blob, blob("new\n")),
                ("exact-after.txt", EntryKind::Blob, exact),
                ("fuzzy-after.txt", EntryKind::Blob, after),
                ("new-dir", EntryKind::Tree, nested.id),
                ("unrelated.txt", EntryKind::Blob, blob("unrelated addition\n")),
            ],
        );
        // The previous eager implementation is the reference for classification and
        // ordering, including directory moves and similarity-based rename matching.
        let options = gix::diff::Options::default().with_rewrites(Some(gix::diff::Rewrites::default()));
        let expected = repo
            .diff_tree_to_tree(Some(&old), Some(&new), options)
            .unwrap()
            .into_iter()
            .filter(|change| describe(change.clone()).is_some())
            .collect::<Vec<_>>();
        for path in ["exact-after.txt", "fuzzy-after.txt", "new-dir/inside.txt"] {
            assert!(
                expected.iter().any(|change| {
                    describe(change.clone()).is_some_and(|(name, _, status, _, _)| name == path && status == ChangeStatus::Renamed)
                }),
                "{path}: {expected:?}"
            );
        }
        for limit in 0..=expected.len() + 1 {
            let (actual, omitted) = bounded_changes(&repo, Some(&old), &new, limit).unwrap();
            assert_eq!(actual, expected[..limit.min(expected.len())], "file limit {limit}");
            assert_eq!(omitted, limit < expected.len());
        }
    }

    #[test]
    fn file_limits_stop_before_an_unneeded_subtree_is_loaded() {
        let dir = tempfile::tempdir().unwrap();
        let repo = gix::init_bare(dir.path()).unwrap();
        let before = repo.write_blob("old\n").unwrap().detach();
        let after = repo.write_blob("new\n").unwrap().detach();
        let missing = ObjectId::from_hex(b"ffffffffffffffffffffffffffffffffffffffff").unwrap();
        let old = tree(&repo, &[("a.txt", EntryKind::Blob, before), ("b.txt", EntryKind::Blob, before)]);
        let new =
            tree(&repo, &[("a.txt", EntryKind::Blob, after), ("b.txt", EntryKind::Blob, after), ("z-late", EntryKind::Tree, missing)]);
        let limits = DiffLimits { max_files: 1, ..DiffLimits::default() };
        let (files, omitted) = diff_trees(&repo, Some(&old), &new, limits).unwrap();
        assert!(omitted);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "a.txt");
        assert_eq!((files[0].additions, files[0].deletions), (1, 1));
        // No timing assertion: the missing tree makes any unnecessary traversal fail.
        assert!(bounded_changes(&repo, Some(&old), &new, usize::MAX).is_err());
        let (files, omitted) = bounded_changes(&repo, None, &new, 0).unwrap();
        assert!(files.is_empty() && omitted, "a zero-file budget needs no rename discovery");
        let (files, omitted) = bounded_changes(&repo, Some(&old), &old, 0).unwrap();
        assert!(files.is_empty() && !omitted, "an unchanged tree is not truncated");
    }
}
