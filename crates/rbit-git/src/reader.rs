//! Read-only access to one bare repository.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

use gix::{
    ObjectId, bstr::ByteSlice, objs::tree::EntryKind as GixEntryKind, revision::walk::Sorting, traverse::commit::simple::CommitTimeOrder,
};
use rbit_core::Timestamp;

use crate::{
    BlobView, CommitSummary, EntryKind, GitError, RefKind, RefSummary, TreeEntry, TreeView,
    diff::is_binary,
    lease::{RepositoryLease, RepositoryLock},
};

/// File names recognised as a directory's README, in priority order.
const README_NAMES: &[&str] = &["readme.md", "readme.markdown", "readme", "readme.txt", "readme.rst"];

/// A handle to a repository on disk. Cheap to create; by default each operation opens
/// the repository on the blocking pool, which is fast thanks to gitoxide's lazy loading.
///
/// A web page makes several calls (refs, tree, README, language statistics, …);
/// [`Self::pinned`] lets them share one opened repository, so configuration, the
/// reference store and pack indices are loaded once per page instead of once per call.
///
/// Readers from [`crate::RepositoryStorage::reader`] also hold the repository's
/// [lease](crate::lease) while they have it open — for each operation, or for a pinned
/// reader as long as its opened repository lives — so a transfer waits for them instead
/// of failing on Windows.
#[derive(Debug, Clone)]
pub struct RepositoryReader {
    path: PathBuf,
    /// The repository's lock, for readers that take leases.
    lock: Option<Arc<RepositoryLock>>,
    /// The repository opened by the first operation of a pinned reader, shared by all
    /// its clones. `None` for an ordinary reader.
    pinned: Option<Arc<Mutex<Option<Pinned>>>>,
}

/// A pinned reader's opened repository and the lease that keeps it in place. Fields drop
/// in order: the repository's files are closed before the lease is released.
#[derive(Debug)]
struct Pinned {
    repo: gix::ThreadSafeRepository,
    _lease: Option<RepositoryLease>,
}

impl RepositoryReader {
    /// A reader for the repository at `path` that takes no lease (for repositories that
    /// are never moved, such as test fixtures).
    pub fn new(path: PathBuf) -> Self {
        Self { path, lock: None, pinned: None }
    }

    pub(crate) fn with_lock(mut self, lock: Arc<RepositoryLock>) -> Self {
        self.lock = Some(lock);
        self
    }

    /// A reader whose operations (and those of its clones) share one opened repository.
    ///
    /// Meant to live for one request: an open repository keeps pack files mapped, and on
    /// Windows a mapped file cannot be deleted, so a long-lived pinned reader would get in
    /// the way of `git gc` and repository deletion. Objects and references written
    /// meanwhile are still seen: gitoxide re-scans the packs when an object is missing
    /// and reads references from disk on every lookup. Its lease (if any) is held from
    /// the first operation until the last clone is dropped, so the repository cannot be
    /// moved during the request.
    #[must_use]
    pub fn pinned(&self) -> Self {
        Self { path: self.path.clone(), lock: self.lock.clone(), pinned: Some(Arc::default()) }
    }

    /// Closes a pinned reader's shared repository and releases its lease now, for every
    /// clone; the next operation opens it again. Call it before moving the repository in
    /// a request that has read it through a pinned reader, which would otherwise wait for
    /// its own lease. Does nothing for an ordinary reader.
    pub fn close(&self) {
        if let Some(slot) = &self.pinned {
            let pinned = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
            drop(pinned);
        }
    }

    /// The lease this operation needs: none without a lock, none for a pinned reader that
    /// already holds one with its opened repository, otherwise a fresh one.
    async fn lease(&self) -> Option<RepositoryLease> {
        let lock = self.lock.as_ref()?;
        if self.pinned.as_ref().is_some_and(|slot| slot.lock().unwrap_or_else(PoisonError::into_inner).is_some()) {
            return None;
        }
        Some(Arc::clone(lock).shared().await)
    }

    /// Runs `operation` against the opened repository on the blocking thread pool.
    pub(crate) async fn with_repo<T, F>(&self, operation: F) -> Result<T, GitError>
    where
        T: Send + 'static,
        F: FnOnce(&gix::Repository) -> Result<T, GitError> + Send + 'static,
    {
        let path = self.path.clone();
        let pinned = self.pinned.clone();
        let mut lease = self.lease().await;
        tokio::task::spawn_blocking(move || {
            let mut repo = match pinned {
                // The first operation hands its lease to the pinned repository.
                Some(slot) => shared_repository(&slot, &path, &mut lease)?.to_thread_local(),
                None => gix::open_opts(&path, gix::open::Options::isolated()).map_err(GitError::backend)?,
            };
            repo.object_cache_size_if_unset(4 * 1024 * 1024);
            let result = operation(&repo);
            // Close this operation's files before giving up its lease.
            drop(repo);
            drop(lease);
            result
        })
        .await?
    }

