//! Scratch refs: `refs/scratch/<owner>/<name>`, where an agent pushes a commit that is on no
//! branch so CI tests it (agent plan step 3.1). They are not branches, so no branch list
//! shows them; the server records each one and deletes it when it expires.

use std::path::Path;

use crate::{GitError, RepositoryReader, merge::hardened, pull_refs::validate_id};

/// The prefix of every scratch ref.
pub const PREFIX: &str = "refs/scratch/";

/// The owner a scratch ref names (`refs/scratch/<owner>/…`), when `name` is one with a name
/// after the owner.
pub fn owner(name: &str) -> Option<&str> {
    let rest = name.strip_prefix(PREFIX)?;
    let (owner, tail) = rest.split_once('/')?;
    (!owner.is_empty() && !tail.is_empty()).then_some(owner)
}

impl RepositoryReader {
    /// Deletes scratch ref `name` if it still points at `expected`; `false` when it moved or
    /// is gone. Only names under [`PREFIX`] are ever deleted.
    pub async fn delete_scratch(&self, git_binary: &Path, name: &str, expected: &str) -> Result<bool, GitError> {
        if owner(name).is_none() || name.starts_with('-') || gix::refs::FullName::try_from(name).is_err() {
            return Err(GitError::InvalidBranchName(name.to_owned()));
        }
        validate_id(expected)?;
        let output = hardened(git_binary, Some(self.path()))
            .args(["update-ref", "--no-deref", "-m", "rbit: expire scratch ref", "-d", "--end-of-options"])
            .arg(name)
            .arg(expected)
            .output()
            .await?;
        Ok(output.status.success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scratch_ref_names_its_owner() {
        assert_eq!(owner("refs/scratch/ada/4c1e9a"), Some("ada"));
        assert_eq!(owner("refs/scratch/ada/deep/name"), Some("ada"));
        assert_eq!(owner("refs/scratch/ada"), None);
        assert_eq!(owner("refs/scratch//x"), None);
        assert_eq!(owner("refs/heads/scratch/ada/x"), None);
    }
}
