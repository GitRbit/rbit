//! Fetching a pull mirror from its upstream with the `git` executable.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! The fetch copies the upstream's branches and tags (`--prune`: refs deleted upstream
//! are deleted here too) and nothing else. Git runs with the scrubbed environment of
//! [`git_command`] plus: no terminal prompts, no credential helpers, only the `http` and
//! `https` protocols, no redirects (a redirect could lead to an address that was never
//! checked), and a timeout after which the process is killed. Which upstream URLs are
//! acceptable at all is decided by the caller.

use std::{path::Path, process::Stdio, time::Duration};

use crate::{GitError, archive::git_command};

/// Longest error excerpt kept from Git's output.
const MAX_ERROR_LEN: usize = 500;

/// Why a mirror fetch failed, in words safe to show to the repository's administrators.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("the fetch took longer than {0:?}")]
    TimedOut(Duration),
    #[error("{0}")]
    Failed(String),
    #[error(transparent)]
    Git(#[from] GitError),
}

/// A `git` command restricted for talking to an upstream (see the module docs).
fn remote_command(git_binary: &Path, repo_path: Option<&Path>) -> tokio::process::Command {
    let mut command = git_command(git_binary, repo_path);
    command.env("GIT_ALLOW_PROTOCOL", "http:https").args([
        "-c",
        "credential.helper=",
        "-c",
        "http.followRedirects=false",
        "-c",
        "core.askPass=",
    ]);
    command
}

/// Runs `command` with `timeout`, returning its standard output.
async fn run(command: &mut tokio::process::Command, timeout: Duration) -> Result<Vec<u8>, FetchError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let output =
        tokio::time::timeout(timeout, command.output()).await.map_err(|_| FetchError::TimedOut(timeout))?.map_err(GitError::from)?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut message: String = stderr.trim().chars().filter(|c| !c.is_control() || *c == '\n').take(MAX_ERROR_LEN).collect();
    if message.is_empty() {
        message = format!("git failed ({})", output.status);
    }
    Err(FetchError::Failed(message))
}

/// Fetches `url` into the bare repository at `repo_path`, replacing its branches and
/// tags with the upstream's.
pub async fn fetch_mirror(git_binary: &Path, repo_path: &Path, url: &str, timeout: Duration) -> Result<(), FetchError> {
    if !tokio::fs::try_exists(repo_path.join("HEAD")).await.map_err(GitError::from)? {
        return Err(GitError::NotFound("repository").into());
    }
    let mut command = remote_command(git_binary, Some(repo_path));
    command
        .args(["fetch", "--quiet", "--prune", "--force", "--no-write-fetch-head", "--"])
        .arg(url)
        .args(["+refs/heads/*:refs/heads/*", "+refs/tags/*:refs/tags/*"]);
    run(&mut command, timeout).await.map(drop)
}

/// The branch the upstream's `HEAD` points at (its default branch), if it says.
pub async fn upstream_default_branch(git_binary: &Path, url: &str, timeout: Duration) -> Result<Option<String>, FetchError> {
    let mut command = remote_command(git_binary, None);
    command.args(["ls-remote", "--symref", "--"]).arg(url).arg("HEAD");
    let stdout = run(&mut command, timeout).await?;
    Ok(parse_symref(&String::from_utf8_lossy(&stdout)))
}

/// `ref: refs/heads/main\tHEAD` → `main`.
fn parse_symref(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (target, name) = line.strip_prefix("ref: ")?.split_once('\t')?;
        (name.trim() == "HEAD").then_some(())?;
        target.strip_prefix("refs/heads/").map(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbolic_head_is_parsed() {
        let output = "ref: refs/heads/trunk\tHEAD\n0123456789abcdef0123456789abcdef01234567\tHEAD\n";
        assert_eq!(parse_symref(output), Some("trunk".to_owned()));
        assert_eq!(parse_symref("0123\tHEAD\n"), None);
    }

    #[tokio::test]
    async fn only_http_upstreams_are_fetched() {
        let dir = tempfile::tempdir().unwrap();
        let storage = crate::RepositoryStorage::new(dir.path());
        let source = storage.create(rustit_core::AccountId(1), rustit_core::RepoId(1), "main").await.unwrap();
        let target = storage.create(rustit_core::AccountId(1), rustit_core::RepoId(2), "main").await.unwrap();
        let local = source.to_string_lossy().into_owned();
        let file_url = format!("file://{}", local.replace('\\', "/"));
        for url in [local.as_str(), file_url.as_str()] {
            let result = fetch_mirror(Path::new("git"), &target, url, Duration::from_secs(30)).await;
            assert!(matches!(result, Err(FetchError::Failed(_))), "{url}: {result:?}");
        }
    }
}
