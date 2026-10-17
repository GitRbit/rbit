//! Talking to remotes with credentials, with the `git` executable: copying a repository
//! for an import ([`fetch_import`]), pushing a push mirror ([`push_mirror`]), and finding
//! the Git LFS objects a repository's history refers to ([`lfs_pointers`]).
//!
//! Milestone 3 workstream M (importing and push mirrors) owns this file.
//!
//! **Credentials never appear in arguments or URLs.** The caller passes a complete
//! `Authorization` header value ([`Remote::authorization`]); git receives it as
//! `http.extraHeader` through the environment (`GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_0`,
//! `GIT_CONFIG_VALUE_0`), which other users of the machine cannot read (unlike the command
//! line) and which git neither prints nor stores in the repository's configuration.
//!
//! Git runs with the scrubbed environment of [`git_command`] plus: no terminal prompts,
//! no credential helpers or askpass programs, no redirects (a redirect could lead to an
//! address that was never checked, and would carry the header along), only the `http` and
//! `https` protocols — and `file` when the caller allows local URLs (a test switch) — and
//! a timeout after which the process is killed. Which remotes are acceptable at all
//! (addresses, schemes) is decided by the caller.

use std::{path::Path, process::Stdio, time::Duration};

use rbit_core::lfs::{LfsPointer, MAX_POINTER_BYTES};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use crate::{
    GitError,
    archive::{git_command, null_device},
};

/// Longest error excerpt kept from Git's output.
const MAX_ERROR_LEN: usize = 500;
/// The smallest pointer file there can be (version, oid and size lines).
const MIN_POINTER_BYTES: u64 = 100;

/// A remote to fetch from or push to.
#[derive(Clone, Copy)]
pub struct Remote<'a> {
    pub url: &'a str,
    /// The `Authorization` header value to send (`Basic …`), if any.
    pub authorization: Option<&'a str>,
    /// Allow `file://` URLs (tests only).
    pub allow_local: bool,
}

impl std::fmt::Debug for Remote<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The header is a secret.
        f.debug_struct("Remote").field("url", &self.url).field("authorization", &self.authorization.map(|_| "…")).finish()
    }
}

/// Why talking to a remote failed, in words safe to show to the people who configured it.
#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error("git took longer than {0:?}")]
    TimedOut(Duration),
    #[error("{0}")]
    Failed(String),
    #[error(transparent)]
    Git(#[from] GitError),
}

/// A `git` command restricted for talking to `remote` (see the module docs).
fn remote_command(git_binary: &Path, repo_path: &Path, remote: Remote<'_>) -> tokio::process::Command {
    let mut command = git_command(git_binary, Some(repo_path));
    command.env("GIT_ALLOW_PROTOCOL", if remote.allow_local { "http:https:file" } else { "http:https" }).args([
        "-c",
        "credential.helper=",
        "-c",
        "http.followRedirects=false",
        "-c",
        "core.askPass=",
    ]);
    command.arg("-c").arg(hooks_path_option());
    if let Some(header) = remote.authorization {
        command
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "http.extraHeader")
            .env("GIT_CONFIG_VALUE_0", format!("Authorization: {header}"));
    }
    command
}

/// `core.hooksPath=<null device>`: no hook runs, whatever the repository says.
fn hooks_path_option() -> std::ffi::OsString {
    let mut option = std::ffi::OsString::from("core.hooksPath=");
    option.push(null_device());
    option
}

/// A failure of a local `git` command (not the remote's fault).
fn failed(message: &str) -> GitError {
    GitError::backend(std::io::Error::other(message.to_owned()))
}

/// Runs `command` with `timeout`.
async fn run(command: &mut tokio::process::Command, timeout: Duration) -> Result<(), RemoteError> {
    command.stdout(Stdio::null()).stderr(Stdio::piped());
    let output =
        tokio::time::timeout(timeout, command.output()).await.map_err(|_| RemoteError::TimedOut(timeout))?.map_err(GitError::from)?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut message: String = stderr.trim().chars().filter(|c| !c.is_control() || *c == '\n').take(MAX_ERROR_LEN).collect();
    if message.is_empty() {
        message = format!("git failed ({})", output.status);
    }
    Err(RemoteError::Failed(message))
}

