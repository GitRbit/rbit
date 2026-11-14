//! Copy a repository's default-branch tree without its commit history.

use std::{path::Path, time::Duration};

use rbit_core::{Timestamp, pull::CommitIdentity};

use crate::{
    merge::{Git, MergeError, Signature, hand_over},
    overlay::Overlay,
    pull_refs::{branch_ref, validate_id},
};

/// Creates a root commit in a disposable overlay and transfers the objects needed
/// by that commit. The destination can stand alone even after the source is removed.
pub async fn copy_snapshot(
    git_binary: &Path,
    tmp_dir: &Path,
    source: (&Path, &str),
    target: (&Path, &str),
    author: &CommitIdentity,
    timeout: Duration,
) -> Result<(), MergeError> {
    let (source_path, commit_id) = source;
    validate_id(commit_id)?;
    let target_ref = branch_ref(target.1)?;
    let work = async {
        let overlay = Overlay::create(tmp_dir, &[source_path]).await?;
        let scratch = Git { binary: git_binary, dir: overlay.path() };
        let tree = scratch.tree_of(commit_id).await?;
        let destination = Git { binary: git_binary, dir: target.0 };
        let signature = Signature::new(author, Timestamp::now());
        let commit = scratch.commit_tree(&tree, &[], b"Initial commit\n", &signature, &signature).await?;
        let zeros = "0".repeat(commit.len());
        hand_over(&scratch, &destination, &target_ref, &commit, &zeros, "create repository from template").await
    };
    tokio::time::timeout(timeout, work).await.map_err(|_| MergeError::TimedOut(timeout))?
}
