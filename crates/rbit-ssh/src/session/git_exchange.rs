//! Owned Git exchanges retain admission slots through process and effect cleanup.

use super::{FAILURE, GitCommand, PendingChannel, STDERR};
use crate::{
    access::{internal, push_source, read_only_deploy_key},
    server::Shared,
};
use rbit_services::{ConcurrencySlot, PendingPush, RepoContext, protection::PushActor, ssh_keys::SshIdentity};
use rbit_transport::{
    ProcessControl, ProcessIo, ReceiveOptions, Service, TransportError,
    receive::{self, CommandLimits, Intercept},
    refused_push::{PushRefusal, push_refusal},
};
use russh::ChannelMsg;
use std::{
    process::ExitStatus,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt, DuplexStream, duplex},
    task::JoinHandle,
};

const INPUT_BUFFER: usize = 64 * 1024;

/// Runs the Git service for an authorised `exec` request, streaming between the channel
/// and git, then reports git's exit status and closes the channel. `_process` is the
/// command's place within the Git process limit, held until it ends.
pub(super) async fn run_git(
    shared: Arc<Shared>,
    identity: SshIdentity,
    command: GitCommand,
    ctx: RepoContext,
    pending: PendingChannel,
    _process: ConcurrencySlot,
) {
    let PendingChannel { channel, protocol, _slot, .. } = pending;
    let control = ProcessControl::new(shared.tasks.cancellation(), tokio::time::Instant::now() + shared.config.ssh.max_session());
    if control.check().is_err() {
        return;
    }
    let (mut client, writer) = channel.split();
    let mut stdout = writer.make_writer();
    let mut stderr = writer.make_writer_ext(Some(STDERR));

    // Channel messages → git's stdin. Only data is forwarded; EOF ends git's input.
    let (input, mut feed) = duplex(INPUT_BUFFER);
    let pump = shared.tasks.spawn(async move {
        while let Some(message) = client.wait().await {
            match message {
                ChannelMsg::Data { data } => {
                    if feed.write_all(&data).await.is_err() {
                        break;
                    }
                }
                ChannelMsg::Eof | ChannelMsg::Close => break,
                _ => {}
            }
        }
        let _ = feed.shutdown().await;
    });
    let Some(pump) = pump else { return };
    let mut pump = AbortOnDrop(pump);

    let started = Instant::now();
    let service = command.service;
    let exchange = async {
        if service.is_write() {
            push(&shared, &identity, &ctx, protocol.as_deref(), ProcessIo::new(input, &mut stdout, &mut stderr), &control).await
        } else {
            let status = shared
                .transport
                .serve_interactive_controlled(
                    service,
                    &ctx.disk_path,
                    protocol.as_deref(),
                    ProcessIo::new(input, &mut stdout, &mut stderr),
                    &control,
                )
                .await?;
            Ok(Exchanged { status: exit_code(status), push: None })
        }
    };
    // The transport owns cancellation and reaping. Dropping an outer timeout here
    // would only request kill-on-drop and release admission before Git really exited.
    let outcome = exchange.await;
    let (status, push, message) = match outcome {
        Ok(Exchanged { status, push }) => (status, push, None),
        Err(TransportError::Cancelled | TransportError::TimedOut) => {
            tracing::warn!(repo = %command.full_name(), %service, "SSH Git operation stopped");
            (FAILURE, None, Some("ERROR: the operation was stopped\r\n"))
        }
        Err(error) => {
            tracing::error!(%error, repo = %command.full_name(), %service, "failed to run git");
            (FAILURE, None, Some("ERROR: internal error; please try again later\r\n"))
        }
    };
    pump.0.abort();
    let _ = (&mut pump.0).await;
    drop(pump);

    // Successful mutations must finish bookkeeping even when a peer or host stops.
    if let Some(push) = push.filter(|_| status == 0) {
        shared.services.pushes.complete(push).await;
    }
    let stop = shared.tasks.cancellation();
    tokio::select! {
        biased;
        () = stop.cancelled() => {},
        _ = tokio::time::timeout(Duration::from_secs(1), async {
            if let Some(message) = message { let _ = stderr.write_all(message.as_bytes()).await; }
            let _ = stdout.flush().await;
            let _ = stderr.flush().await;
            let _ = writer.exit_status(status).await;
            let _ = writer.eof().await;
            let _ = writer.close().await;
        }) => {},
    }

    let actor = match &identity {
        SshIdentity::User { user, .. } => user.name().to_string(),
        SshIdentity::Deploy { key, .. } => format!("deploy key {}", key.0),
    };
    tracing::info!(%actor, repo = %command.full_name(), %service, status, elapsed_ms = started.elapsed().as_millis(), "SSH Git request");
}

/// How a Git exchange ended: the exit status to report, and the push to record once it
/// succeeded.
struct Exchanged {
    status: u32,
    push: Option<PendingPush>,
}

