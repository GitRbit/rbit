//! HTTP streaming owns Git and its pumps independently of the response waiter.
use super::{ProcessTransport, ReceiveOptions, TransportError, pump::Pump};
use crate::Service;
use std::{
    future::Future,
    io,
    path::Path,
    pin::Pin,
    process::{ExitStatus, Stdio},
    task::{Context, Poll, ready},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, ReadBuf},
    process::ChildStdout,
    sync::oneshot,
};
/// A running `git <service> --stateless-rpc` exchange.
#[derive(Debug)]
pub struct ServiceSession {
    /// The response body to stream back to the client.
    pub output: ServiceOutput,
    /// Resolves with the exit status once git finishes; use it to trigger post-push work.
    pub completion: oneshot::Receiver<ExitStatus>,
}

/// A response stream that owns cancellation of its Git exchange. Early drop signals
/// its independent owner to kill/reap Git and abort/join request and stderr readers.
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

pub(super) fn serve<G: Send + 'static>(
    transport: &ProcessTransport,
    service: Service,
    repo_path: &Path,
    protocol: Option<&str>,
    options: ReceiveOptions,
    mut request: impl AsyncRead + Send + Unpin + 'static,
    guard: G,
) -> Result<ServiceSession, TransportError> {
    let mut child = transport
        .command(service, repo_path, protocol, options)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(TransportError::Spawn)?;

    let mut stdin = child.stdin.take().expect("stdin is piped");
    let output = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");

    let feed = Pump::new(tokio::spawn(async move {
        if let Err(error) = tokio::io::copy(&mut request, &mut stdin).await {
            tracing::debug!(%service, %error, "client request body ended early");
        }
        // Dropping stdin signals end-of-request to git.
    }));

    let errors = Pump::new(tokio::spawn(async move {
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
        tokio::join!(feed.stop(), errors.stop());
        drop(guard);
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
