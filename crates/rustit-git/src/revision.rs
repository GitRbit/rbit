//! Resolving a branch, tag or abbreviated id to the full commit id it names, and
//! creating tags.
//!
//! Derived data (language statistics, last commits) is cached by commit id because a
//! commit's content never changes, while a branch name moves with every push.

use crate::{GitError, RepositoryReader, reader::resolve_commit};

impl RepositoryReader {
    /// The full hexadecimal id of the commit `revision` names.
    pub async fn commit_id(&self, revision: String) -> Result<String, GitError> {
        self.with_repo(move |repo| Ok(resolve_commit(repo, &revision)?.to_hex().to_string())).await
    }

    /// Creates the lightweight tag `name` pointing at the commit `target` names (a branch,
    /// tag or commit id) and returns that commit's id. Fails with
    /// [`GitError::AlreadyExists`] when the tag exists and [`GitError::InvalidBranchName`]
    /// when `name` is not a valid tag name.
    ///
    /// The reference is written with Git's locking (a `.lock` file, then a rename), so it
    /// is safe against concurrent pushes.
    pub async fn create_tag(&self, name: String, target: String) -> Result<String, GitError> {
        self.with_repo(move |repo| {
            let full_name = format!("refs/tags/{name}");
            if name.is_empty() || name.starts_with('-') || gix::refs::FullName::try_from(full_name.as_str()).is_err() {
                return Err(GitError::InvalidBranchName(name));
            }
            if repo.try_find_reference(full_name.as_str()).map_err(GitError::backend)?.is_some() {
                return Err(GitError::AlreadyExists);
            }
            let commit = resolve_commit(repo, &target)?;
            repo.tag_reference(&name, commit, gix::refs::transaction::PreviousValue::MustNotExist).map_err(GitError::backend)?;
            Ok(commit.to_hex().to_string())
        })
        .await
    }
}
