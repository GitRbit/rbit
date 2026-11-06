//! Smart-protocol backend that delegates to the Git executable.
use crate::{Service, pkt_line};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    process::Command,
};

mod advertisement;
mod control;
#[cfg(test)]
mod controlled_tests;
mod exchange;
mod owned_advertisement;
mod pump;
mod stateless;
#[cfg(test)]
mod tests;

pub use control::ProcessControl;
pub use exchange::ProcessIo;
pub use stateless::{ServiceOutput, ServiceSession};

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("Git operation was cancelled")]
    Cancelled,
    #[error("Git operation exceeded its session deadline")]
    TimedOut,
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

/// Runs Git services as child processes with a scrubbed environment.
#[derive(Debug, Clone)]
pub struct ProcessTransport {
    git_binary: PathBuf,
    max_push_bytes: Option<u64>,
}

impl ProcessTransport {
    pub fn new(git_binary: impl Into<PathBuf>) -> Self {
        Self { git_binary: git_binary.into(), max_push_bytes: None }
    }

    /// Caps the pack a push may send (`receive.maxInputSize`); `None` leaves it unlimited.
    #[must_use]
    pub fn with_max_push_bytes(mut self, max_push_bytes: Option<u64>) -> Self {
        self.max_push_bytes = max_push_bytes;
        self
    }

    /// Builds `git <service>` ignoring system/global configuration and inheriting only the
    /// environment variables Git needs to run. Repository hooks never run
    /// (`core.hooksPath=<null device>`). Handle drop requests termination as a fallback;
    /// controlled callers await actual reaping before claiming shutdown completion.
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
        let mut hooks = OsString::from("core.hooksPath=");
        hooks.push(null_device());
        command.arg("-c").arg(hooks);
        if service == Service::ReceivePack {
            if options.deny_non_fast_forwards {
                command.args(["-c", "receive.denyNonFastForwards=true"]);
            }
            if let Some(max) = self.max_push_bytes {
                command.arg("-c").arg(format!("receive.maxInputSize={max}"));
            }
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

    /// The raw ref advertisement, without the HTTP service header.
    /// Arbitrary future drop requests a kill but is not proof of reaping; owned
    /// shutdown paths must await the controlled variant to completion.
    pub async fn advertisement(&self, service: Service, repo_path: &Path, protocol: Option<&str>) -> Result<Vec<u8>, TransportError> {
        self.advertisement_inner(service, repo_path, protocol, None).await
    }

    /// Buffers an advertisement while observing session cancellation and deadline.
    /// Returns only after Git is reaped and its local pipe readers are destroyed.
    pub async fn advertisement_controlled(
        &self,
        service: Service,
        repo_path: &Path,
        protocol: Option<&str>,
        control: &ProcessControl,
    ) -> Result<Vec<u8>, TransportError> {
        self.advertisement_inner(service, repo_path, protocol, Some(control)).await
    }

    async fn advertisement_inner(
        &self,
        service: Service,
        repo_path: &Path,
        protocol: Option<&str>,
        control: Option<&ProcessControl>,
    ) -> Result<Vec<u8>, TransportError> {
        control::check(control)?;
        let child = self
            .command(service, repo_path, protocol, ReceiveOptions::default())
            .arg("--advertise-refs")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(TransportError::Spawn)?;
        advertisement::collect(service, child, control).await
    }

    /// Produces HTTP info/refs, including the v0/v1 service header where required.
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

    /// Starts a stateless HTTP exchange with independently owned process cleanup.
    /// Completion follows process reaping and destruction of request/stderr pumps.
    pub fn serve(
        &self,
        service: Service,
        repo_path: &Path,
        protocol: Option<&str>,
        options: ReceiveOptions,
        request: impl AsyncRead + Send + Unpin + 'static,
    ) -> Result<ServiceSession, TransportError> {
        stateless::serve(self, service, repo_path, protocol, options, request, ())
    }

    /// Starts an exchange whose independent owner retains the caller's resource.
    /// Successful cleanup releases it only after direct-child exit and pump destruction.
    /// Use a shared guard when response-stream lifetime must also retain admission.
    /// A wait error or owner panic is not proof of process exit; see the lifecycle contract.
    pub fn serve_with_guard<G: Send + 'static>(
        &self,
        service: Service,
        repo_path: &Path,
        protocol: Option<&str>,
        options: ReceiveOptions,
        request: impl AsyncRead + Send + Unpin + 'static,
        guard: G,
    ) -> Result<ServiceSession, TransportError> {
        stateless::serve(self, service, repo_path, protocol, options, request, guard)
    }

