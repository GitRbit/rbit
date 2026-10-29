//! What the runner keeps between jobs in its work directory (the Node.js of JavaScript
//! actions, fetched action repositories) is checked at each use against a SHA-256 this
//! runner process recorded when it put it there.
//!
//! A host label's job runs as the runner's own user and can change those files, for itself
//! and for every later job, container jobs included. Copies made by another runner process,
//! or changed since, are not used: they are downloaded or fetched again. The digests live
//! in memory only, where no job can write them. This keeps one job's changes from reaching
//! another through the runner's caches; it cannot make host jobs safe neighbours, since
//! they can read everything the runner's user can (see `docs/actions.md`).

use std::{
    collections::BTreeMap,
    io::{self, Read as _},
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

use sha2::{Digest as _, Sha256};

/// The digest recorded for each path this process put in place.
static RECORDED: Mutex<BTreeMap<PathBuf, [u8; 32]>> = Mutex::new(BTreeMap::new());

/// What a path is hashed as: one file's content, or a whole directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Tree,
}

/// Records `path` as it is now. Blocking.
pub fn record(path: &Path, kind: Kind) -> io::Result<()> {
    let digest = digest(path, kind)?;
    RECORDED.lock().unwrap_or_else(PoisonError::into_inner).insert(path.to_path_buf(), digest);
    Ok(())
}

/// Whether `path` is what this process recorded: `false` for anything it never recorded,
/// changed since, or unreadable. Blocking.
pub fn intact(path: &Path, kind: Kind) -> bool {
    let recorded = RECORDED.lock().unwrap_or_else(PoisonError::into_inner).get(path).copied();
    recorded.is_some_and(|recorded| digest(path, kind).is_ok_and(|now| now == recorded))
}

/// Forgets `path`, which is being replaced.
pub fn forget(path: &Path) {
    RECORDED.lock().unwrap_or_else(PoisonError::into_inner).remove(path);
}

fn digest(path: &Path, kind: Kind) -> io::Result<[u8; 32]> {
    let mut hasher = Sha256::new();
    match kind {
        Kind::File => hash_file(&mut hasher, path)?,
        Kind::Tree => hash_tree(&mut hasher, path, Path::new(""))?,
    }
    Ok(hasher.finalize().into())
}

fn hash_file(hasher: &mut Sha256, path: &Path) -> io::Result<()> {
    let mut file = std::fs::File::open(path)?;
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(());
        }
        hasher.update(&buffer[..read]);
    }
}

/// Every entry under `dir` in name order: its relative path, its kind, whether it is
/// executable, and a file's content or a link's target. Links are not followed.
fn hash_tree(hasher: &mut Sha256, dir: &Path, relative: &Path) -> io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<io::Result<_>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let name = relative.join(entry.file_name());
        let metadata = std::fs::symlink_metadata(&path)?;
        hasher.update(name.to_string_lossy().as_bytes());
        hasher.update([0]);
        if metadata.is_dir() {
            hasher.update(b"d");
            hash_tree(hasher, &path, &name)?;
        } else if metadata.is_file() {
            hasher.update(if executable(&metadata) { b"x" } else { b"f" });
            hasher.update(metadata.len().to_le_bytes());
            hash_file(hasher, &path)?;
        } else {
            hasher.update(b"l");
            hasher.update(std::fs::read_link(&path).map(|target| target.to_string_lossy().into_owned()).unwrap_or_default().as_bytes());
        }
        hasher.update([0]);
    }
    Ok(())
}

#[cfg(unix)]
fn executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable(_: &std::fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_what_this_process_recorded_and_left_alone_is_intact() {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("action");
        std::fs::create_dir_all(tree.join("dist")).unwrap();
        std::fs::write(tree.join("dist/index.js"), "console.log('clean')").unwrap();
        let node = dir.path().join("node");
        std::fs::write(&node, "binary").unwrap();

        assert!(!intact(&tree, Kind::Tree) && !intact(&node, Kind::File), "never recorded");
        record(&tree, Kind::Tree).unwrap();
        record(&node, Kind::File).unwrap();
        assert!(intact(&tree, Kind::Tree) && intact(&node, Kind::File));

        std::fs::write(tree.join("dist/index.js"), "console.log('POISONED')").unwrap();
        std::fs::write(&node, "#!/bin/sh\necho POISONED").unwrap();
        assert!(!intact(&tree, Kind::Tree) && !intact(&node, Kind::File), "changed since");

        record(&tree, Kind::Tree).unwrap();
        std::fs::write(tree.join("added.js"), "").unwrap();
        assert!(!intact(&tree, Kind::Tree), "a file added");
        forget(&tree);
        assert!(!intact(&tree, Kind::Tree));
    }
}