async fn require_repository(repo_path: &Path) -> Result<(), RemoteError> {
    if tokio::fs::try_exists(repo_path.join("HEAD")).await.map_err(GitError::from)? {
        Ok(())
    } else {
        Err(GitError::NotFound("repository").into())
    }
}

/// Fetches the branches and tags of `remote` into the bare repository at `repo_path`
/// (replacing what is there), and with `pull_heads` also its pull request heads
/// (`refs/pull/{n}/head`, which GitHub and Gitea both publish).
pub async fn fetch_import(
    git_binary: &Path,
    repo_path: &Path,
    remote: Remote<'_>,
    pull_heads: bool,
    timeout: Duration,
) -> Result<(), RemoteError> {
    require_repository(repo_path).await?;
    let mut command = remote_command(git_binary, repo_path, remote);
    command
        .args(["fetch", "--quiet", "--prune", "--force", "--no-write-fetch-head", "--"])
        .arg(remote.url)
        .args(["+refs/heads/*:refs/heads/*", "+refs/tags/*:refs/tags/*"]);
    if pull_heads {
        command.arg("+refs/pull/*/head:refs/pull/*/head");
    }
    run(&mut command, timeout).await
}

/// Pushes every branch and tag of the bare repository at `repo_path` to `remote`,
/// overwriting what is there (`--force`) and deleting the remote's branches and tags that
/// do not exist here (`--prune`). Pull request heads and other refs stay local.
pub async fn push_mirror(git_binary: &Path, repo_path: &Path, remote: Remote<'_>, timeout: Duration) -> Result<(), RemoteError> {
    require_repository(repo_path).await?;
    let mut command = remote_command(git_binary, repo_path, remote);
    command
        .args(["push", "--quiet", "--force", "--prune", "--no-verify", "--"])
        .arg(remote.url)
        .args(["+refs/heads/*:refs/heads/*", "+refs/tags/*:refs/tags/*"]);
    run(&mut command, timeout).await
}

/// The Git LFS pointer files among the objects of the repository at `repo_path` (each
/// object once, in no particular order): every blob small enough to be a pointer is read
/// and parsed with [`LfsPointer::parse`].
pub async fn lfs_pointers(git_binary: &Path, repo_path: &Path) -> Result<Vec<LfsPointer>, GitError> {
    let mut list = git_command(git_binary, Some(repo_path));
    list.args(["cat-file", "--batch-all-objects", "--unordered", "--batch-check=%(objectname) %(objecttype) %(objectsize)"]);
    let output = list.stdout(Stdio::piped()).stderr(Stdio::piped()).output().await?;
    if !output.status.success() {
        return Err(failed(String::from_utf8_lossy(&output.stderr).trim()));
    }
    let max = u64::try_from(MAX_POINTER_BYTES).unwrap_or(u64::MAX);
    let candidates: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split(' ');
            let (id, kind, size) = (fields.next()?, fields.next()?, fields.next()?.parse::<u64>().ok()?);
            (kind == "blob" && (MIN_POINTER_BYTES..=max).contains(&size)).then(|| id.to_owned())
        })
        .collect();
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let mut read = git_command(git_binary, Some(repo_path));
    read.args(["cat-file", "--batch"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = read.spawn()?;
    let mut stdin = child.stdin.take().ok_or_else(|| failed("cat-file has no input"))?;
    let stdout = child.stdout.take().ok_or_else(|| failed("cat-file has no output"))?;
    let writer = tokio::spawn(async move {
        for id in candidates {
            stdin.write_all(id.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
        }
        stdin.shutdown().await
    });
    let mut reader = BufReader::new(stdout);
    let mut pointers = Vec::new();
    let mut header = String::new();
    loop {
        header.clear();
        if reader.read_line(&mut header).await? == 0 {
            break;
        }
        // `<id> blob <size>` (or `<id> missing`), then the content and a newline.
        let Some(size) = header.trim_end().rsplit(' ').next().and_then(|size| size.parse::<usize>().ok()) else { continue };
        let mut content = vec![0; size + 1];
        reader.read_exact(&mut content).await?;
        content.pop();
        if let Some(pointer) = LfsPointer::parse(&content)
            && !pointers.contains(&pointer)
        {
            pointers.push(pointer);
        }
    }
    writer.await??;
    let status = child.wait().await?;
    if !status.success() {
        return Err(failed(&format!("cat-file failed ({status})")));
    }
    Ok(pointers)
}
