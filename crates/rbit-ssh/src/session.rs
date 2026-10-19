//! One client connection: public-key authentication, session channels and the Git
//! commands run on them.
//!
//! Only `session` channels are accepted, and on them only `exec` of a Git service, an
//! `env` request for `GIT_PROTOCOL`, and `shell` (which greets the user and closes).
//! PTYs, X11, agent forwarding, subsystems and every kind of port forwarding are refused.
//! Nothing a client sends is ever passed to a shell.
//!
//! Fetches run `git upload-pack` interactively. Pushes run in two steps, like smart HTTP:
//! the server sends the ref advertisement itself, the push gate reads the client's
//! command list and asks `ProtectionService::check_push` (after the access, mirror and
//! old-name checks of [`Connection::authorize`]), and only then does
//! `git receive-pack --stateless-rpc` run on the replayed input.
//!
//! `git-lfs-authenticate <repo> <upload|download>` (Git LFS) prints the repository's LFS
//! endpoint with a short-lived signed header as JSON, after the same checks as a fetch
//! (download) or push (upload); it is refused when `[lfs] enabled` is off.

// Every handler method is written as `async fn` for uniformity, whether or not it awaits.
use std::{
    collections::HashMap,
    net::SocketAddr,
    process::ExitStatus,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use rbit_core::User;
use rbit_services::{
    PendingPush, RepoContext, ServiceError, ThrottleScope, lfs::TokenSubject, protection::PushActor, ssh_keys::SshIdentity,
};
use rbit_transport::{
    ReceiveOptions, Service, TransportError, is_safe_protocol_header,
    receive::{self, CommandLimits, Intercept},
};
use russh::{
    Channel, ChannelId, ChannelMsg, ChannelOpenFailure, Pty,
    keys::PublicKey,
    server::{Auth, ChannelOpenHandle, Handler, Msg, Session},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt, DuplexStream, duplex},
    sync::{OwnedSemaphorePermit, Semaphore},
    task::JoinHandle,
};

use crate::{
    command::{self, GitCommand, LfsAuthenticate},
    server::Shared,
};

/// Session channels one connection may have open at once.
const MAX_CHANNELS: usize = 4;
/// Buffer between the SSH channel and git's stdin.
const INPUT_BUFFER: usize = 64 * 1024;
/// SSH extended-data type for stderr (RFC 4254 §5.2).
const STDERR: u32 = 1;
/// Exit status reported for refused commands and failures.
const FAILURE: u32 = 1;

/// A session channel that has been opened but has not run a command yet.
struct PendingChannel {
    channel: Channel<Msg>,
    /// The client's `GIT_PROTOCOL` (e.g. `version=2`), already filtered.
    protocol: Option<String>,
    _slot: OwnedSemaphorePermit,
}

/// Per-connection state; russh creates one per client and drives it.
pub(crate) struct Connection {
    shared: Arc<Shared>,
    peer: SocketAddr,
    authenticated: Arc<AtomicBool>,
    identity: Option<SshIdentity>,
    fingerprint: Option<String>,
    channels: HashMap<ChannelId, PendingChannel>,
    channel_slots: Arc<Semaphore>,
}

impl Connection {
    pub(crate) fn new(shared: Arc<Shared>, peer: SocketAddr, authenticated: Arc<AtomicBool>) -> Self {
        Self {
            shared,
            peer,
            authenticated,
            identity: None,
            fingerprint: None,
            channels: HashMap::new(),
            channel_slots: Arc::new(Semaphore::new(MAX_CHANNELS)),
        }
    }

    fn throttle_key(&self) -> String {
        self.peer.ip().to_string()
    }

    fn is_throttled(&self) -> bool {
        self.shared.services.throttle.check_client(ThrottleScope::SshAuth, &self.throttle_key()).is_err()
    }

    /// Who holds `key`, if anyone (suspended users count as no one).
    async fn identify(&self, key: &PublicKey) -> Option<SshIdentity> {
        let fingerprint = rbit_services::ssh_keys::fingerprint(key);
        match self.shared.services.ssh_keys.find_by_fingerprint(&fingerprint).await {
            Ok(identity) => identity,
            Err(error) => {
                tracing::error!(%error, peer = %self.peer, "SSH key lookup failed");
                None
            }
        }
    }

