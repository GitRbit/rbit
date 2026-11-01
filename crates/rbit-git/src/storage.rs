//! Physical layout of bare repositories: `<root>/<owner_id>/<repo_id>.git`.
//!
//! Numeric ids (not names) keep renames free and make path traversal impossible. Work
//! inside a repository's directory is coordinated with moving it through per-repository
//! leases (see [`crate::lease`]).

use std::path::{Path, PathBuf};

use rbit_core::{AccountId, RepoId};

use crate::{
    GitError, RepositoryReader,
    lease::{ExclusiveLease, RepositoryLease, RepositoryLocks},
};

/// The repositories under one root directory. Clones share the same leases, so create
/// one per root and clone it.
#[derive(Debug, Clone)]
pub struct RepositoryStorage {
    root: PathBuf,
    locks: RepositoryLocks,
    pub(crate) owned_edits: std::sync::Arc<crate::owned_edit::state::Registry>,
    #[cfg(test)]
    pub(crate) worker_gates: crate::worker_gate::WorkerGates,
}

impl RepositoryStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            locks: RepositoryLocks::default(),
            owned_edits: std::sync::Arc::default(),
            #[cfg(test)]
            worker_gates: crate::worker_gate::WorkerGates::default(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path_for(&self, owner: AccountId, repo: RepoId) -> PathBuf {
        self.root.join(owner.0.to_string()).join(format!("{}.git", repo.0))
    }

    /// A reader whose every operation holds a [`RepositoryLease`] while it has the
    /// repository open. Moves normally wait for readers, but may proceed after the
    /// reader-wait budget expires; this lease is cooperative coordination.
    pub fn reader(&self, owner: AccountId, repo: RepoId) -> RepositoryReader {
        RepositoryReader::new(self.path_for(owner, repo)).with_lock(self.locks.lock_for(repo))
    }

    /// Waits until `repo` is not being moved and takes a shared lease. A later move
    /// normally waits for it, but may proceed after its reader-wait budget expires.
    /// Take one around work that opens files outside a [`RepositoryReader`] (a `git`
    /// process, for example). Leases may be nested; they are not physical exclusion.
    pub async fn lease(&self, repo: RepoId) -> RepositoryLease {
        self.locks.shared(repo).await
    }

    /// Waits until no lease on `repo` is held (at most [`crate::lease::MAX_WAIT`]) and
    /// holds back new ones until dropped.
    pub(crate) async fn exclusive(&self, repo: RepoId) -> ExclusiveLease {
        self.locks.exclusive(repo).await
    }

    /// Initialises an empty bare repository whose `HEAD` points at `default_branch`.
    pub async fn create(&self, owner: AccountId, repo: RepoId, default_branch: &str) -> Result<PathBuf, GitError> {
        let head_ref = head_ref(default_branch)?;

        let path = self.path_for(owner, repo);
        if tokio::fs::try_exists(&path).await? {
            return Err(GitError::AlreadyExists);
        }

        let init_path = path.clone();
        tokio::task::spawn_blocking(move || -> Result<(), GitError> { create_fixed(&init_path, &head_ref) }).await??;

        tracing::info!(path = %path.display(), "initialised bare repository");
        Ok(path)
    }

    /// Points the bare repository's `HEAD` at `refs/heads/{branch}`, which decides what
    /// `git clone` checks out. The branch name is validated like in [`Self::create`].
    ///
    /// The new `HEAD` is written to `HEAD.lock` and renamed into place (Git's own locking
    /// convention), so concurrent readers never observe a half-written file and a
    /// concurrent Git process holding the lock makes this fail instead of racing it.
    pub async fn set_head(&self, owner: AccountId, repo: RepoId, branch: &str) -> Result<(), GitError> {
        let head_ref = head_ref(branch)?;
        let path = self.path_for(owner, repo);
        if !tokio::fs::try_exists(path.join("HEAD")).await? {
            return Err(GitError::NotFound("repository"));
        }

        tokio::task::spawn_blocking(move || -> Result<(), GitError> { set_head_fixed(&path, &head_ref) }).await??;
        Ok(())
    }

    /// Permanently removes a repository from disk after readers and background jobs
    /// release their open files, keeping new work out until deletion finishes.
    pub async fn delete(&self, owner: AccountId, repo: RepoId) -> Result<(), GitError> {
        let lease = self.exclusive(repo).await;
        let path = self.path_for(owner, repo);
        #[cfg(test)]
        let gate = self.worker_gates.take();
        tokio::task::spawn_blocking(move || -> Result<(), GitError> {
            // The actual filesystem worker retains exclusion if its async waiter leaves.
            let _lease = lease;
            #[cfg(test)]
            if let Some(gate) = gate {
                gate.enter();
            }
            delete_fixed(&path)
        })
        .await?
    }
}

