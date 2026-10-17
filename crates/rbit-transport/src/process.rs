//! Smart-protocol backend that delegates to the `git` executable.

use std::{
    ffi::OsString,
    future::Future,
    io,
    path::{Path, PathBuf},
    pin::Pin,
    process::{ExitStatus, Stdio},
    task::{Context, Poll, ready},
};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf},
    process::{Child, ChildStdout, Command},
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

/// How `git receive-pack` runs, as decided by the push gate (`crate::receive`) before git
/// starts. Ignored for `upload-pack`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReceiveOptions {
    /// Run with `receive.denyNonFastForwards`: git refuses every non-fast-forward update
    /// of this push after unpacking and before updating refs ("non-fast-forward").
    pub deny_non_fast_forwards: bool,
}

/// A running `git <service> --stateless-rpc` exchange.
#[derive(Debug)]
pub struct ServiceSession {
    /// The response body to stream back to the client.
    pub output: ServiceOutput,
    /// Resolves with the exit status once git finishes; use it to trigger post-push work.
    pub completion: oneshot::Receiver<ExitStatus>,
}

/// A response stream that owns the lifetime of its Git exchange. Dropping it before
/// completion kills Git and releases the request body and stderr readers.
#[derive(Debug)]
pub struct ServiceOutput {
    stdout: ChildStdout,
    finished: oneshot::Receiver<()>,
    ended: bool,
    _cancel: oneshot::Sender<()>,
}