    /// A connection may open more commands long after authentication. Recheck its key
    /// and load current account permissions before each one; key deletion and admin
    /// demotion must also affect already connected clients.
    async fn current_identity(&self, previous: &SshIdentity) -> Result<SshIdentity, String> {
        let refused = || "this SSH key is no longer authorized; reconnect with a valid key".to_owned();
        let fingerprint = self.fingerprint.as_deref().ok_or_else(refused)?;
        let current = self.shared.services.ssh_keys.find_by_fingerprint(fingerprint).await.map_err(|error| internal(&error))?;
        current.filter(|current| same_key_holder(previous, current)).ok_or_else(refused)
    }

    /// Opens the repository named by `command` with exactly the access HTTP would grant.
    /// The error is the message shown to the client.
    async fn authorize(&self, identity: &SshIdentity, command: &GitCommand, check_write_access: bool) -> Result<RepoContext, String> {
        let services = &self.shared.services;
        let full_name = command.full_name();
        let opened = match identity {
            SshIdentity::User { user, .. } => {
                // The connection may have been authenticated before a suspension.
                match services.accounts.is_suspended(user.id()).await {
                    Ok(false) => {}
                    Ok(true) => return Err("this account has been suspended".to_owned()),
                    Err(error) => return Err(internal(&error)),
                }
                services.repositories.open_following_renames(Some(user), &command.owner, &command.repo).await
            }
            SshIdentity::Deploy { .. } => services.ssh_keys.open_for_deploy_key(identity, &command.owner, &command.repo).await,
        };
        let ctx = match opened {
            Ok(ctx) => ctx,
            Err(ServiceError::NotFound) => return Err(format!("repository {full_name} not found, or you do not have access to it")),
            Err(error) => return Err(internal(&error)),
        };
        if check_write_access && command.service.is_write() && !ctx.access.can_write() {
            return Err(match identity {
                SshIdentity::Deploy { .. } => format!("this deploy key is read-only; it cannot push to {full_name}"),
                SshIdentity::User { .. } => format!("you do not have permission to push to {full_name}"),
            });
        }
        if command.service.is_write() {
            match services.mirrors.is_mirror(ctx.repo.repo.id).await {
                Ok(false) => {}
                Ok(true) => return Err(rbit_services::mirrors::PUSH_REFUSED.to_owned()),
                Err(error) => return Err(internal(&error)),
            }
        }
        Ok(ctx)
    }

    /// Answers `git-lfs-authenticate`: `{href, header, expires_in}` for the HTTP LFS API,
    /// with the access a fetch (download) or push (upload) over SSH would have.
    async fn lfs_authenticate(&self, identity: &SshIdentity, request: &LfsAuthenticate) -> Result<String, String> {
        let lfs = &self.shared.services.lfs;
        if !lfs.enabled() {
            return Err("Git LFS is not enabled on this server".to_owned());
        }
        let ctx = self.authorize(identity, &request.as_git_command(), true).await?;
        let subject = match identity {
            SshIdentity::User { user, .. } => TokenSubject::User(user.id()),
            SshIdentity::Deploy { key, .. } => TokenSubject::DeployKey(*key),
        };
        let token = lfs.issue(&ctx, subject, request.operation, None);
        let href = format!("{}/{}.git/info/lfs", self.shared.config.server.base_url(), ctx.repo.full_name());
        let answer = serde_json::json!({
            "href": href,
            "header": { "Authorization": token.authorization },
            "expires_in": token.expires_in,
        });
        Ok(format!(
            "{answer}
"
        ))
    }

    /// `Hi alice! …`, shown to `ssh git@host`.
    fn greeting(&self) -> String {
        let site = self.shared.services.instance.current().site_name;
        match &self.identity {
            Some(SshIdentity::User { user, .. }) => {
                format!("Hi {}! You've successfully authenticated, but {site} does not provide shell access.\r\n", user.name())
            }
            Some(SshIdentity::Deploy { repo, .. }) => format!(
                "Hi {}! You've successfully authenticated with a deploy key, but {site} does not provide shell access.\r\n",
                repo.full_name()
            ),
            None => format!("{site} does not provide shell access.\r\n"),
        }
    }
}

/// Logs an unexpected error and returns the generic message shown to the client.
fn internal(error: &ServiceError) -> String {
    tracing::error!(%error, "SSH request failed");
    "internal error; please try again later".to_owned()
}

/// Accepts a request, shows `message` on the client's stderr and ends the channel with
/// `status`. Used for refusals, so the client sees *why* rather than a bare failure.
fn finish_with_message(session: &mut Session, channel: ChannelId, message: &str, status: u32) -> Result<(), russh::Error> {
    session.channel_success(channel)?;
    session.extended_data(channel, STDERR, message.as_bytes().to_vec())?;
    session.exit_status_request(channel, status)?;
    session.eof(channel)?;
    session.close(channel)
}

