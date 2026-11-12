//! Merge-queue candidates (agent plan step 4.1): the merge of a pull request built on the
//! current base tip into a hidden ref `refs/merge-queue/<base>/pr-<n>`, where CI tests it
//! as a push to the branch `merge-queue/<base>/pr-<n>`; then exactly that tested commit
//! lands on the base branch by compare-and-swap, or nothing does.

use std::path::Path;

use super::{Git, MergeEnvironment, MergeError, MergeSpec, Merged, build, failure, validate_id};
use crate::pull_refs::branch_ref;

/// The prefix of every candidate ref.
pub const CANDIDATE_PREFIX: &str = "refs/merge-queue/";

/// The candidate ref of pull request `number` into `base`.
#[must_use]
pub fn candidate_ref(base: &str, number: u32) -> String {
    format!("{CANDIDATE_PREFIX}{base}/pr-{number}")
}

/// Builds `spec`'s merge on `spec.base_tip` into candidate ref `target` (replacing an
/// earlier candidate there). The base branch does not move.
///
/// # Errors
///
/// What a merge fails with ([`MergeError::Conflict`], [`MergeError::NothingToMerge`], …),
/// and [`MergeError::Failed`] for a `target` outside [`CANDIDATE_PREFIX`].
pub async fn candidate(env: &MergeEnvironment, spec: &MergeSpec<'_>, target: &str) -> Result<Merged, MergeError> {
    if !target.starts_with(CANDIDATE_PREFIX) || gix::refs::FullName::try_from(target).is_err() {
        return Err(MergeError::Failed(format!("`{target}` is not a merge-queue ref")));
    }
    let repo = Git { binary: &env.git_binary, dir: spec.repo };
    let expected = repo.rev_parse(target).await?.unwrap_or_else(|| "0".repeat(spec.base_tip.len()));
    build(env, spec, target, &expected, "rbit: merge queue candidate").await
}

/// Moves base branch `base` of `repo` from `expected` to the tested `commit`, atomically;
/// [`MergeError::BaseMoved`] when the branch is no longer `expected`.
///
/// # Errors
///
/// [`MergeError::BaseMoved`], or a Git failure.
pub async fn land(env: &MergeEnvironment, repo: &Path, base: &str, commit: &str, expected: &str) -> Result<(), MergeError> {
    validate_id(commit)?;
    validate_id(expected)?;
    let target = branch_ref(base)?;
    let git = Git { binary: &env.git_binary, dir: repo };
    let mut update = git.command();
    update.args(["update-ref", "--no-deref", "-m", "rbit: land merge queue candidate", "--end-of-options"]).arg(&target).arg(commit).arg(expected);
    let output = update.output().await?;
    if output.status.success() {
        return Ok(());
    }
    match git.rev_parse(&target).await? {
        Some(current) if current != expected => Err(MergeError::BaseMoved),
        None => Err(MergeError::BaseMoved),
        Some(_) => Err(failure(&output, "update-ref")),
    }
}

/// Deletes candidate ref `target` of `repo`, whatever it points at; nothing when it is gone.
///
/// # Errors
///
/// A Git failure, or [`MergeError::Failed`] for a ref outside [`CANDIDATE_PREFIX`].
pub async fn drop_candidate(env: &MergeEnvironment, repo: &Path, target: &str) -> Result<(), MergeError> {
    if !target.starts_with(CANDIDATE_PREFIX) {
        return Err(MergeError::Failed(format!("`{target}` is not a merge-queue ref")));
    }
    let git = Git { binary: &env.git_binary, dir: repo };
    if git.rev_parse(target).await?.is_some() {
        git.run(["update-ref", "--no-deref", "-d", "--end-of-options", target]).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_live_under_their_own_prefix() {
        assert_eq!(candidate_ref("main", 12), "refs/merge-queue/main/pr-12");
        assert!(candidate_ref("release/1.0", 3).starts_with(CANDIDATE_PREFIX));
    }
}
