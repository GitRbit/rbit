//! Resolving a branch, tag or abbreviated id to the full commit id it names.
//!
//! Derived data (language statistics, last commits) is cached by commit id because a
//! commit's content never changes, while a branch name moves with every push.

use crate::{GitError, RepositoryReader, reader::resolve_commit};

impl RepositoryReader {
    /// The full hexadecimal id of the commit `revision` names.
    pub async fn commit_id(&self, revision: String) -> Result<String, GitError> {
        self.with_repo(move |repo| Ok(resolve_commit(repo, &revision)?.to_hex().to_string())).await
    }
}