    /// True when no branch has any commit yet (freshly created repository).
    pub async fn is_empty(&self) -> Result<bool, GitError> {
        self.with_repo(|repo| Ok(repo.head_id().is_err())).await
    }

    /// Branches and tags, branches first, each group sorted by name.
    pub async fn refs(&self) -> Result<Vec<RefSummary>, GitError> {
        self.with_repo(|repo| {
            let platform = repo.references().map_err(GitError::backend)?;
            let mut refs = Vec::new();
            for (kind, iter) in [
                (RefKind::Branch, platform.local_branches().map_err(GitError::backend)?),
                (RefKind::Tag, platform.tags().map_err(GitError::backend)?),
            ] {
                let start = refs.len();
                for reference in iter.flatten() {
                    let Some(target) = reference.try_id() else { continue };
                    refs.push(RefSummary {
                        name: reference.name().shorten().to_str_lossy().into_owned(),
                        kind,
                        target: target.to_hex().to_string(),
                    });
                }
                refs[start..].sort_by(|a, b| a.name.cmp(&b.name));
            }
            Ok(refs)
        })
        .await
    }

    /// Splits `"<revision>/<path>"` where the revision itself may contain slashes
    /// (e.g. `feature/login/src/main.rs`). The longest prefix that names an existing
    /// branch, tag or commit wins — the same rule GitHub applies.
    pub async fn split_revision_path(&self, combined: String) -> Result<(String, String), GitError> {
        self.with_repo(move |repo| {
            let combined = combined.trim_matches('/');
            let split_points = combined.match_indices('/').map(|(i, _)| i).rev();
            for end in std::iter::once(combined.len()).chain(split_points) {
                let candidate = &combined[..end];
                if resolve_commit(repo, candidate).is_ok() {
                    let path = combined[end..].trim_start_matches('/').to_owned();
                    return Ok((candidate.to_owned(), path));
                }
            }
            Err(GitError::NotFound("revision"))
        })
        .await
    }

    /// Lists the directory at `path` (empty for the root) as of `revision`.
    pub async fn tree(&self, revision: String, path: String) -> Result<TreeView, GitError> {
        self.with_repo(move |repo| {
            let commit = repo.find_commit(resolve_commit(repo, &revision)?).map_err(GitError::backend)?;
            let root = commit.tree().map_err(GitError::backend)?;
            let tree = if path.is_empty() {
                root
            } else {
                let entry = root.lookup_entry_by_path(&path).map_err(GitError::backend)?.ok_or(GitError::NotFound("path"))?;
                if !entry.mode().is_tree() {
                    return Err(GitError::NotFound("directory"));
                }
                entry.object().map_err(GitError::backend)?.into_tree()
            };

            let decoded = tree.decode().map_err(GitError::backend)?;
            let mut entries: Vec<TreeEntry> = decoded
                .entries
                .iter()
                .map(|entry| TreeEntry {
                    name: entry.filename.to_str_lossy().into_owned(),
                    kind: entry_kind(entry.mode.kind()),
                    object_id: entry.oid.to_hex().to_string(),
                })
                .collect();
            entries.sort_by(|a, b| {
                a.kind.is_directory().cmp(&b.kind.is_directory()).reverse().then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });

            Ok(TreeView { commit: summarize(&commit)?, entries })
        })
        .await
    }

    /// Reads the file at `path` as of `revision`. Content larger than `max_bytes` is omitted.
    pub async fn blob(&self, revision: String, path: String, max_bytes: usize) -> Result<BlobView, GitError> {
        self.with_repo(move |repo| {
            let commit = repo.find_commit(resolve_commit(repo, &revision)?).map_err(GitError::backend)?;
            let entry = commit
                .tree()
                .map_err(GitError::backend)?
                .lookup_entry_by_path(&path)
                .map_err(GitError::backend)?
                .ok_or(GitError::NotFound("file"))?;
            if entry.mode().is_tree() || entry.mode().is_commit() {
                return Err(GitError::NotFound("file"));
            }

            let header = repo.find_header(entry.object_id()).map_err(GitError::backend)?;
            let size = header.size();
            let data = if usize::try_from(size).is_ok_and(|s| s <= max_bytes) {
                Some(repo.find_blob(entry.object_id()).map_err(GitError::backend)?.take_data())
            } else {
                None
            };
            let is_binary = data.as_deref().is_some_and(is_binary);

            Ok(BlobView { commit: summarize(&commit)?, id: entry.object_id().to_hex().to_string(), size, data, is_binary })
        })
        .await
    }

