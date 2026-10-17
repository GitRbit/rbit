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

use std::{collections::HashSet, net::IpAddr, path::Path, process::Stdio, time::Duration};

use rbit_core::lfs::{LfsPointer, MAX_POINTER_BYTES};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use crate::{
    GitError,
    archive::{git_command, null_device},
    process::bounded_output,
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
    /// The addresses approved by the caller's outbound policy. Git connects only to
    /// these addresses while retaining the URL's host for HTTP and TLS.
    pub resolution: Option<&'a ResolvedHost>,
}

/// One DNS answer approved before a remote operation. Passing it to Git prevents a
/// second DNS lookup from bypassing the caller's private-address checks.
#[derive(Debug, Clone)]
pub struct ResolvedHost {
    host: String,
    port: u16,
    addresses: Vec<IpAddr>,
}

impl ResolvedHost {
    pub fn new(host: &str, port: u16, addresses: Vec<IpAddr>) -> Result<Self, GitError> {
        // Do not allow curl's wildcard, removal or expiration syntax in the host.
        let valid_host = host.parse::<IpAddr>().is_ok()
            || (!host.is_empty()
                && !host.starts_with('-')
                && host.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')));
        if !valid_host || port == 0 || addresses.is_empty() {
            return Err(failed("invalid resolved remote host"));
        }
        Ok(Self { host: host.to_owned(), port, addresses })
    }

    pub(crate) fn apply(&self, command: &mut tokio::process::Command) {
        // Literal addresses do not use DNS; older libcurl versions cannot use IPv6
        // literals in the HOST part of CURLOPT_RESOLVE.
        if self.host.parse::<IpAddr>().is_ok() {
            return;
        }
        let addresses = self
            .addresses
            .iter()
            .map(|address| match address {
                IpAddr::V4(address) => address.to_string(),
                IpAddr::V6(address) => format!("[{address}]"),
            })
            .collect::<Vec<_>>()
            .join(",");
        command.arg("-c").arg(format!("http.curloptResolve={}:{}:{addresses}", self.host, self.port));
    }
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
        "-c",
        "http.proxy=",
        "-c",
        "http.curloptResolve=",
    ]);
    if let Some(resolution) = remote.resolution {
        resolution.apply(&mut command);
    }
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
    let output = tokio::time::timeout(timeout, bounded_output(command, 0, MAX_ERROR_LEN * 4))
        .await
        .map_err(|_| RemoteError::TimedOut(timeout))?
        .map_err(GitError::from)?;
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
    let mut inventory = list.stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let listed = inventory.stdout.take().ok_or_else(|| failed("cat-file has no inventory"))?;

    let mut read = git_command(git_binary, Some(repo_path));
    read.args(["cat-file", "--batch"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = read.spawn()?;
    let mut stdin = child.stdin.take().ok_or_else(|| failed("cat-file has no input"))?;
    let stdout = child.stdout.take().ok_or_else(|| failed("cat-file has no output"))?;
    let feed = async move {
        // Backpressure connects the inventory directly to the blob reader: neither the
        // complete object listing nor the candidate ids accumulate in memory.
        let mut lines = BufReader::new(listed).lines();
        let max = u64::try_from(MAX_POINTER_BYTES).unwrap_or(u64::MAX);
        while let Some(line) = lines.next_line().await? {
            let mut fields = line.split(' ');
            let (Some(id), Some(kind), Some(size)) = (fields.next(), fields.next(), fields.next()) else { continue };
            if kind == "blob" && size.parse::<u64>().is_ok_and(|size| (MIN_POINTER_BYTES..=max).contains(&size)) {
                stdin.write_all(id.as_bytes()).await?;
                stdin.write_all(b"\n").await?;
            }
        }
        stdin.shutdown().await
    };
    let ((), pointers, read_status, list_status) = tokio::try_join!(feed, read_pointers(stdout), child.wait(), inventory.wait())?;
    for status in [read_status, list_status] {
        if !status.success() {
            return Err(failed(&format!("cat-file failed ({status})")));
        }
    }
    Ok(pointers)
}

async fn read_pointers(stdout: impl tokio::io::AsyncRead + Unpin) -> std::io::Result<Vec<LfsPointer>> {
    let mut reader = BufReader::new(stdout);
    let mut pointers = Vec::new();
    let mut seen = HashSet::new();
    let mut header = String::new();
    loop {
        header.clear();
        if reader.read_line(&mut header).await? == 0 {
            break;
        }
        // `<id> blob <size>` (or `<id> missing`), then the content and a newline.
        let Some(size) = header.trim_end().rsplit(' ').next().and_then(|size| size.parse::<u64>().ok()) else { continue };
        let size = usize::try_from(size).map_err(|_| std::io::Error::other("cat-file returned an oversized pointer candidate"))?;
        if size > MAX_POINTER_BYTES {
            return Err(std::io::Error::other("cat-file returned an oversized pointer candidate"));
        }
        let mut content = vec![0; size + 1];
        reader.read_exact(&mut content).await?;
        content.pop();
        if let Some(pointer) = LfsPointer::parse(&content)
            && seen.insert((pointer.oid.clone(), pointer.size))
        {
            pointers.push(pointer);
        }
    }
    Ok(pointers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_hosts_cannot_inject_curl_resolution_directives() {
        for host in ["", "*", "+example.com", "-example.com", "-example.com:443", "a,b", "a:443:127.0.0.1"] {
            assert!(ResolvedHost::new(host, 443, vec!["93.184.216.34".parse().unwrap()]).is_err(), "{host}");
        }
        assert!(ResolvedHost::new("example.com", 443, Vec::new()).is_err());
        let resolution =
            ResolvedHost::new("example.com", 443, vec!["93.184.216.34".parse().unwrap(), "2606:4700::1111".parse().unwrap()]).unwrap();
        let mut command = tokio::process::Command::new("git");
        resolution.apply(&mut command);
        let args: Vec<_> = command.as_std().get_args().map(|arg| arg.to_string_lossy()).collect();
        assert_eq!(args, ["-c", "http.curloptResolve=example.com:443:93.184.216.34,[2606:4700::1111]"]);
    }

    #[tokio::test]
    async fn oversized_pointer_responses_are_rejected_before_allocation() {
        let response = format!("{} blob 18446744073709551615\n", "a".repeat(40));
        assert!(read_pointers(response.as_bytes()).await.is_err());
    }
}