/// git's exit status as SSH reports it.
fn exit_code(status: ExitStatus) -> u32 {
    status.code().and_then(|code| u32::try_from(code).ok()).unwrap_or(FAILURE)
}

/// `git-receive-pack` in two steps, like smart HTTP: the ref advertisement first, then the
/// push gate reads the client's command list and decides, and only then does
/// `git receive-pack --stateless-rpc` run, on the replayed input. Refused pushes never
/// start git: the client gets the refusal report (or, without `report-status`, an error
/// on stderr and exit status 1). Messages for the client go to `stderr`; the error is
/// for failures to talk to git.
async fn push<O, E>(
    shared: &Shared,
    identity: &SshIdentity,
    ctx: &RepoContext,
    protocol: Option<&str>,
    io: ProcessIo<DuplexStream, &mut O, &mut E>,
    control: &ProcessControl,
) -> Result<Exchanged, TransportError>
where
    O: AsyncWrite + Unpin,
    E: AsyncWrite + Unpin,
{
    let ProcessIo { input, output: stdout, errors: stderr } = io;
    control.check()?;
    let refused = |status| Ok(Exchanged { status, push: None });
    let advertisement = shared.transport.advertisement_controlled(Service::ReceivePack, &ctx.disk_path, protocol, control).await?;
    controlled_io(control, async {
        stdout.write_all(&advertisement).await?;
        stdout.flush().await
    })
    .await?;

    let (user, deploy_key) = match identity {
        SshIdentity::User { user, .. } => (Some(user), false),
        SshIdentity::Deploy { .. } => (None, true),
    };
    let (input, options): (Box<dyn AsyncRead + Send + Unpin>, ReceiveOptions) = match controlled_intercept(input, control).await? {
        Err(error) => {
            controlled_io(control, stderr.write_all(format!("ERROR: {error}\r\n").as_bytes())).await?;
            return refused(FAILURE);
        }
        Ok(Intercept::Unread(input)) => (Box::new(input), ReceiveOptions::default()),
        Ok(Intercept::Read { commands, input }) => {
            if commands.is_empty() {
                // `Everything up-to-date`: nothing for git to do.
                return refused(0);
            }
            let actor = PushActor { user, deploy_key, access: ctx.access };
            let mut verdict = match shared.services.protection.check_push(ctx, &actor, &commands.updates()).await {
                Ok(verdict) => verdict,
                Err(error) => {
                    controlled_io(control, stderr.write_all(format!("ERROR: {}\r\n", internal(&error)).as_bytes())).await?;
                    return refused(FAILURE);
                }
            };
            if deploy_key && !ctx.access.can_write() {
                let reason = read_only_deploy_key(&ctx.repo.full_name());
                for refusal in &mut verdict.refusals {
                    refusal.reason.clone_from(&reason);
                }
            }
            if verdict.is_refused() {
                tokio::select! { error = control.stopped() => return Err(error), () = receive::drain(input) => {} };
                match push_refusal(&commands, &verdict.refusals) {
                    PushRefusal::Explanation(message) => {
                        let message = message.replace('\n', "\r\n");
                        controlled_io(control, stderr.write_all(format!("ERROR: push declined\r\n{message}").as_bytes())).await?;
                        return refused(FAILURE);
                    }
                    PushRefusal::Report(report) => {
                        controlled_io(control, stdout.write_all(&report)).await?;
                        // Like a declining pre-receive hook: git reports and exits successfully.
                        return refused(0);
                    }
                }
            }
            (Box::new(input), ReceiveOptions { deny_non_fast_forwards: verdict.deny_non_fast_forwards })
        }
    };

    // Taken after the verdict, before git changes anything.
    control.check()?;
    let pending = shared.services.pushes.begin(ctx, push_source(identity)).await;
    let status =
        shared.transport.receive_pack_controlled(&ctx.disk_path, protocol, options, ProcessIo::new(input, stdout, stderr), control).await?;
    Ok(Exchanged { status: exit_code(status), push: Some(pending) })
}

/// Requests abort on exceptional drop; the server scope still tracks destruction.
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

// These boundaries contain no running Child. Database effects above are awaited;
// they are not abandoned merely because the network/session deadline elapsed.
async fn controlled_io(control: &ProcessControl, io: impl std::future::Future<Output = std::io::Result<()>>) -> Result<(), TransportError> {
    tokio::select! { error = control.stopped() => Err(error), result = io => result.map_err(TransportError::from) }
}

async fn controlled_intercept(
    input: DuplexStream,
    control: &ProcessControl,
) -> Result<Result<Intercept<DuplexStream>, rbit_transport::receive::ReceiveError>, TransportError> {
    tokio::select! { error = control.stopped() => Err(error), result = receive::intercept(input, CommandLimits::default()) => Ok(result) }
}