    /// Finds and reads the README of the directory at `path`, if there is one.
    pub async fn readme(
        &self,
        revision: String,
        directory: &[TreeEntry],
        path: &str,
        max_bytes: usize,
    ) -> Result<Option<(String, String)>, GitError> {
        let Some(name) = README_NAMES
            .iter()
            .find_map(|candidate| directory.iter().find(|e| !e.kind.is_directory() && e.name.eq_ignore_ascii_case(candidate)))
            .map(|e| e.name.clone())
        else {
            return Ok(None);
        };
        let file_path = if path.is_empty() { name.clone() } else { format!("{path}/{name}") };
        let blob = self.blob(revision, file_path, max_bytes).await?;
        Ok(blob.text().map(|text| (name, text.to_owned())))
    }

    /// Commit history reachable from `revision`, newest first, paginated.
    pub async fn history(&self, revision: String, skip: usize, limit: usize) -> Result<Vec<CommitSummary>, GitError> {
        self.with_repo(move |repo| {
            let tip = resolve_commit(repo, &revision)?;
            repo.rev_walk([tip])
                .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
                .all()
                .map_err(GitError::backend)?
                .skip(skip)
                .take(limit)
                .map(|info| {
                    let info = info.map_err(GitError::backend)?;
                    summarize(&info.object().map_err(GitError::backend)?)
                })
                .collect()
        })
        .await
    }

    /// A bounded history across all requested branch tips, with shared ancestors
    /// visited once. The graph view orders this window with children before parents.
    pub async fn branch_history(&self, revisions: Vec<String>, limit: usize) -> Result<Vec<CommitSummary>, GitError> {
        self.with_repo(move |repo| {
            let tips = revisions.iter().map(|revision| resolve_commit(repo, revision)).collect::<Result<Vec<_>, _>>()?;
            repo.rev_walk(tips)
                .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
                .all()
                .map_err(GitError::backend)?
                .take(limit)
                .map(|info| summarize(&info.map_err(GitError::backend)?.object().map_err(GitError::backend)?))
                .collect()
        })
        .await
    }
}

/// The repository held by a pinned reader, opened on first use. Concurrent first calls
/// wait for a single open rather than each opening their own.
/// The pinned repository in `slot`, opened on first use. Opening it moves `lease` into
/// the slot, where it stays as long as the repository.
fn shared_repository(
    slot: &Mutex<Option<Pinned>>,
    path: &Path,
    lease: &mut Option<RepositoryLease>,
) -> Result<gix::ThreadSafeRepository, GitError> {
    // A panic while opening leaves `None` behind, which is safe to retry.
    let mut guard = slot.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(pinned) = guard.as_ref() {
        return Ok(pinned.repo.clone());
    }
    let repo = gix::ThreadSafeRepository::open_opts(path, gix::open::Options::isolated()).map_err(GitError::backend)?;
    *guard = Some(Pinned { repo: repo.clone(), _lease: lease.take() });
    Ok(repo)
}

/// Resolves a branch, tag, or (abbreviated) commit id to a commit id.
pub(crate) fn resolve_commit(repo: &gix::Repository, revision: &str) -> Result<ObjectId, GitError> {
    // Restrict to plain names and hex ids: full rev-parse syntax (`@{-1}`, `^{/regex}`)
    // is unnecessary for URLs and expands the attack surface.
    if revision.is_empty() || revision.contains("..") || revision.contains(['@', '^', '~', ':', '{']) {
        return Err(GitError::NotFound("revision"));
    }
    let id = repo.rev_parse_single(revision).map_err(|_| GitError::NotFound("revision"))?;
    let object = id.object().map_err(GitError::backend)?.peel_to_commit().map_err(|_| GitError::NotFound("commit"))?;
    Ok(object.id)
}

fn entry_kind(kind: GixEntryKind) -> EntryKind {
    match kind {
        GixEntryKind::Tree => EntryKind::Directory,
        GixEntryKind::Blob => EntryKind::File,
        GixEntryKind::BlobExecutable => EntryKind::Executable,
        GixEntryKind::Link => EntryKind::Symlink,
        GixEntryKind::Commit => EntryKind::Submodule,
    }
}

pub(crate) fn summarize(commit: &gix::Commit<'_>) -> Result<CommitSummary, GitError> {
    let message = commit.message().map_err(GitError::backend)?;
    let author = commit.author().map_err(GitError::backend)?.trim();
    let body = message.body.map(|b| b.to_str_lossy().trim().to_owned()).filter(|b| !b.is_empty());

    Ok(CommitSummary {
        id: commit.id.to_hex().to_string(),
        short_id: commit.id.to_hex_with_len(7).to_string(),
        summary: message.summary().to_str_lossy().into_owned(),
        body,
        author_name: author.name.to_str_lossy().into_owned(),
        author_email: author.email.to_str_lossy().into_owned(),
        authored_at: Timestamp(author.seconds()),
        parent_ids: commit.parent_ids().map(|id| id.to_hex().to_string()).collect(),
    })
}