/// Accepts a request, writes `output` to stdout, and flushes before ending the channel.
fn finish_pending_with_output(
    session: &mut Session,
    channel: ChannelId,
    pending: PendingChannel,
    output: Vec<u8>,
) -> Result<(), russh::Error> {
    session.channel_success(channel)?;
    tokio::spawn(async move {
        let PendingChannel { channel, _slot, .. } = pending;
        let (_input, writer) = channel.split();
        let mut stdout = writer.make_writer();
        let _ = stdout.write_all(&output).await;
        let _ = stdout.flush().await;
        let _ = writer.exit_status(0).await;
        let _ = writer.eof().await;
        let _ = writer.close().await;
    });
    Ok(())
}

/// Accepts a request, writes `message` to stderr, and flushes before ending the channel.
fn finish_pending_with_message(
    session: &mut Session,
    channel: ChannelId,
    pending: PendingChannel,
    message: String,
    status: u32,
) -> Result<(), russh::Error> {
    session.channel_success(channel)?;
    tokio::spawn(async move {
        let PendingChannel { channel, _slot, .. } = pending;
        let (_input, writer) = channel.split();
        let mut stderr = writer.make_writer_ext(Some(STDERR));
        let _ = stderr.write_all(message.as_bytes()).await;
        let _ = stderr.flush().await;
        let _ = writer.exit_status(status).await;
        let _ = writer.eof().await;
        let _ = writer.close().await;
    });
    Ok(())
}

/// Ends a Git service with a flushed protocol error packet, so the client does not wait
/// for a ref advertisement that will never arrive.
fn finish_with_git_error(session: &mut Session, channel: ChannelId, pending: PendingChannel, message: String) -> Result<(), russh::Error> {
    session.channel_success(channel)?;
    tokio::spawn(async move {
        let PendingChannel { channel, _slot, .. } = pending;
        let (_input, writer) = channel.split();
        let mut stdout = writer.make_writer();
        let packet = rbit_transport::pkt_line::encode(&format!("ERR {message}\n"));
        let _ = stdout.write_all(&packet).await;
        let _ = stdout.flush().await;
        let _ = writer.exit_status(FAILURE).await;
        let _ = writer.eof().await;
        let _ = writer.close().await;
    });
    Ok(())
}

impl Handler for Connection {
    type Error = russh::Error;

    /// Called before the client proves it holds the private key: cheap refusals only.
    async fn auth_publickey_offered(&mut self, _user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        if self.is_throttled() {
            return Ok(Auth::reject());
        }
        if self.identify(key).await.is_some() {
            Ok(Auth::Accept)
        } else {
            self.shared.services.throttle.record_client_failure(ThrottleScope::SshAuth, &self.throttle_key());
            Ok(Auth::reject())
        }
    }

