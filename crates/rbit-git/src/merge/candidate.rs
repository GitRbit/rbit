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
    let expected = ref_target(&repo, target).await?.unwrap_or_else(|| "0".repeat(spec.base_tip.len()));
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
    update
        .args(["update-ref", "--no-deref", "-m", "rbit: land merge queue candidate", "--end-of-options"])
        .arg(&target)
        .arg(commit)
        .arg(expected);
    let output = update.output().await?;
    if output.status.success() {
        return Ok(());
    }
    match ref_target(&git, &target).await? {
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
    if ref_target(&git, target).await?.is_some() {
        git.run(["update-ref", "--no-deref", "-d", "--end-of-options", target]).await?;
    }
    Ok(())
}

/// Why a branch with a reserved name is refused.
pub const RESERVED_BRANCH: &str = "branch names starting with `refs/` or `merge-queue/` are reserved";

/// Whether full ref `name` is a branch named like a ref rbit manages (`refs/heads/refs/…`,
/// `refs/heads/merge-queue/…`): loose name resolution would read it as that ref, and
/// workflows would see a candidate's branch.
#[must_use]
pub fn reserved_branch(name: &str) -> bool {
    name.strip_prefix("refs/heads/").is_some_and(|branch| branch.starts_with("refs/") || branch.starts_with("merge-queue/"))
}

/// The commit the full ref `name` (`refs/…`) points at, looked up exactly. `rev-parse`
/// would also take a branch whose own name is `name` (`refs/heads/refs/…`), which anyone who
/// may push can create.
pub(super) async fn ref_target(git: &Git<'_>, name: &str) -> Result<Option<String>, MergeError> {
    let output = git.command().args(["show-ref", "--verify", "--hash"]).arg(name).output().await?;
    if output.status.success() {
        return Ok(Some(String::from_utf8_lossy(&output.stdout).trim().to_owned()));
    }
    // Git may report both a missing ref and an operational failure with status
    // 128 here. Quiet verification distinguishes missing (1) without parsing
    // stderr. Keep the successful lookup at one process; preserve the original
    // diagnostic for every other outcome, including a ref appearing meanwhile.
    let verification = git.command().args(["show-ref", "--verify", "--quiet"]).arg(name).output().await;
    if verification.is_ok_and(|verification| verification.status.code() == Some(1)) { Ok(None) } else { Err(failure(&output, "show-ref")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_live_under_their_own_prefix() {
        assert_eq!(candidate_ref("main", 12), "refs/merge-queue/main/pr-12");
        assert!(candidate_ref("release/1.0", 3).starts_with(CANDIDATE_PREFIX));
    }

    #[test]
    fn branches_named_like_managed_refs_are_reserved() {
        assert!(reserved_branch("refs/heads/refs/merge-queue/main/pr-1"), "read loosely as the candidate itself");
        assert!(reserved_branch("refs/heads/merge-queue/main/pr-1"), "a candidate's branch name to workflows");
        assert!(!reserved_branch("refs/heads/main") && !reserved_branch("refs/heads/feature/refs"));
        assert!(!reserved_branch("refs/merge-queue/main/pr-1"), "the candidate ref itself is no branch");
    }

    async fn control_commit(git: &Git<'_>, message: &str) -> String {
        let output =
            git.command().args(["hash-object", "-w", "-t", "tree", "--stdin"]).stdin(std::process::Stdio::null()).output().await.unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let tree = String::from_utf8(output.stdout).unwrap();
        let output = git
            .run(["-c", "user.name=Fixture", "-c", "user.email=fixture@example.com", "commit-tree", tree.trim(), "-m", message])
            .await
            .unwrap();
        String::from_utf8(output).unwrap().trim().to_owned()
    }

    #[tokio::test]
    async fn a_missing_exact_ref_has_no_target() {
        let directory = tempfile::tempdir().unwrap();
        gix::init_bare(directory.path()).unwrap();
        let git = Git { binary: Path::new("git"), dir: directory.path() };
        assert_eq!(ref_target(&git, "refs/heads/missing").await.unwrap(), None);
        let commit = control_commit(&git, "candidate").await;
        let env = MergeEnvironment {
            git_binary: "git".into(),
            tmp_dir: directory.path().join("tmp"),
            timeout: std::time::Duration::from_secs(5),
        };
        assert!(matches!(land(&env, directory.path(), "missing", &commit, &commit).await, Err(MergeError::BaseMoved)));
    }

    #[tokio::test]
    async fn exact_ref_lookup_ignores_a_branch_named_like_the_target() {
        let directory = tempfile::tempdir().unwrap();
        gix::init_bare(directory.path()).unwrap();
        let git = Git { binary: Path::new("git"), dir: directory.path() };
        let target = "refs/merge-queue/main/pr-1";
        let shadow = control_commit(&git, "shadow").await;
        git.run(["update-ref", &format!("refs/heads/{target}"), &shadow]).await.unwrap();
        assert_eq!(ref_target(&git, target).await.unwrap(), None);
        let exact = control_commit(&git, "exact").await;
        assert_ne!(exact, shadow);
        git.run(["update-ref", target, &exact]).await.unwrap();
        assert_eq!(ref_target(&git, target).await.unwrap(), Some(exact));
    }

    #[tokio::test]
    async fn native_lookup_failure_is_not_a_missing_ref_or_moved_base() {
        let directory = tempfile::tempdir().unwrap();
        let git = Git { binary: Path::new("git"), dir: directory.path() };
        assert!(
            matches!(ref_target(&git, "refs/heads/main").await, Err(MergeError::Failed(_))),
            "a plain directory is an operational Git failure"
        );
        let env = MergeEnvironment {
            git_binary: "git".into(),
            tmp_dir: directory.path().join("tmp"),
            timeout: std::time::Duration::from_secs(5),
        };
        let result = land(&env, directory.path(), "main", &"1".repeat(40), &"2".repeat(40)).await;
        assert!(matches!(result, Err(MergeError::Failed(_))), "failed native lookup must not trigger a BaseMoved retry: {result:?}");
    }
}
