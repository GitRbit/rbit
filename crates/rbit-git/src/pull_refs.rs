//! `refs/pull/{n}/head` in the base repository: set from a same-repository branch with
//! `update-ref`, fetched from a fork, and refreshed when the head branch moves.
//!
//! Milestone 2 workstream P (pull requests) owns this file.
//!
//! Keeping the head in the base repository means a pull request keeps its commits when the
//! fork is deleted, and merges (which run on the base repository's objects) always find
//! them. Every invocation is a [`crate::merge::hardened`] `git` command: no hooks, no
//! global configuration, `--end-of-options` before revisions.

use std::path::Path;

use crate::{GitError, merge::hardened};

/// `refs/pull/{number}/head`.
pub fn head_ref(number: u32) -> String {
    format!("refs/pull/{number}/head")
}

/// Points `refs/pull/{number}/head` of the repository at `repo` at the commit `id` (a
/// full hexadecimal id already in that repository).
pub async fn set(git_binary: &Path, repo: &Path, number: u32, id: &str) -> Result<(), GitError> {
    validate_id(id)?;
    let output = hardened(git_binary, Some(repo))
        .args(["update-ref", "--no-deref", "-m", "rbit: pull request head", "--end-of-options"])
        .arg(head_ref(number))
        .arg(id)
        .output()
        .await?;
    check(&output, "update-ref")
}

/// Fetches branch `branch` of the repository at `source` (a fork in the same network)
/// into `refs/pull/{number}/head` of the repository at `base`, and returns the commit id
/// it now points at.
pub async fn fetch(git_binary: &Path, base: &Path, source: &Path, branch: &str, number: u32) -> Result<String, GitError> {
    let branch_ref = branch_ref(branch)?;
    let output = hardened(git_binary, Some(base))
        .args(["fetch", "--quiet", "--no-tags", "--no-write-fetch-head", "--no-recurse-submodules", "--"])
        .arg(source)
        .arg(format!("+{branch_ref}:{}", head_ref(number)))
        .output()
        .await?;
    check(&output, "fetch")?;
    read(git_binary, base, number).await?.ok_or(GitError::NotFound("pull request head"))
}

/// The commit `refs/pull/{number}/head` points at, if it exists.
pub async fn read(git_binary: &Path, repo: &Path, number: u32) -> Result<Option<String>, GitError> {
    let output = hardened(git_binary, Some(repo))
        .args(["rev-parse", "--verify", "--quiet", "--end-of-options"])
        .arg(format!("{}^{{commit}}", head_ref(number)))
        .output()
        .await?;
    if !output.status.success() {
        return Ok(None);
    }
    let id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    validate_id(&id)?;
    Ok(Some(id))
}

/// `refs/heads/{branch}` for a valid branch name (never an option).
pub(crate) fn branch_ref(branch: &str) -> Result<String, GitError> {
    let full = format!("refs/heads/{branch}");
    if branch.is_empty() || branch.starts_with('-') || gix::refs::FullName::try_from(full.as_str()).is_err() {
        return Err(GitError::InvalidBranchName(branch.to_owned()));
    }
    Ok(full)
}

/// A full hexadecimal object id (40 or 64 digits), never anything Git could read as an
/// option or an expression.
pub(crate) fn validate_id(id: &str) -> Result<(), GitError> {
    if matches!(id.len(), 40 | 64) && id.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
        Ok(())
    } else {
        Err(GitError::NotFound("commit"))
    }
}

fn check(output: &std::process::Output, what: &str) -> Result<(), GitError> {
    if output.status.success() {
        return Ok(());
    }
    Err(GitError::Io(std::io::Error::other(format!(
        "git {what} failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    ))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_ids_are_validated() {
        assert_eq!(head_ref(12), "refs/pull/12/head");
        assert_eq!(branch_ref("feature/x").unwrap(), "refs/heads/feature/x");
        for bad in ["", "-x", "a..b", "with space", "x.lock"] {
            assert!(branch_ref(bad).is_err(), "{bad:?}");
        }
        assert!(validate_id(&"a".repeat(40)).is_ok());
        for bad in ["HEAD", "--all", &"A".repeat(40), &"a".repeat(39)] {
            assert!(validate_id(bad).is_err(), "{bad:?}");
        }
    }
}