    /// Called once the signature is verified. The SSH user name is ignored: the key
    /// alone identifies the user (clone URLs say `git@`).
    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        if self.is_throttled() {
            return Ok(Auth::reject());
        }
        let Some(identity) = self.identify(key).await else {
            self.shared.services.throttle.record_client_failure(ThrottleScope::SshAuth, &self.throttle_key());
            return Ok(Auth::reject());
        };
        self.shared.services.throttle.reset_client(ThrottleScope::SshAuth, &self.throttle_key());
        self.shared.services.ssh_keys.record_use(&identity).await;
        match &identity {
            SshIdentity::User { user: account, .. } => {
                tracing::info!(peer = %self.peer, ssh_user = user, user = %account.name(), "SSH sign-in");
            }
            SshIdentity::Deploy { repo, .. } => {
                tracing::info!(peer = %self.peer, repo = %repo.full_name(), "SSH sign-in with a deploy key");
            }
        }
        self.identity = Some(identity);
        self.fingerprint = Some(rbit_services::ssh_keys::fingerprint(key));
        self.authenticated.store(true, Ordering::Release);
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let slot = if self.identity.is_some() { Arc::clone(&self.channel_slots).try_acquire_owned().ok() } else { None };
        match slot {
            Some(slot) => {
                self.channels.insert(channel.id(), PendingChannel { channel, protocol: None, _slot: slot });
                reply.accept().await;
            }
            None => reply.reject(ChannelOpenFailure::ResourceShortage).await,
        }
        Ok(())
    }

    async fn channel_close(&mut self, channel: ChannelId, _session: &mut Session) -> Result<(), Self::Error> {
        self.channels.remove(&channel);
        Ok(())
    }

    /// Only `GIT_PROTOCOL` (sent by `git` for protocol v2), and only safe values.
    async fn env_request(&mut self, channel: ChannelId, name: &str, value: &str, session: &mut Session) -> Result<(), Self::Error> {
        match self.channels.get_mut(&channel) {
            Some(pending) if name == "GIT_PROTOCOL" && is_safe_protocol_header(value) => {
                pending.protocol = Some(value.to_owned());
                session.channel_success(channel)
            }
            _ => session.channel_failure(channel),
        }
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_failure(channel)
    }

    async fn x11_request(
        &mut self,
        channel: ChannelId,
        _single_connection: bool,
        _x11_auth_protocol: &str,
        _x11_auth_cookie: &str,
        _x11_screen_number: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_failure(channel)
    }

    async fn subsystem_request(&mut self, channel: ChannelId, _name: &str, session: &mut Session) -> Result<(), Self::Error> {
        session.channel_failure(channel)
    }

    /// `ssh git@host`: say hello and close.
    async fn shell_request(&mut self, channel: ChannelId, session: &mut Session) -> Result<(), Self::Error> {
        if self.channels.remove(&channel).is_none() {
            return session.channel_failure(channel);
        }
        finish_with_message(session, channel, &self.greeting(), FAILURE)
    }

    async fn exec_request(&mut self, channel: ChannelId, data: &[u8], session: &mut Session) -> Result<(), Self::Error> {
        let (Some(pending), Some(identity)) = (self.channels.remove(&channel), self.identity.clone()) else {
            return session.channel_failure(channel);
        };
        let identity = match self.current_identity(&identity).await {
            Ok(identity) => identity,
            Err(message) => return finish_pending_with_message(session, channel, pending, format!("ERROR: {message}\r\n"), FAILURE),
        };
        self.identity = Some(identity.clone());
        // Milestone 3: Git LFS (L).
        if let Some(request) = command::parse_lfs_authenticate(data) {
            let answer = match request {
                Ok(request) => self.lfs_authenticate(&identity, &request).await,
                Err(error) => Err(error.to_string()),
            };
            return match answer {
                Ok(json) => finish_pending_with_output(session, channel, pending, json.into_bytes()),
                Err(message) => {
                    tracing::info!(peer = %self.peer, reason = %message, "SSH git-lfs-authenticate refused");
                    finish_pending_with_message(
                        session,
                        channel,
                        pending,
                        format!(
                            "ERROR: {message}
"
                        ),
                        FAILURE,
                    )
                }
            };
        }
        let command = match command::parse(data) {
            Ok(command) => command,
            Err(error) => {
                tracing::info!(peer = %self.peer, command = %String::from_utf8_lossy(data).chars().take(80).collect::<String>(), "SSH command refused");
                return finish_pending_with_message(session, channel, pending, format!("ERROR: {error}\r\n"), FAILURE);
            }
        };
        // Let receive-pack read the command list before it refuses missing write access.
        // Some Git clients wait forever when an SSH server closes before advertising refs.
        let ctx = match self.authorize(&identity, &command, false).await {
            Ok(ctx) => ctx,
            Err(message) => {
                tracing::info!(peer = %self.peer, repo = %command.full_name(), service = %command.service, reason = %message, "SSH Git request refused");
                return finish_with_git_error(session, channel, pending, message);
            }
        };
        session.channel_success(channel)?;
        tokio::spawn(run_git(Arc::clone(&self.shared), identity, command, ctx, pending));
        Ok(())
    }
}

/// A deleted key re-registered for another holder cannot turn an old authenticated
/// connection into that holder's session.
fn same_key_holder(previous: &SshIdentity, current: &SshIdentity) -> bool {
    match (previous, current) {
        (SshIdentity::User { key: old_key, user: old }, SshIdentity::User { key, user }) => old_key == key && old.id() == user.id(),
        (SshIdentity::Deploy { key: old_key, repo: old, .. }, SshIdentity::Deploy { key, repo, .. }) => {
            old_key == key && old.repo.id == repo.repo.id
        }
        _ => false,
    }
}

