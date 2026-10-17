//! Object overlays: throw-away bare repositories under `<tmp_dir>/overlays` whose
//! `objects/info/alternates` list two repositories' object directories, so commits of a
//! fork and its parent can be compared without writing to either.
//!
//! Milestone 2 workstream P (pull requests) owns this file.
//!
//! An [`Overlay`] has no refs and no configuration of its own beyond what
//! `gix::init_bare` writes: callers resolve branch names in each repository first and
//! then work with commit ids, which the overlay finds in either object store. The same
//! kind of repository (with the base repository as its only alternate) is the scratch
//! space of merges (`<tmp_dir>/merges`, see [`crate::merge`]): new objects land in the
//! overlay, never in the repositories it reads.
//!
//! The directory is removed when the overlay is dropped — on success, on error and while
//! unwinding from a panic — and [`sweep`] removes leftovers (a crash, a kill) at start-up.

use std::{
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hasher},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{GitError, RepositoryReader, lease::RepositoryLease};

/// Subdirectory of the temporary directory holding compare overlays.
pub const OVERLAYS_DIR: &str = "overlays";
/// Subdirectory of the temporary directory holding merge workspaces.
pub const MERGES_DIR: &str = "merges";

/// A temporary bare repository reading the objects of other repositories.
#[derive(Debug)]
pub struct Overlay {
    path: PathBuf,
    /// Keeps the repositories it reads in place (no transfer moves them meanwhile).
    leases: Vec<RepositoryLease>,
}

impl Overlay {
    /// Creates an overlay under `<tmp_dir>/overlays` reading the objects of every
    /// repository in `sources` (bare repository paths).
    pub async fn create(tmp_dir: &Path, sources: &[&Path]) -> Result<Self, GitError> {
        Self::create_in(&tmp_dir.join(OVERLAYS_DIR), sources).await
    }

    /// Like [`Self::create`], directly inside `parent` (e.g. `<tmp_dir>/merges`).
    pub async fn create_in(parent: &Path, sources: &[&Path]) -> Result<Self, GitError> {
        let mut alternates = String::new();
        for source in sources {
            if !tokio::fs::try_exists(source.join("HEAD")).await? {
                return Err(GitError::NotFound("repository"));
            }
            let objects = std::path::absolute(source.join("objects"))?;
            // Forward slashes are understood by Git and gitoxide on every platform.
            alternates.push_str(&objects.to_string_lossy().replace('\\', "/"));
            alternates.push('\n');
        }
        // Created synchronously (a handful of small files): with no await point between
        // creating the directory and owning its guard, even a cancelled caller (a timeout)
        // never leaves it behind.
        std::fs::create_dir_all(parent)?;
        let path = parent.join(random_name());
        std::fs::create_dir(&path)?;
        let overlay = Self { path, leases: Vec::new() };
        gix::init_bare(&overlay.path).map_err(GitError::backend)?;
        let info = overlay.path.join("objects").join("info");
        std::fs::create_dir_all(&info)?;
        std::fs::write(info.join("alternates"), alternates)?;
        Ok(overlay)
    }

    /// Keeps `lease` until the overlay is dropped.
    #[must_use]
    pub fn holding(mut self, lease: RepositoryLease) -> Self {
        self.leases.push(lease);
        self
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A reader over the overlay (commit ids only: it has no refs).
    pub fn reader(&self) -> RepositoryReader {
        RepositoryReader::new(self.path.clone())
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, path = %self.path.display(), "cannot remove a temporary repository; it is swept at the next start");
        }
    }
}

/// A name nobody can predict: 128 bits from the standard library's randomly seeded hasher
/// (fresh keys per [`RandomState`]), mixed with a counter, the time and the process id.
pub(crate) fn random_name() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let word = || {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
        hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        hasher.write_u32(std::process::id());
        hasher.finish()
    };
    format!("{:016x}{:016x}", word(), word())
}

/// Removes every overlay and merge workspace left under `tmp_dir` (call at start-up,
/// before any merge or compare runs).
pub async fn sweep(tmp_dir: &Path) -> Result<(), GitError> {
    for dir in [OVERLAYS_DIR, MERGES_DIR] {
        match tokio::fs::remove_dir_all(tmp_dir.join(dir)).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_distinct_hex() {
        let (a, b) = (random_name(), random_name());
        assert_ne!(a, b);
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn overlays_are_removed_when_dropped_even_while_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let storage = crate::RepositoryStorage::new(dir.path().join("repos"));
        let repo = storage.create(rbit_core::AccountId(1), rbit_core::RepoId(1), "main").await.unwrap();
        let tmp = dir.path().join("tmp");

        let overlay = Overlay::create(&tmp, &[&repo]).await.unwrap();
        let path = overlay.path().to_owned();
        assert!(path.join("objects/info/alternates").exists());
        drop(overlay);
        assert!(!path.exists());

        let overlay = Overlay::create(&tmp, &[&repo]).await.unwrap();
        let path = overlay.path().to_owned();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _held = overlay;
            panic!("boom");
        }));
        assert!(result.is_err());
        assert!(!path.exists(), "unwinding drops the guard");

        assert!(matches!(Overlay::create(&tmp, &[&dir.path().join("missing")]).await, Err(GitError::NotFound(_))));
        std::fs::create_dir_all(tmp.join(MERGES_DIR).join("left-over")).unwrap();
        sweep(&tmp).await.unwrap();
        assert!(!tmp.join(MERGES_DIR).exists() && !tmp.join(OVERLAYS_DIR).exists());
        sweep(&tmp).await.unwrap();
    }
}
