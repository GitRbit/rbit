//! Server-side branch writes an agent asks for (agent plan step 4.4), each a commit made in
//! a scratch overlay and handed over by compare-and-swap, as merges are:
//!
//! * [`update_branch`]: merges the base tip into a pull request's head branch, only if the
//!   head is still the commit the caller saw.
//! * [`revert`]: a new branch from the base tip with one commit that undoes a commit, the
//!   three-way merge of the base with the commit's parent over the commit itself.

use std::path::Path;

use rbit_core::{Timestamp, issue::MergeStrategy, pull::CommitIdentity};

use super::{
    Git, MERGES_DIR, MergeEnvironment, MergeError, MergeSpec, Merged, Overlay, Signature, build, hand_over, validate_id,
};
use crate::pull_refs::branch_ref;

/// Merges `base_tip` into branch `head_branch` of `repo` if it still is `head_sha`, with a
/// merge commit by `merger`; [`MergeError::NothingToMerge`] when the head already has it.
///
/// # Errors
///
/// [`MergeError::BaseMoved`] when the head moved, [`MergeError::Conflict`], or a Git failure.
pub async fn update_branch(
    env: &MergeEnvironment,
    repo: &Path,
    (head_branch, head_sha): (&str, &str),
    (base_branch, base_tip): (&str, &str),
    merger: &CommitIdentity,
    at: Timestamp,
) -> Result<Merged, MergeError> {
    let message = format!("Merge branch '{base_branch}' into {head_branch}");
    let spec = MergeSpec {
        repo,
        base_branch: head_branch,
        base_tip: head_sha,
        head: base_tip,
        strategy: MergeStrategy::Merge,
        message: &message,
        author: merger,
        merger,
        at,
    };
    let target = branch_ref(head_branch)?;
    build(env, &spec, &target, head_sha, "rbit: update branch from its base").await
}

/// Creates branch `new_branch` of `repo` at a commit on `base_tip` that reverts `commit`.
///
/// # Errors
///
/// [`MergeError::Conflict`] when the revert does not apply cleanly,
/// [`MergeError::NothingToMerge`] when the base no longer has the change,
/// [`MergeError::BaseMoved`] when `new_branch` exists, or a Git failure.
pub async fn revert(
    env: &MergeEnvironment,
    repo: &Path,
    (base_tip, commit): (&str, &str),
    new_branch: &str,
    author: &CommitIdentity,
    at: Timestamp,
) -> Result<Merged, MergeError> {
    validate_id(base_tip)?;
    validate_id(commit)?;
    let target = branch_ref(new_branch)?;
    let scratch = Overlay::create_in(&env.tmp_dir.join(MERGES_DIR), &[repo]).await?;
    let git = Git { binary: &env.git_binary, dir: scratch.path() };
    let Some(parent) = git.rev_parse(&format!("{commit}^")).await? else {
        return Err(MergeError::Failed("a commit without a parent cannot be reverted".to_owned()));
    };
    let original = git.read_commit(commit).await?;
    let summary = String::from_utf8_lossy(&original.message).lines().next().unwrap_or_default().trim().to_owned();
    let tree = git.merge_tree(Some(commit), base_tip, &parent).await?.map_err(MergeError::Conflict)?;
    if tree == git.tree_of(base_tip).await? {
        return Err(MergeError::NothingToMerge);
    }
    let message = format!("Revert \"{summary}\"\n\nThis reverts commit {commit}.\n");
    let signature = Signature::new(author, at);
    let reverted = git.commit_tree(&tree, &[base_tip], message.as_bytes(), &signature, &signature).await?;
    let base = Git { binary: &env.git_binary, dir: repo };
    let zeros = "0".repeat(base_tip.len());
    hand_over(&git, &base, &target, &reverted, &zeros, "rbit: revert a commit").await?;
    drop(scratch);
    Ok(Merged { commit: reverted })
}