/// Runs the Git service for an authorised `exec` request, streaming between the channel
/// and git, then reports git's exit status and closes the channel.
async fn run_git(shared: Arc<Shared>, identity: SshIdentity, command: GitCommand, ctx: RepoContext, pending: PendingChannel) {
    let PendingChannel { channel, protocol, _slot } = pending;
    let (mut client, writer) = channel.split();
    let mut stdout = writer.make_writer();
    let mut stderr = writer.make_writer_ext(Some(STDERR));

    // Channel messages → git's stdin. Only data is forwarded; EOF ends git's input.
    let (input, mut feed) = duplex(INPUT_BUFFER);
    let pump = AbortOnDrop(tokio::spawn(async move {
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
    }));

    let started = Instant::now();
    let service = command.service;
    let exchange = async {
        if service.is_write() {
            push(&shared, &identity, &ctx, protocol.as_deref(), input, &mut stdout, &mut stderr).await
        } else {
            let status =
                shared.transport.serve_interactive(service, &ctx.disk_path, protocol.as_deref(), input, &mut stdout, &mut stderr).await?;
            Ok(Exchanged { status: exit_code(status), push: None })
        }
    };
    let outcome = tokio::time::timeout(shared.config.ssh.max_session(), exchange).await;
    let (status, push) = match outcome {
        Ok(Ok(Exchanged { status, push })) => (status, push),
        Ok(Err(error)) => {
            tracing::error!(%error, repo = %command.full_name(), %service, "failed to run git");
            let _ = stderr.write_all(b"ERROR: internal error; please try again later\r\n").await;
            (FAILURE, None)
        }
        Err(_) => {
            tracing::warn!(repo = %command.full_name(), %service, "SSH Git operation exceeded the time limit");
            let _ = stderr.write_all(b"ERROR: the operation took too long and was stopped\r\n").await;
            (FAILURE, None)
        }
    };
    drop(pump);
    let _ = stdout.flush().await;
    let _ = stderr.flush().await;

    if let Some(push) = push.filter(|_| status == 0) {
        shared.services.pushes.complete(push).await;
    }

    let _ = writer.exit_status(status).await;
    let _ = writer.eof().await;
    let _ = writer.close().await;

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
    input: DuplexStream,
    stdout: &mut O,
    stderr: &mut E,
) -> Result<Exchanged, TransportError>
where
    O: AsyncWrite + Unpin,
    E: AsyncWrite + Unpin,
{
    let refused = |status| Ok(Exchanged { status, push: None });
    let advertisement = shared.transport.advertisement(Service::ReceivePack, &ctx.disk_path, protocol).await?;
    stdout.write_all(&advertisement).await?;
    stdout.flush().await?;

    let (user, deploy_key) = match identity {
        SshIdentity::User { user, .. } => (Some(user), false),
        SshIdentity::Deploy { .. } => (None, true),
    };
    let (input, options): (Box<dyn AsyncRead + Send + Unpin>, ReceiveOptions) =
        match receive::intercept(input, CommandLimits::default()).await {
            Err(error) => {
                stderr.write_all(format!("ERROR: {error}\r\n").as_bytes()).await?;
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
                        stderr.write_all(format!("ERROR: {}\r\n", internal(&error)).as_bytes()).await?;
                        return refused(FAILURE);
                    }
                };
                if deploy_key && !ctx.access.can_write() {
                    let reason = format!("this deploy key is read-only; it cannot push to {}", ctx.repo.full_name());
                    for refusal in &mut verdict.refusals {
                        refusal.reason.clone_from(&reason);
                    }
                }
                if verdict.is_refused() {
                    receive::drain(input).await;
                    let report = receive::refusal(&commands, &verdict.refusals);
                    if report.is_empty() {
                        let message = receive::refusal_message(&verdict.refusals).replace('\n', "\r\n");
                        stderr.write_all(format!("ERROR: push declined\r\n{message}").as_bytes()).await?;
                        return refused(FAILURE);
                    }
                    stdout.write_all(&report).await?;
                    // Like a declining pre-receive hook: git reports and exits successfully.
                    return refused(0);
                }
                (Box::new(input), ReceiveOptions { deny_non_fast_forwards: verdict.deny_non_fast_forwards })
            }
        };

    // Taken after the verdict, before git changes anything.
    let pending = shared.services.pushes.begin(ctx, user.map(User::id)).await;
    let status = shared.transport.receive_pack(&ctx.disk_path, protocol, options, input, stdout, stderr).await?;
    Ok(Exchanged { status: exit_code(status), push: Some(pending) })
}

/// Aborts a background task when dropped, so it never outlives the request it serves.
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