    /// Runs an SSH push after its separately inspected advertisement and commands.
    /// Arbitrary drop requests kill only; use the controlled API for a stopped proof.
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
        self.receive_pack_inner(repo_path, protocol, options, ProcessIo::new(input, output, errors), None).await
    }

    /// Awaits an SSH push, including kill/reap and feeder destruction on interruption.
    /// The caller retains admission permits until this future returns, and must not
    /// wrap it in a dropping timeout or abort its owning task while Git is active.
    pub async fn receive_pack_controlled<I, O, E>(
        &self,
        repo_path: &Path,
        protocol: Option<&str>,
        options: ReceiveOptions,
        io: ProcessIo<I, O, E>,
        control: &ProcessControl,
    ) -> Result<ExitStatus, TransportError>
    where
        I: AsyncRead + Send + Unpin + 'static,
        O: AsyncWrite + Unpin,
        E: AsyncWrite + Unpin,
    {
        self.receive_pack_inner(repo_path, protocol, options, io, Some(control)).await
    }

    async fn receive_pack_inner<I, O, E>(
        &self,
        repo_path: &Path,
        protocol: Option<&str>,
        options: ReceiveOptions,
        io: ProcessIo<I, O, E>,
        control: Option<&ProcessControl>,
    ) -> Result<ExitStatus, TransportError>
    where
        I: AsyncRead + Send + Unpin + 'static,
        O: AsyncWrite + Unpin,
        E: AsyncWrite + Unpin,
    {
        control::check(control)?;
        let child = self
            .command(Service::ReceivePack, repo_path, protocol, options)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(TransportError::Spawn)?;
        exchange::run(Service::ReceivePack, child, io, control).await
    }

    /// Runs one interactive Git conversation, preserving the legacy stream API.
    /// Arbitrary future drop is not a process-reaping or admission-release proof.
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
        self.interactive_inner(service, repo_path, protocol, ProcessIo::new(input, output, errors), None).await
    }

    /// Runs interactive Git until completion or cooperative session interruption.
    /// Interruption destroys output-copy futures, kills and reaps Git, then aborts
    /// and joins the input feeder before returning. Caller-held permits cover cleanup.
    pub async fn serve_interactive_controlled<I, O, E>(
        &self,
        service: Service,
        repo_path: &Path,
        protocol: Option<&str>,
        io: ProcessIo<I, O, E>,
        control: &ProcessControl,
    ) -> Result<ExitStatus, TransportError>
    where
        I: AsyncRead + Send + Unpin + 'static,
        O: AsyncWrite + Unpin,
        E: AsyncWrite + Unpin,
    {
        self.interactive_inner(service, repo_path, protocol, io, Some(control)).await
    }

    async fn interactive_inner<I, O, E>(
        &self,
        service: Service,
        repo_path: &Path,
        protocol: Option<&str>,
        io: ProcessIo<I, O, E>,
        control: Option<&ProcessControl>,
    ) -> Result<ExitStatus, TransportError>
    where
        I: AsyncRead + Send + Unpin + 'static,
        O: AsyncWrite + Unpin,
        E: AsyncWrite + Unpin,
    {
        control::check(control)?;
        let child = self
            .service_command(service, protocol, ReceiveOptions::default())
            .arg(repo_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(TransportError::Spawn)?;
        exchange::run(service, child, io, control).await
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
