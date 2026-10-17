//! Repository housekeeping with the `git` executable: `git gc --auto`.
//!
//! gitoxide reads repositories but does not repack them, so maintenance delegates to Git
//! itself. `--auto` makes it cheap to call often: Git only repacks when loose objects or
//! packs exceed its thresholds, and otherwise exits immediately.
//!
//! Like the smart-HTTP transport, the child runs with a scrubbed environment and without
//! system or global configuration, so an operator's `~/.gitconfig` (or an attacker's
//! environment) cannot change what runs. Detached auto-gc is disabled so the caller
//! knows when the work is actually done and can bound how much runs at once.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use tokio::process::Command;

/// Upper bound for one `git gc --auto`; a repack that takes longer is killed and retried
/// at the next maintenance run.
const GC_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Environment variables passed through to Git; everything else is cleared.
const INHERITED_ENV: [&str; 5] = ["PATH", "SYSTEMROOT", "TEMP", "TMP", "TMPDIR"];

#[derive(Debug, thiserror::Error)]
pub enum MaintenanceError {
    #[error("failed to run git: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("git gc timed out after {0:?}")]
    TimedOut(Duration),
    #[error("git gc failed ({status}): {stderr}")]
    Failed { status: std::process::ExitStatus, stderr: String },
    #[error("not a repository: {0}")]
    NotARepository(PathBuf),
}

/// Runs housekeeping commands against bare repositories.
#[derive(Debug, Clone)]
pub struct Maintenance {
    git_binary: PathBuf,
}

impl Maintenance {
    pub fn new(git_binary: impl Into<PathBuf>) -> Self {
        Self { git_binary: git_binary.into() }
    }

    /// Runs `git gc --auto --quiet` in the bare repository at `repo_path`.
    pub async fn gc_auto(&self, repo_path: &Path) -> Result<(), MaintenanceError> {
        if !repo_path.join("HEAD").is_file() {
            return Err(MaintenanceError::NotARepository(repo_path.to_owned()));
        }
        let mut command = self.command(repo_path);
        command.args(["-c", "gc.autoDetach=false", "gc", "--auto", "--quiet"]);
        let output = tokio::time::timeout(GC_TIMEOUT, command.output())
            .await
            .map_err(|_| MaintenanceError::TimedOut(GC_TIMEOUT))?
            .map_err(MaintenanceError::Spawn)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(MaintenanceError::Failed { status: output.status, stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned() })
        }
    }

    /// A `git --git-dir <repo>` command with a scrubbed environment and isolated config.
    fn command(&self, repo_path: &Path) -> Command {
        let mut command = Command::new(&self.git_binary);
        command.env_clear();
        for key in INHERITED_ENV {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", null_device())
            .env("GIT_TERMINAL_PROMPT", "0")
            .arg("--git-dir")
            .arg(repo_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command
    }
}

fn null_device() -> &'static OsStr {
    OsStr::new(if cfg!(windows) { "NUL" } else { "/dev/null" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RepositoryStorage;
    use rbit_core::{AccountId, RepoId};

    #[tokio::test]
    async fn gc_auto_succeeds_on_a_fresh_repository() {
        let dir = tempfile::tempdir().unwrap();
        let path = RepositoryStorage::new(dir.path()).create(AccountId(1), RepoId(1), "main").await.unwrap();
        Maintenance::new("git").gc_auto(&path).await.unwrap();
    }

    #[tokio::test]
    async fn refuses_paths_that_are_not_repositories() {
        let dir = tempfile::tempdir().unwrap();
        let result = Maintenance::new("git").gc_auto(dir.path()).await;
        assert!(matches!(result, Err(MaintenanceError::NotARepository(_))));
    }

    #[tokio::test]
    async fn reports_a_missing_git_binary() {
        let dir = tempfile::tempdir().unwrap();
        let path = RepositoryStorage::new(dir.path()).create(AccountId(1), RepoId(1), "main").await.unwrap();
        let result = Maintenance::new(dir.path().join("no-such-git")).gc_auto(&path).await;
        assert!(matches!(result, Err(MaintenanceError::Spawn(_))));
    }
}
