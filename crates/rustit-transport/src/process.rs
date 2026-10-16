//! Smart-protocol backend that delegates to the `git` executable.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
};

use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::{ChildStdout, Command},
    sync::oneshot,
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

    /// Builds a `git` command that ignores system/global configuration and inherits only
    /// the environment variables Git needs to run.
    fn command(&self, service: Service, repo_path: &Path, protocol: Option<&str>) -> Command {
        let mut command = Command::new(&self.git_binary);
        command.env_clear();
        for key in ["PATH", "SYSTEMROOT", "TEMP", "TMP", "TMPDIR"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", null_device())
            .env("GIT_TERMINAL_PROMPT", "0");
        if let Some(protocol) = protocol.filter(|p| is_safe_protocol_header(p)) {
            command.env("GIT_PROTOCOL", protocol);
        }
        command.arg(service.subcommand()).arg("--stateless-rpc").arg(repo_path).kill_on_drop(true);
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

/// `GIT_PROTOCOL` is client-controlled; only allow the `key=value[:key=value]` alphabet.
fn is_safe_protocol_header(value: &str) -> bool {
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
}