impl AsyncRead for ServiceOutput {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        if self.ended || buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        let before = buf.filled().len();
        ready!(Pin::new(&mut self.stdout).poll_read(cx, buf))?;
        if buf.filled().len() != before {
            return Poll::Ready(Ok(()));
        }
        // Keep ownership until Git exits: dropping a fully read response must not kill
        // a successful push between closing stdout and reporting its exit status.
        let finished = ready!(Pin::new(&mut self.finished).poll(cx));
        self.ended = true;
        Poll::Ready(finished.map_err(|_| io::Error::other("git exchange was interrupted")))
    }
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
    fn service_command(&self, service: Service, protocol: Option<&str>, options: ReceiveOptions) -> Command {
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
        if service == Service::ReceivePack && options.deny_non_fast_forwards {
            command.args(["-c", "receive.denyNonFastForwards=true"]);
        }
        command.arg(service.subcommand()).kill_on_drop(true);
        command
    }

    /// `git <service> --stateless-rpc <repo>`, for request/response exchanges.
    fn command(&self, service: Service, repo_path: &Path, protocol: Option<&str>, options: ReceiveOptions) -> Command {
        let mut command = self.service_command(service, protocol, options);
        command.arg("--stateless-rpc").arg(repo_path);
        command
    }

    /// The ref advertisement exactly as git writes it, without HTTP's `# service=` header:
    /// what an SSH server sends before reading a push's commands.
    pub async fn advertisement(&self, service: Service, repo_path: &Path, protocol: Option<&str>) -> Result<Vec<u8>, TransportError> {
        let output = self
            .command(service, repo_path, protocol, ReceiveOptions::default())
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
        Ok(output.stdout)
    }

    /// Produces the body for `GET …/info/refs?service=…`.
    ///
    /// For protocol v0/v1 the advertisement is prefixed with the `# service=` header; v2
    /// clients expect the capability advertisement directly.
    pub async fn advertise_refs(&self, service: Service, repo_path: &Path, protocol: Option<&str>) -> Result<Vec<u8>, TransportError> {
        let advertisement = self.advertisement(service, repo_path, protocol).await?;
        let is_v2 = protocol.filter(|p| is_safe_protocol_header(p)).is_some_and(|p| p.split(':').any(|kv| kv == "version=2"));
        let mut body = Vec::with_capacity(advertisement.len() + 64);
        if !is_v2 {
            body.extend(pkt_line::encode(&format!("# service={}\n", service.wire_name())));
            body.extend_from_slice(pkt_line::FLUSH);
        }
        body.extend(advertisement);
        Ok(body)
    }

    /// Starts `POST …/git-upload-pack` or `…/git-receive-pack`: streams `request` into git's
    /// stdin and returns git's stdout for streaming back to the client. `options` apply to
    /// receive-pack only.
    pub fn serve(
        &self,
        service: Service,
        repo_path: &Path,
        protocol: Option<&str>,
        options: ReceiveOptions,
        mut request: impl AsyncRead + Send + Unpin + 'static,
    ) -> Result<ServiceSession, TransportError> {
        let mut child = self
            .command(service, repo_path, protocol, options)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(TransportError::Spawn)?;

        let mut stdin = child.stdin.take().expect("stdin is piped");
        let output = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");

        let feed = AbortOnDrop(tokio::spawn(async move {
            if let Err(error) = tokio::io::copy(&mut request, &mut stdin).await {
                tracing::debug!(%service, %error, "client request body ended early");
            }
            // Dropping stdin signals end-of-request to git.
        }));

        let errors = AbortOnDrop(tokio::spawn(async move {
            // Remote progress and error messages may have no newline at all. Drain
            // with fixed memory and cap logging as well as the retained excerpt.
            let mut stderr = stderr;
            let mut buffer = [0_u8; 8192];
            let mut remaining = 16 * 1024;
            while let Ok(read) = stderr.read(&mut buffer).await {
                if read == 0 {
                    break;
                }
                let logged = read.min(remaining);
                if logged > 0 {
                    tracing::debug!(%service, stderr = %String::from_utf8_lossy(&buffer[..logged]), "git stderr");
                    remaining -= logged;
                }
            }
        }));

        let (done, completion) = oneshot::channel();
        let (finished_tx, finished) = oneshot::channel();
        let (cancel, cancelled) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let result = tokio::select! {
                status = child.wait() => status,
                _ = cancelled => {
                    let _ = child.kill().await;
                    child.wait().await
                }
            };
            drop((feed, errors));
            match result {
                Ok(status) => {
                    let _ = done.send(status);
                }
                Err(error) => tracing::warn!(%service, %error, "failed to wait for git"),
            }
            let _ = finished_tx.send(());
        });

        let output = ServiceOutput { stdout: output, finished, ended: false, _cancel: cancel };
        Ok(ServiceSession { output, completion })
    }

    /// Runs `git receive-pack --stateless-rpc <repo>` on `input` (the commands and pack
    /// that follow an advertisement sent with [`Self::advertisement`]), streaming git's
    /// stdout to `output` and its stderr to `errors`, and returns git's exit status. This
    /// is the second half of SSH's two-step push, which lets the push gate read the
    /// commands before git starts; it is the same exchange smart HTTP performs.
    ///
    /// Dropping the returned future kills git.
    pub async fn receive_pack<I, O, E>(
        &self,
        repo_path: &Path,
        protocol: Option<&str>,
        options: ReceiveOptions,
        input: I,
        output: O,
        errors: E,
    ) -> Result<ExitStatus, TransportError>
    where
        I: AsyncRead + Send + Unpin + 'static,
        O: AsyncWrite + Unpin,
        E: AsyncWrite + Unpin,
    {
        let child = self
            .command(Service::ReceivePack, repo_path, protocol, options)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(TransportError::Spawn)?;
        exchange(Service::ReceivePack, child, input, output, errors).await
    }

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
        input: I,
        output: O,
        errors: E,
    ) -> Result<ExitStatus, TransportError>
    where
        I: AsyncRead + Send + Unpin + 'static,
        O: AsyncWrite + Unpin,
        E: AsyncWrite + Unpin,
    {
        let child = self
            .service_command(service, protocol, ReceiveOptions::default())
            .arg(repo_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(TransportError::Spawn)?;
        exchange(service, child, input, output, errors).await
    }
}

