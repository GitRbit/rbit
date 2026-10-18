//! Creating a fork's bare repository from its parent on disk.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! `git clone --bare --local` hard-links the parent's object files where the filesystem
//! allows (objects are immutable, so sharing them is safe and makes forks nearly free)
//! and copies only branches and tags. The clone's `origin` remote is removed: a fork is
//! an independent repository, and its configuration must not point at another
//! repository's path.

use std::{path::Path, process::Stdio, time::Duration};

use crate::{GitError, archive::git_command};

/// Upper bound for one clone; a fork of a huge repository taking longer is abandoned.
const FORK_TIMEOUT: Duration = Duration::from_mins(10);

/// Clones the bare repository at `source` into `target`, which must not exist yet. On
/// failure, whatever was created at `target` is removed again.
pub async fn fork_repository(git_binary: &Path, source: &Path, target: &Path) -> Result<(), GitError> {
    if tokio::fs::try_exists(target).await? {
        return Err(GitError::AlreadyExists);
    }
    if !tokio::fs::try_exists(source.join("HEAD")).await? {
        return Err(GitError::NotFound("repository"));
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let result = clone(git_binary, source, target).await;
    if result.is_err() {
        let _ = tokio::fs::remove_dir_all(target).await;
    }
    result
}

async fn clone(git_binary: &Path, source: &Path, target: &Path) -> Result<(), GitError> {
    run(git_command(git_binary, None).args(["clone", "--bare", "--local", "--quiet", "--"]).arg(source).arg(target)).await?;
    run(git_command(git_binary, Some(target)).args(["remote", "remove", "origin"])).await
}

async fn run(command: &mut tokio::process::Command) -> Result<(), GitError> {
    let output = tokio::time::timeout(FORK_TIMEOUT, command.stdout(Stdio::null()).stderr(Stdio::piped()).output())
        .await
        .map_err(|_| GitError::Io(std::io::Error::new(std::io::ErrorKind::TimedOut, "git clone timed out")))??;
    if output.status.success() {
        Ok(())
    } else {
        Err(GitError::Io(std::io::Error::other(format!(
            "git failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))))
    }
}
