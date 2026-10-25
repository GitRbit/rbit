//! Repository housekeeping with the `git` executable: `git gc --auto` and
//! `git commit-graph write`.
//!
//! gitoxide reads repositories but does not repack them, so maintenance delegates to Git
//! itself. `--auto` makes it cheap to call often: Git only repacks when loose objects or
//! packs exceed its thresholds, and otherwise exits immediately. Below those thresholds it
//! never writes a commit-graph either, so that is written separately, incrementally.
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

/// Upper bound for one housekeeping command; a repack that takes longer is killed and
/// retried at the next maintenance run.
const GIT_TIMEOUT: Duration = Duration::from_mins(30);

/// Environment variables passed through to Git; everything else is cleared.
const INHERITED_ENV: [&str; 5] = ["PATH", "SYSTEMROOT", "TEMP", "TMP", "TMPDIR"];

#[derive(Debug, thiserror::Error)]
pub enum MaintenanceError {
    #[error("failed to run git: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("git housekeeping timed out after {0:?}")]
    TimedOut(Duration),
    #[error("git housekeeping failed ({status}): {stderr}")]
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
        self.run(repo_path, &["-c", "gc.autoDetach=false", "gc", "--auto", "--quiet"]).await
    }

    /// Brings the repository's commit-graph up to date: `git commit-graph write --reachable
    /// --split`. It adds the commits reachable from the references that the graph does not
    /// cover yet as a new layer (merging layers as Git sees fit), so history walks and
    /// [last-commit](crate::last_commit) lookups read parents and root trees from it
    /// instead of decoding commit objects. A repository without commits gets no graph.
    ///
    /// Git replaces graph files by renaming over them and removes merged layers, which
    /// Windows refuses while another process has them mapped, so callers hold the
    /// repository's lease as for [`Self::gc_auto`].
    pub async fn write_commit_graph(&self, repo_path: &Path) -> Result<(), MaintenanceError> {
        self.run(repo_path, &["commit-graph", "write", "--reachable", "--split", "--no-progress"]).await
    }

    /// Runs `git <args>` in the bare repository at `repo_path`, bounded by [`GIT_TIMEOUT`].
    async fn run(&self, repo_path: &Path, args: &[&str]) -> Result<(), MaintenanceError> {
        if !repo_path.join("HEAD").is_file() {
            return Err(MaintenanceError::NotARepository(repo_path.to_owned()));
        }
        let mut command = self.command(repo_path);
        command.args(args);
        let output = tokio::time::timeout(GIT_TIMEOUT, command.output())
            .await
            .map_err(|_| MaintenanceError::TimedOut(GIT_TIMEOUT))?
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
        let result = Maintenance::new("git").write_commit_graph(dir.path()).await;
        assert!(matches!(result, Err(MaintenanceError::NotARepository(_))));
    }

    #[tokio::test]
    async fn writes_a_commit_graph_that_gitoxide_reads() {
        let dir = tempfile::tempdir().unwrap();
        let path = RepositoryStorage::new(dir.path()).create(AccountId(1), RepoId(1), "main").await.unwrap();
        let maintenance = Maintenance::new("git");
        maintenance.write_commit_graph(&path).await.unwrap();
        let graph = |path: &Path| gix::open_opts(path, gix::open::Options::isolated()).unwrap().commit_graph_if_enabled().unwrap();
        assert!(graph(&path).is_none(), "no commits, no graph");

        let work = tempfile::tempdir().unwrap();
        for args in [
            &["init", "-q"][..],
            &["commit", "-q", "--allow-empty", "-m", "one"],
            &["push", "-q", path.to_str().unwrap(), "HEAD:refs/heads/main"],
        ] {
            let status = std::process::Command::new("git")
                .current_dir(work.path())
                .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main"])
                .args(args)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        }
        maintenance.write_commit_graph(&path).await.unwrap();
        // Writing again with nothing new is harmless.
        maintenance.write_commit_graph(&path).await.unwrap();
        let graph = graph(&path).expect("a graph after commits were pushed");
        assert_eq!(graph.num_commits(), 1);
    }

    #[tokio::test]
    async fn reports_a_missing_git_binary() {
        let dir = tempfile::tempdir().unwrap();
        let path = RepositoryStorage::new(dir.path()).create(AccountId(1), RepoId(1), "main").await.unwrap();
        let result = Maintenance::new(dir.path().join("no-such-git")).gc_auto(&path).await;
        assert!(matches!(result, Err(MaintenanceError::Spawn(_))));
    }
}
