//! Controlled streaming keeps the child and feeder owned until actual completion.

use std::process::ExitStatus;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    process::Child,
};

use super::{ProcessControl, TransportError, control, pump::Pump};
use crate::Service;

/// The three independently streamed directions of one Git exchange.
/// Bundling them keeps the same static stream types across HTTP and SSH adapters.
#[derive(Debug)]
pub struct ProcessIo<I, O, E> {
    /// Client data copied to Git's standard input.
    pub input: I,
    /// Git's standard output, forwarded without pack buffering.
    pub output: O,
    /// Git progress and errors, kept separate from protocol output.
    pub errors: E,
}

impl<I, O, E> ProcessIo<I, O, E> {
    /// Bundles existing streams without copying or buffering their contents.
    pub fn new(input: I, output: O, errors: E) -> Self {
        Self { input, output, errors }
    }
}

pub(super) async fn run<I, O, E>(
    service: Service,
    mut child: Child,
    io: ProcessIo<I, O, E>,
    control: Option<&ProcessControl>,
) -> Result<ExitStatus, TransportError>
where
    I: AsyncRead + Send + Unpin + 'static,
    O: AsyncWrite + Unpin,
    E: AsyncWrite + Unpin,
{
    let ProcessIo { mut input, mut output, mut errors } = io;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let feed = Pump::new(tokio::spawn(async move {
        if let Err(error) = tokio::io::copy(&mut input, &mut stdin).await {
            tracing::debug!(%service, %error, "client stream ended early");
        }
    }));

    // Scope the borrowed copy futures so they are destroyed before reaping/return.
    let copied = tokio::select! {
        biased;
        reason = control::stopped(control) => Err(reason),
        copied = async { tokio::try_join!(tokio::io::copy(&mut stdout, &mut output), tokio::io::copy(&mut stderr, &mut errors)) } => {
            copied.map(|_| ()).map_err(TransportError::Io)
        }
    };
    let result = match copied {
        Err(reason @ (TransportError::Cancelled | TransportError::TimedOut)) => {
            terminate(&mut child).await.map_err(TransportError::Io).and(Err(reason))
        }
        Err(error) => {
            // Preserve the legacy broken-output result: report Git's killed exit status.
            tracing::debug!(%service, %error, "client went away while git was responding");
            terminate(&mut child).await.map_err(TransportError::Io)
        }
        Ok(()) => tokio::select! {
            biased;
            reason = control::stopped(control) => {
                terminate(&mut child).await.map_err(TransportError::Io).and(Err(reason))
            }
            status = child.wait() => status.map_err(TransportError::Io),
        },
    };
    // Abort is a request. Join is the proof that input/stdin captures were destroyed.
    feed.stop().await;
    result
}

pub(super) async fn terminate(child: &mut Child) -> std::io::Result<ExitStatus> {
    if let Err(error) = child.start_kill() {
        tracing::warn!(%error, "could not request Git termination; waiting for its actual exit");
    }
    child.wait().await
}
