//! Source archives (`zip`, `tar.gz`) of a commit, streamed from `git archive`.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! `git archive` writes the archive to its standard output, which is handed to the caller
//! as an [`AsyncRead`] without buffering the whole archive; `tar.gz` is compressed
//! in-process while streaming. Like every Git child process Rbit runs, `git` sees a
//! scrubbed environment and no system or global configuration ([`git_command`]).
//!
//! The child is killed as soon as the reader is dropped (a client that disconnects never
//! leaves `git archive` running), and a non-zero exit status turns the end of the stream
//! into an I/O error, so a failed archive is never mistaken for a complete one.

use std::{
    ffi::OsStr,
    future::Future,
    io,
    path::{Path, PathBuf},
    pin::Pin,
    process::{ExitStatus, Stdio},
    task::{Context, Poll, ready},
};

use async_compression::tokio::bufread::GzipEncoder;
use tokio::{
    io::{AsyncRead, BufReader, ReadBuf},
    process::{ChildStdout, Command},
    sync::oneshot,
};

use crate::GitError;

/// Environment variables passed through to Git; everything else is cleared.
const INHERITED_ENV: [&str; 5] = ["PATH", "SYSTEMROOT", "TEMP", "TMP", "TMPDIR"];

/// A `git` command with a scrubbed environment and isolated configuration: no system or
/// global config, no terminal prompts. With `git_dir`, it operates on that repository.
/// The child is killed when its handle is dropped.
pub(crate) fn git_command(git_binary: &Path, git_dir: Option<&Path>) -> Command {
    let mut command = Command::new(git_binary);
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
        .stdin(Stdio::null())
        .kill_on_drop(true);
    if let Some(dir) = git_dir {
        command.arg("--git-dir").arg(dir);
    }
    command
}

pub(crate) fn null_device() -> &'static OsStr {
    OsStr::new(if cfg!(windows) { "NUL" } else { "/dev/null" })
}

/// The archive formats offered for download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    TarGz,
}

impl ArchiveFormat {
    pub const ALL: [Self; 2] = [Self::Zip, Self::TarGz];

    /// The file name suffix, including the leading dot.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Zip => ".zip",
            Self::TarGz => ".tar.gz",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Self::Zip => "application/zip",
            Self::TarGz => "application/gzip",
        }
    }

    /// Splits `v1.0.tar.gz` into (`v1.0`, [`Self::TarGz`]).
    pub fn split_suffix(name: &str) -> Option<(&str, Self)> {
        Self::ALL
            .into_iter()
            .find_map(|format| name.strip_suffix(format.extension()).filter(|rest| !rest.is_empty()).map(|rest| (rest, format)))
    }

    fn git_format(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarGz => "tar",
        }
    }
}

/// Runs `git archive` for repositories on disk.
#[derive(Debug, Clone)]
pub struct Archiver {
    git_binary: PathBuf,
}

/// An archive being produced: read it to the end to receive every byte.
pub type ArchiveStream = Pin<Box<dyn AsyncRead + Send>>;

impl Archiver {
    pub fn new(git_binary: impl Into<PathBuf>) -> Self {
        Self { git_binary: git_binary.into() }
    }

    /// Starts archiving `commit` (a full commit id) of the bare repository at
    /// `repo_path`, with every entry under `prefix/`.
    ///
    /// `commit` must be a full hexadecimal id (resolve names first): it is passed to
    /// `git` as an argument, and anything else is refused so it can never be read as an
    /// option.
    pub fn start(&self, repo_path: &Path, commit: &str, prefix: &str, format: ArchiveFormat) -> Result<ArchiveStream, GitError> {
        let is_full_id = matches!(commit.len(), 40 | 64) && commit.bytes().all(|b| b.is_ascii_hexdigit());
        if !is_full_id {
            return Err(GitError::NotFound("commit"));
        }
        let mut child = git_command(&self.git_binary, Some(repo_path))
            .arg("archive")
            .arg(format!("--format={}", format.git_format()))
            .arg(format!("--prefix={prefix}/"))
            .arg(commit)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child.stdout.take().ok_or_else(|| io::Error::other("git archive has no standard output"))?;

        // The child is owned by a task that reports its exit status, or kills it once the
        // reader (and with it `cancel`) is dropped before git finished.
        let (status_tx, status_rx) = oneshot::channel();
        let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let finished = tokio::select! {
                status = child.wait() => Some(status),
                _ = cancel_rx => None,
            };
            match finished {
                Some(status) => {
                    let _ = status_tx.send(status);
                }
                None => {
                    let _ = child.kill().await;
                }
            }
        });

        let reader = ProcessReader { stdout, status: status_rx, _cancel: cancel_tx };
        Ok(match format {
            ArchiveFormat::Zip => Box::pin(reader),
            ArchiveFormat::TarGz => Box::pin(GzipEncoder::new(BufReader::new(reader))),
        })
    }
}

/// A child's standard output whose end of file is only reported once the child exited
/// successfully.
struct ProcessReader {
    stdout: ChildStdout,
    status: oneshot::Receiver<io::Result<ExitStatus>>,
    /// Dropping the reader drops this, which tells the owning task to kill the child.
    _cancel: oneshot::Sender<()>,
}

impl AsyncRead for ProcessReader {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        ready!(Pin::new(&mut self.stdout).poll_read(cx, buf))?;
        if buf.filled().len() > before {
            return Poll::Ready(Ok(()));
        }
        // End of output: succeed only if git did.
        match ready!(Pin::new(&mut self.status).poll(cx)) {
            Ok(Ok(status)) if status.success() => Poll::Ready(Ok(())),
            Ok(Ok(status)) => Poll::Ready(Err(io::Error::other(format!("git archive failed ({status})")))),
            Ok(Err(error)) => Poll::Ready(Err(error)),
            Err(_) => Poll::Ready(Err(io::Error::other("git archive was interrupted"))),
        }
    }
}

impl std::fmt::Debug for ProcessReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessReader").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffixes_are_split_from_revisions() {
        assert_eq!(ArchiveFormat::split_suffix("v1.0.zip"), Some(("v1.0", ArchiveFormat::Zip)));
        assert_eq!(ArchiveFormat::split_suffix("feature/x.tar.gz"), Some(("feature/x", ArchiveFormat::TarGz)));
        assert_eq!(ArchiveFormat::split_suffix("main.tar"), None);
        assert_eq!(ArchiveFormat::split_suffix(".zip"), None);
    }

    #[test]
    fn only_full_commit_ids_are_archived() {
        let archiver = Archiver::new("git");
        for bad in ["main", "--output=/tmp/x", "abc123"] {
            assert!(matches!(archiver.start(Path::new("."), bad, "x", ArchiveFormat::Zip), Err(GitError::NotFound(_))), "{bad}");
        }
    }
}
