//! Smart-protocol backend that delegates to the `git` executable.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
};

use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, BufReader},
    process::{ChildStdout, Command},
    sync::oneshot,
    task::JoinHandle,
};

use crate::{Service, pkt_line};

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("failed to run git: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("git {service} exited with {status}")]
    Failed { service: Service, status: ExitStatus },
    #[error("I/O error talking to git: {0}")]
    Io(#[from] std::io::Error),
}

/// A running `git <service> --stateless-rpc` exchange.
#[derive(Debug)]
pub struct ServiceSession {
    /// The response body to stream back to the client.
    pub output: ChildStdout,
    /// Resolves with the exit status once git finishes; use it to trigger post-push work.
    pub completion: oneshot::Receiver<ExitStatus>,
}

/// Runs Git services as child processes with a scrubbed environment.
#[derive(Debug, Clone)]
pub struct ProcessTransport {
    git_binary: PathBuf,
}

impl ProcessTransport {
    pub fn new(git_binary: impl Into<PathBuf>) -> Self {
        Self { git_binary: git_binary.into() }
    }

    /// Builds `git <service>` ignoring system/global configuration and inheriting only the
    /// environment variables Git needs to run. The process is killed if its handle is
    /// dropped, so an abandoned exchange never leaves git running.
    ///
    /// Arguments are passed directly to the executable; no shell is ever involved.
    fn service_command(&self, service: Service, protocol: Option<&str>) -> Command {
        let mut command = Command::new(&self.git_binary);
        command.env_clear();
        for key in ["PATH", "SYSTEMROOT", "TEMP", "TMP", "TMPDIR"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command.env("GIT_CONFIG_NOSYSTEM", "1").env("GIT_CONFIG_GLOBAL", null_device()).env("GIT_TERMINAL_PROMPT", "0");
        if let Some(protocol) = protocol.filter(|p| is_safe_protocol_header(p)) {
            command.env("GIT_PROTOCOL", protocol);
        }
        command.arg(service.subcommand()).kill_on_drop(true);
        command
    }

    /// `git <service> --stateless-rpc <repo>`, for HTTP's request/response exchanges.
    fn command(&self, service: Service, repo_path: &Path, protocol: Option<&str>) -> Command {
        let mut command = self.service_command(service, protocol);
        command.arg("--stateless-rpc").arg(repo_path);
        command
    }

    /// Produces the body for `GET …/info/refs?service=…`.
    ///
    /// For protocol v0/v1 the advertisement is prefixed with the `# service=` header; v2
    /// clients expect the capability advertisement directly.
    pub async fn advertise_refs(&self, service: Service, repo_path: &Path, protocol: Option<&str>) -> Result<Vec<u8>, TransportError> {
        let output = self
            .command(service, repo_path, protocol)
            .arg("--advertise-refs")
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(TransportError::Spawn)?;

        if !output.status.success() {
            tracing::warn!(%service, stderr = %String::from_utf8_lossy(&output.stderr), "ref advertisement failed");
            return Err(TransportError::Failed { service, status: output.status });
        }

        let is_v2 = protocol.is_some_and(|p| p.split(':').any(|kv| kv == "version=2"));
        let mut body = Vec::with_capacity(output.stdout.len() + 64);
        if !is_v2 {
            body.extend(pkt_line::encode(&format!("# service={}\n", service.wire_name())));
            body.extend_from_slice(pkt_line::FLUSH);
        }
        body.extend(output.stdout);
        Ok(body)
    }

    /// Starts `POST …/git-upload-pack` or `…/git-receive-pack`: streams `request` into git's
    /// stdin and returns git's stdout for streaming back to the client.
    pub fn serve(
        &self,
        service: Service,
        repo_path: &Path,
        protocol: Option<&str>,
        mut request: impl AsyncRead + Send + Unpin + 'static,
    ) -> Result<ServiceSession, TransportError> {
        let mut child = self
            .command(service, repo_path, protocol)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(TransportError::Spawn)?;

        let mut stdin = child.stdin.take().expect("stdin is piped");
        let output = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");

        tokio::spawn(async move {
            if let Err(error) = tokio::io::copy(&mut request, &mut stdin).await {
                tracing::debug!(%service, %error, "client request body ended early");
            }
            // Dropping stdin signals end-of-request to git.
        });

        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!(%service, "git: {line}");
            }
        });

        let (done, completion) = oneshot::channel();
        tokio::spawn(async move {
            match child.wait().await {
                Ok(status) => {
                    let _ = done.send(status);
                }
                Err(error) => tracing::warn!(%service, %error, "failed to wait for git"),
            }
        });

        Ok(ServiceSession { output, completion })
    }
}