pub(crate) fn create_fixed(path: &Path, head_ref: &str) -> Result<(), GitError> {
    std::fs::create_dir_all(path)?;
    gix::init_bare(path).map_err(GitError::backend)?;
    std::fs::write(path.join("HEAD"), format!("ref: {head_ref}\n"))?;
    Ok(())
}

pub(crate) fn set_head_fixed(path: &Path, head_ref: &str) -> Result<(), GitError> {
    let lock = path.join("HEAD.lock");
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&lock)?;
    let written = std::io::Write::write_all(
        &mut file,
        format!(
            "ref: {head_ref}
"
        )
        .as_bytes(),
    )
    .and_then(|()| file.sync_all());
    drop(file);
    if let Err(error) = written.and_then(|()| std::fs::rename(&lock, path.join("HEAD"))) {
        let _ = std::fs::remove_file(&lock);
        return Err(error.into());
    }
    Ok(())
}

pub(crate) fn delete_fixed(path: &Path) -> Result<(), GitError> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// `refs/heads/{branch}`, if that is a valid Git reference name.
pub(crate) fn head_ref(branch: &str) -> Result<String, GitError> {
    let full = format!("refs/heads/{branch}");
    gix::refs::FullName::try_from(full.as_str()).map_err(|_| GitError::InvalidBranchName(branch.to_owned()))?;
    Ok(full)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn deletion_waits_for_open_repository_work() {
        let dir = tempfile::tempdir().unwrap();
        let storage = RepositoryStorage::new(dir.path());
        let owner = AccountId(1);
        let repo = RepoId(2);
        let path = storage.create(owner, repo, "main").await.unwrap();
        let lease = storage.lease(repo).await;
        // A reader or cache warm-up may have a repository file open after a push.
        // Windows refuses deletion while that handle lives; Unix must wait too.
        let open = std::fs::File::open(path.join("HEAD")).unwrap();
        let deleting = tokio::spawn({
            let storage = storage.clone();
            async move { storage.delete(owner, repo).await }
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!deleting.is_finished(), "deletion must wait for the repository's lease");
        assert!(path.join("HEAD").exists(), "no files are removed while the repository is in use");
        drop(open);
        drop(lease);
        tokio::time::timeout(std::time::Duration::from_secs(5), deleting).await.unwrap().unwrap().unwrap();
        assert!(!path.exists());
        storage.delete(owner, repo).await.unwrap();
    }

    #[tokio::test]
    async fn set_head_points_head_at_branch() {
        let dir = tempfile::tempdir().unwrap();
        let storage = RepositoryStorage::new(dir.path());
        let path = storage.create(AccountId(1), RepoId(2), "main").await.unwrap();

        storage.set_head(AccountId(1), RepoId(2), "release/1.x").await.unwrap();
        assert_eq!(
            std::fs::read_to_string(path.join("HEAD")).unwrap(),
            "ref: refs/heads/release/1.x
"
        );
        assert!(!path.join("HEAD.lock").exists());
    }

    #[tokio::test]
    async fn set_head_rejects_invalid_names_and_missing_repositories() {
        let dir = tempfile::tempdir().unwrap();
        let storage = RepositoryStorage::new(dir.path());
        let path = storage.create(AccountId(1), RepoId(2), "main").await.unwrap();

        for bad in ["", "a..b", "with space", "../escape", "x.lock"] {
            let result = storage.set_head(AccountId(1), RepoId(2), bad).await;
            assert!(matches!(result, Err(GitError::InvalidBranchName(_))), "{bad:?} should be rejected");
        }
        assert_eq!(
            std::fs::read_to_string(path.join("HEAD")).unwrap(),
            "ref: refs/heads/main
"
        );
        assert!(matches!(storage.set_head(AccountId(1), RepoId(3), "main").await, Err(GitError::NotFound(_))));
    }
}
