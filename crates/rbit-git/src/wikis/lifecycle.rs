//! Whole-wiki object work outside page changes: removing the object stores of a
//! deleted repository, and copying or verifying exactly the objects a catalog's
//! finalized revisions reach (backups and restores).
//!
//! These are blocking filesystem operations for an owner that already serializes them
//! with page changes (a service's writer lock, or an offline backup copy). Every object
//! that is copied or verified is re-hashed: a stored object whose bytes do not match its
//! name is refused, never silently carried along.
use super::{WikiObjectError, layout, writer_lock};
use rbit_core::RepoId;
use rbit_wikis::{WikiId, WikiPageId, WikiRevisionId, WikiStoreId};
use std::{collections::HashSet, fs, path::Path};

fn io(error: std::io::Error) -> WikiObjectError {
    crate::GitError::Io(error).into()
}

/// Removes the object stores of `repository` under the bound `root`, except the wiki
/// incarnations in `keep` (the repository's current wiki, and any wiki whose accepted
/// intents still need their objects). Returns how many stores were removed. A missing
/// directory is nothing to do. Holds the root's writer lock while removing.
pub fn remove_repository(root: &Path, identity: WikiStoreId, repository: RepoId, keep: &[WikiId]) -> Result<usize, WikiObjectError> {
    if repository.0 <= 0 {
        return Err(WikiObjectError::Binding);
    }
    layout::verify_root(root, identity)?;
    let _writer = writer_lock::acquire(root)?;
    let parent = root.join(repository.0.to_string());
    layout::no_links(&parent)?;
    let entries = match fs::read_dir(&parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(io(error)),
    };
    let mut removed = 0;
    let mut remaining = 0;
    for entry in entries {
        let entry = entry.map_err(io)?;
        let name = entry.file_name();
        let wiki = name.to_str().and_then(|name| name.strip_suffix(".git")).and_then(|id| id.parse::<i64>().ok()).and_then(WikiId::new);
        match wiki {
            Some(wiki) if !keep.contains(&wiki) && entry.file_type().map_err(io)?.is_dir() => {
                remove_tree(&entry.path())?;
                removed += 1;
            }
            _ => remaining += 1,
        }
    }
    if remaining == 0 {
        fs::remove_dir(&parent).map_err(io)?;
    }
    Ok(removed)
}

/// Removes a directory tree whose Git objects may be read-only (Windows).
fn remove_tree(path: &Path) -> Result<(), WikiObjectError> {
    if fs::remove_dir_all(path).is_ok() {
        return Ok(());
    }
    let mut pending = vec![path.to_owned()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).map_err(io)? {
            let entry = entry.map_err(io)?;
            if entry.file_type().map_err(io)?.is_dir() {
                pending.push(entry.path());
            } else {
                let mut permissions = entry.metadata().map_err(io)?.permissions();
                #[expect(clippy::permissions_set_readonly_false, reason = "the file is deleted next")]
                permissions.set_readonly(false);
                fs::set_permissions(entry.path(), permissions).map_err(io)?;
            }
        }
    }
    fs::remove_dir_all(path).map_err(io)
}

/// One wiki incarnation and the finalized revisions its catalog records.
#[derive(Debug, Clone)]
pub struct WikiScope<'a> {
    pub repository: RepoId,
    pub wiki: WikiId,
    pub revisions: &'a [WikiRevisionId],
}

/// How many distinct objects a copy or verification handled.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WikiObjectCount {
    pub objects: u64,
}