impl ProcessTransport {
    /// Runs `git <service> <repo>` in interactive mode, as Git does over SSH: one long
    /// bidirectional conversation instead of HTTP's stateless request/response pairs.
    ///
    /// `input` (the client's stream) is copied to git's stdin, git's stdout to `output`
    /// and its stderr to `errors` (SSH sends it as extended data, which `git` prints as
    /// `remote: …`). Everything streams; nothing is buffered beyond the copy buffers.
    /// Returns git's exit status once it has exited and both of its output streams have
    /// been forwarded in full. `input` may stay open after git exits (clients often keep
    /// the channel open); it is simply abandoned then.
    ///
    /// Dropping the returned future kills git, so callers can bound the exchange with
    /// `tokio::time::timeout`.
    pub async fn serve_interactive<I, O, E>(
        &self,
        service: Service,
        repo_path: &Path,
        protocol: Option<&str>,
        mut input: I,
        mut output: O,
        mut errors: E,
    ) -> Result<ExitStatus, TransportError>
    where
        I: AsyncRead + Send + Unpin + 'static,
        O: AsyncWrite + Unpin,
        E: AsyncWrite + Unpin,
    {
        let mut child = self
            .service_command(service, protocol)
            .arg(repo_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(TransportError::Spawn)?;

        let mut stdin = child.stdin.take().expect("stdin is piped");
        let mut stdout = child.stdout.take().expect("stdout is piped");
        let mut stderr = child.stderr.take().expect("stderr is piped");

        let feed = AbortOnDrop(tokio::spawn(async move {
            if let Err(error) = tokio::io::copy(&mut input, &mut stdin).await {
                tracing::debug!(%service, %error, "client stream ended early");
            }
            // Dropping stdin tells git the client has nothing more to say.
        }));

        let (forwarded, reported) = tokio::join!(tokio::io::copy(&mut stdout, &mut output), tokio::io::copy(&mut stderr, &mut errors));
        if let Err(error) = forwarded.and(reported) {
            // The client went away: nobody drains git's pipes any more, so stop it rather
            // than wait for it to block forever.
            tracing::debug!(%service, %error, "client went away while git was responding");
            child.start_kill()?;
        }
        let status = child.wait().await?;
        drop(feed);
        Ok(status)
    }
}

/// Aborts a background task when dropped, so it never outlives the exchange it serves.
#[derive(Debug)]
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// `GIT_PROTOCOL` is client-controlled; only allow the `key=value[:key=value]` alphabet.
/// Both transports apply this filter: HTTP to the `Git-Protocol` header, SSH to the
/// `GIT_PROTOCOL` environment request.
pub fn is_safe_protocol_header(value: &str) -> bool {
    value.len() <= 128 && value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'=' | b':' | b'.' | b'-' | b'_'))
}

fn null_device() -> OsString {
    if cfg!(windows) { "NUL".into() } else { "/dev/null".into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_header_filter() {
        assert!(is_safe_protocol_header("version=2"));
        assert!(!is_safe_protocol_header("version=2\nGIT_DIR=/etc"));
        assert!(!is_safe_protocol_header("$(rm -rf /)"));
    }

    fn bare_repository() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let status = std::process::Command::new("git").args(["init", "--quiet", "--bare"]).arg(dir.path()).status().unwrap();
        assert!(status.success());
        dir
    }

    async fn interactive(service: Service, repo: &Path, protocol: Option<&str>, input: &'static [u8]) -> (ExitStatus, Vec<u8>) {
        let (mut output, mut errors) = (Vec::new(), Vec::new());
        let status =
            ProcessTransport::new("git").serve_interactive(service, repo, protocol, input, &mut output, &mut errors).await.unwrap();
        (status, output)
    }

    #[tokio::test]
    async fn interactive_upload_pack_advertises_refs_then_exits() {
        let repo = bare_repository();
        let (status, output) = interactive(Service::UploadPack, repo.path(), None, b"0000").await;
        assert!(status.success());
        let output = String::from_utf8_lossy(&output);
        assert!(output.contains("capabilities^{}"), "v0 advertisement: {output}");
        assert!(output.ends_with("0000"), "{output}");
    }

    #[tokio::test]
    async fn interactive_mode_honours_only_safe_protocol_requests() {
        let repo = bare_repository();
        let (status, output) = interactive(Service::UploadPack, repo.path(), Some("version=2"), b"0000").await;
        assert!(status.success());
        assert!(output.starts_with(b"000eversion 2\n"), "{}", String::from_utf8_lossy(&output));

        let (_, output) = interactive(Service::UploadPack, repo.path(), Some("version=2\nx"), b"0000").await;
        assert!(!output.starts_with(b"000eversion 2\n"), "unsafe values are ignored");
    }

    #[tokio::test]
    async fn interactive_receive_pack_finishes_when_the_client_sends_nothing() {
        let repo = bare_repository();
        let (status, output) = interactive(Service::ReceivePack, repo.path(), None, b"0000").await;
        assert!(status.success());
        assert!(String::from_utf8_lossy(&output).contains("report-status"));
    }

    #[tokio::test]
    async fn interactive_mode_returns_when_git_exits_even_if_the_client_stays_open() {
        let repo = bare_repository();
        let (client, mut server_side) = tokio::io::duplex(64);
        let (mut output, mut errors) = (Vec::new(), Vec::new());
        tokio::io::AsyncWriteExt::write_all(&mut server_side, b"0000").await.unwrap();
        let transport = ProcessTransport::new("git");
        let exchange = transport.serve_interactive(Service::UploadPack, repo.path(), None, client, &mut output, &mut errors);
        let status = tokio::time::timeout(std::time::Duration::from_secs(30), exchange).await.expect("did not hang").unwrap();
        assert!(status.success());
        drop(server_side);
    }
}