/// Feeds `input` to a spawned git, forwards its stdout and stderr, and waits for it.
async fn exchange<I, O, E>(
    service: Service,
    mut child: Child,
    mut input: I,
    mut output: O,
    mut errors: E,
) -> Result<ExitStatus, TransportError>
where
    I: AsyncRead + Send + Unpin + 'static,
    O: AsyncWrite + Unpin,
    E: AsyncWrite + Unpin,
{
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");

    let feed = AbortOnDrop(tokio::spawn(async move {
        if let Err(error) = tokio::io::copy(&mut input, &mut stdin).await {
            tracing::debug!(%service, %error, "client stream ended early");
        }
        // Dropping stdin tells git the client has nothing more to say.
    }));

    if let Err(error) = tokio::try_join!(tokio::io::copy(&mut stdout, &mut output), tokio::io::copy(&mut stderr, &mut errors)) {
        // The client went away: nobody drains git's pipes any more, so stop it rather
        // than wait for it to block forever.
        tracing::debug!(%service, %error, "client went away while git was responding");
        child.start_kill()?;
    }
    let status = child.wait().await?;
    drop(feed);
    Ok(status)
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

    #[tokio::test]
    async fn the_raw_advertisement_has_no_http_service_header() {
        let repo = bare_repository();
        let transport = ProcessTransport::new("git");
        let raw = transport.advertisement(Service::ReceivePack, repo.path(), None).await.unwrap();
        assert!(String::from_utf8_lossy(&raw).contains("report-status"));
        assert!(!raw.starts_with(b"001f# service="));
        let http = transport.advertise_refs(Service::ReceivePack, repo.path(), None).await.unwrap();
        assert!(http.starts_with(b"001f# service=git-receive-pack\n0000"));
        assert!(http.ends_with(&raw));
    }

    #[tokio::test]
    async fn stateless_receive_pack_with_an_empty_command_list_exits_cleanly() {
        let repo = bare_repository();
        let (client, mut server_side) = tokio::io::duplex(64);
        tokio::io::AsyncWriteExt::write_all(&mut server_side, b"0000").await.unwrap();
        let (mut output, mut errors) = (Vec::new(), Vec::new());
        let transport = ProcessTransport::new("git");
        // The client keeps its stream open, as an SSH client waiting for the report does.
        let exchange = transport.receive_pack(repo.path(), None, ReceiveOptions::default(), client, &mut output, &mut errors);
        let status = tokio::time::timeout(std::time::Duration::from_secs(30), exchange).await.expect("did not hang").unwrap();
        assert!(status.success(), "{}", String::from_utf8_lossy(&errors));
        assert!(output.is_empty(), "nothing to report: {}", String::from_utf8_lossy(&output));
        drop(server_side);
    }

    #[tokio::test]
    async fn dropping_a_stateless_response_stops_git_and_releases_the_request() {
        struct PendingRequest(Option<oneshot::Sender<()>>);
        impl AsyncRead for PendingRequest {
            fn poll_read(self: Pin<&mut Self>, _: &mut Context<'_>, _: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
                Poll::Pending
            }
        }
        impl Drop for PendingRequest {
            fn drop(&mut self) {
                let _ = self.0.take().unwrap().send(());
            }
        }

        let repo = bare_repository();
        let (dropped, request_dropped) = oneshot::channel();
        let session = ProcessTransport::new("git")
            .serve(Service::UploadPack, repo.path(), None, ReceiveOptions::default(), PendingRequest(Some(dropped)))
            .unwrap();
        drop(session.output);
        let stopped = async {
            let status = session.completion.await.unwrap();
            assert!(!status.success(), "an unfinished exchange must be killed");
            request_dropped.await.unwrap();
        };
        tokio::time::timeout(std::time::Duration::from_secs(10), stopped).await.expect("abandoned exchange stayed alive");
    }

    #[tokio::test]
    async fn a_failed_output_does_not_wait_for_the_other_pipe() {
        struct Disconnected;
        impl AsyncWrite for Disconnected {
            fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, _: &[u8]) -> Poll<io::Result<usize>> {
                Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()))
            }
            fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
                Poll::Ready(Ok(()))
            }
            fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
                Poll::Ready(Ok(()))
            }
        }

        let repo = bare_repository();
        let (input, _client) = tokio::io::duplex(64);
        let transport = ProcessTransport::new("git");
        let exchange = transport.serve_interactive(Service::UploadPack, repo.path(), None, input, Disconnected, tokio::io::sink());
        let status = tokio::time::timeout(std::time::Duration::from_secs(10), exchange).await.expect("failed output hung").unwrap();
        assert!(!status.success());
    }

    #[tokio::test]
    async fn invalid_v2_header_keeps_the_v0_http_service_header() {
        let repo = bare_repository();
        let output =
            ProcessTransport::new("git").advertise_refs(Service::UploadPack, repo.path(), Some("version=2:invalid\n")).await.unwrap();
        assert!(output.starts_with(b"001e# service=git-upload-pack\n0000"));
    }
}
