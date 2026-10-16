//! One client connection: public-key authentication, session channels and the Git
//! commands run on them.
//!
//! Only `session` channels are accepted, and on them only `exec` of a Git service, an
//! `env` request for `GIT_PROTOCOL`, and `shell` (which greets the user and closes).
//! PTYs, X11, agent forwarding, subsystems and every kind of port forwarding are refused.
//! Nothing a client sends is ever passed to a shell.

// Every handler method is written as `async fn` for uniformity, whether or not it awaits.
#![allow(clippy::unused_async_trait_impl)]

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use russh::{
    Channel, ChannelId, ChannelMsg, ChannelOpenFailure, Pty,
    keys::PublicKey,
    server::{Auth, ChannelOpenHandle, Handler, Msg, Session},
};
use rustit_services::{RepoContext, ServiceError, ThrottleScope, ssh_keys::SshIdentity};
use rustit_transport::is_safe_protocol_header;
use tokio::{
    io::{AsyncWriteExt, duplex},
    sync::{OwnedSemaphorePermit, Semaphore},
    task::JoinHandle,
};

use crate::{
    command::{self, GitCommand},
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
        let fingerprint = rustit_services::ssh_keys::fingerprint(key);
        match self.shared.services.ssh_keys.find_by_fingerprint(&fingerprint).await {
            Ok(identity) => identity,
            Err(error) => {
                tracing::error!(%error, peer = %self.peer, "SSH key lookup failed");
                None
            }
        }
    }

    /// Opens the repository named by `command` with exactly the access HTTP would grant.
    /// The error is the message shown to the client.
    async fn authorize(&self, identity: &SshIdentity, command: &GitCommand) -> Result<RepoContext, String> {
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
                services.repositories.open(Some(user), &command.owner, &command.repo).await
            }
            SshIdentity::Deploy { .. } => services.ssh_keys.open_for_deploy_key(identity, &command.owner, &command.repo).await,
        };
        let ctx = match opened {
            Ok(ctx) => ctx,
            Err(ServiceError::NotFound) => return Err(format!("repository {full_name} not found, or you do not have access to it")),
            Err(error) => return Err(internal(&error)),
        };
        if command.service.is_write() && !ctx.access.can_write() {
            return Err(match identity {
                SshIdentity::Deploy { .. } => format!("this deploy key is read-only; it cannot push to {full_name}"),
                SshIdentity::User { .. } => format!("you do not have permission to push to {full_name}"),
            });
        }
        Ok(ctx)
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
        let command = match command::parse(data) {
            Ok(command) => command,
            Err(error) => {
                tracing::info!(peer = %self.peer, command = %String::from_utf8_lossy(data).chars().take(80).collect::<String>(), "SSH command refused");
                return finish_with_message(session, channel, &format!("ERROR: {error}\r\n"), FAILURE);
            }
        };
        let ctx = match self.authorize(&identity, &command).await {
            Ok(ctx) => ctx,
            Err(message) => {
                tracing::info!(peer = %self.peer, repo = %command.full_name(), service = %command.service, reason = %message, "SSH Git request refused");
                return finish_with_message(session, channel, &format!("ERROR: {message}\r\n"), FAILURE);
            }
        };
        session.channel_success(channel)?;
        tokio::spawn(run_git(Arc::clone(&self.shared), identity, command, ctx, pending));
        Ok(())
    }
}

/// Runs the Git service for an authorised `exec` request, streaming between the channel
/// and git, then reports git's exit status and closes the channel.
async fn run_git(shared: Arc<Shared>, identity: SshIdentity, command: GitCommand, ctx: RepoContext, pending: PendingChannel) {
    let PendingChannel { channel, protocol, _slot } = pending;
    let (mut client, writer) = channel.split();
    let stdout = writer.make_writer();
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
    let pending = if service.is_write() {
        let pusher = match &identity {
            SshIdentity::User { user, .. } => Some(user.id()),
            SshIdentity::Deploy { .. } => None,
        };
        Some(shared.services.pushes.begin(&ctx, pusher).await)
    } else {
        None
    };
    let exchange = shared.transport.serve_interactive(service, &ctx.disk_path, protocol.as_deref(), input, stdout, &mut stderr);
    let status = match tokio::time::timeout(shared.config.ssh.max_session(), exchange).await {
        Ok(Ok(status)) => status.code().and_then(|code| u32::try_from(code).ok()).unwrap_or(FAILURE),
        Ok(Err(error)) => {
            tracing::error!(%error, repo = %command.full_name(), %service, "failed to run git");
            let _ = stderr.write_all(b"ERROR: internal error; please try again later\r\n").await;
            FAILURE
        }
        Err(_) => {
            tracing::warn!(repo = %command.full_name(), %service, "SSH Git operation exceeded the time limit");
            let _ = stderr.write_all(b"ERROR: the operation took too long and was stopped\r\n").await;
            FAILURE
        }
    };
    drop(pump);
    let _ = stderr.flush().await;

    if let Some(pending) = pending.filter(|_| status == 0) {
        shared.services.pushes.complete(pending).await;
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

/// Aborts a background task when dropped, so it never outlives the request it serves.
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