/// Visits every object `revisions` reach (commit, tree, page blobs), each once, with
/// its re-hashed kind and bytes. `Missing` when one is absent.
fn walk(
    repo: &gix::Repository,
    revisions: &[WikiRevisionId],
    mut visit: impl FnMut(gix::ObjectId, gix::objs::Kind, &[u8]) -> Result<(), WikiObjectError>,
) -> Result<u64, WikiObjectError> {
    let mut seen = HashSet::new();
    let mut pending = Vec::new();
    for revision in revisions {
        pending.push((gix::ObjectId::try_from(revision.as_bytes()).map_err(|_| WikiObjectError::Format)?, gix::objs::Kind::Commit));
    }
    while let Some((id, expected)) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(object) = repo.try_find_object(id).map_err(crate::GitError::backend)? else {
            return Err(WikiObjectError::Missing);
        };
        let actual = gix::objs::compute_hash(repo.object_hash(), object.kind, &object.data).map_err(crate::GitError::backend)?;
        if object.kind != expected || actual != id {
            return Err(WikiObjectError::Format);
        }
        match object.kind {
            gix::objs::Kind::Commit => {
                let commit = gix::objs::CommitRef::from_bytes(&object.data, repo.object_hash()).map_err(crate::GitError::backend)?;
                pending.push((commit.tree(), gix::objs::Kind::Tree));
            }
            gix::objs::Kind::Tree => {
                for entry in gix::objs::TreeRefIter::from_bytes(&object.data, repo.object_hash()) {
                    let entry = entry.map_err(crate::GitError::backend)?;
                    let name = std::str::from_utf8(entry.filename.as_ref()).map_err(|_| WikiObjectError::Format)?;
                    let page = name.strip_suffix(".md").and_then(|stem| stem.parse::<i64>().ok()).and_then(WikiPageId::new);
                    if page.is_none() || entry.mode.kind() != gix::objs::tree::EntryKind::Blob {
                        return Err(WikiObjectError::Format);
                    }
                    pending.push((entry.oid.to_owned(), gix::objs::Kind::Blob));
                }
            }
            gix::objs::Kind::Blob => {}
            gix::objs::Kind::Tag => return Err(WikiObjectError::Format),
        }
        visit(id, object.kind, &object.data)?;
    }
    Ok(u64::try_from(seen.len()).unwrap_or(u64::MAX))
}

/// Copies exactly the objects `scope`'s revisions reach from the store under `from` into
/// a new store under `to`, both bound to `identity`. `Missing` when the source lacks one
/// (its repository was deleted meanwhile); the partial copy is then removed.
pub fn copy_scope(from: &Path, to: &Path, identity: WikiStoreId, scope: &WikiScope<'_>) -> Result<WikiObjectCount, WikiObjectError> {
    layout::verify_root(from, identity)?;
    let store = from.join(scope.repository.0.to_string()).join(format!("{}.git", scope.wiki.get()));
    if !store.try_exists().map_err(io)? {
        return Err(WikiObjectError::Missing);
    }
    let source = layout::repository(from, identity, scope.repository, scope.wiki, false)?;
    let to = layout::bind_root(to, identity)?;
    let target = layout::repository(&to, identity, scope.repository, scope.wiki, true)?;
    let copied = walk(&source, scope.revisions, |id, kind, data| {
        let written = gix::objs::Write::write_buf_with_known_id(&target.objects, kind, data, id).map_err(crate::GitError::backend)?;
        if written == id { Ok(()) } else { Err(WikiObjectError::Format) }
    });
    match copied {
        Ok(objects) => Ok(WikiObjectCount { objects }),
        Err(error) => {
            drop(target);
            let _ = remove_tree(&to.join(scope.repository.0.to_string()).join(format!("{}.git", scope.wiki.get())));
            Err(error)
        }
    }
}

/// Checks that every object `scope`'s revisions reach is present and intact in the
/// store under `root`, which must be bound to `identity`.
pub fn verify_scope(root: &Path, identity: WikiStoreId, scope: &WikiScope<'_>) -> Result<WikiObjectCount, WikiObjectError> {
    layout::verify_root(root, identity)?;
    let repo = layout::repository(root, identity, scope.repository, scope.wiki, false)?;
    Ok(WikiObjectCount { objects: walk(&repo, scope.revisions, |_, _, _| Ok(()))? })
}
