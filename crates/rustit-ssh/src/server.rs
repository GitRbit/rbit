//! The listener: accepts TCP connections, enforces connection limits and timeouts, and
//! hands each admitted connection to a russh session driven by [`Connection`].

use std::{
    borrow::Cow,
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use russh::{
    Disconnect, MethodKind, MethodSet, SshId,
    keys::{HashAlg, PrivateKey},
    server::{Config as RusshConfig, run_stream},
};
use rustit_core::Config;
use rustit_services::{Services, ThrottleScope};
use rustit_transport::ProcessTransport;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore, watch},
    task::{JoinHandle, JoinSet},
};

use crate::{SshError, host_key, session::Connection};

/// How long open connections get to wind down after shutdown is requested.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
/// Pause after a failed `accept` (e.g. out of file descriptors) so the loop cannot spin.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

/// What every connection shares.
#[derive(Debug)]
pub(crate) struct Shared {
    pub(crate) config: Arc<Config>,
    pub(crate) services: Services,
    pub(crate) transport: ProcessTransport,
    connections: Arc<Semaphore>,
    per_ip: Mutex<HashMap<IpAddr, usize>>,
}

impl Shared {
    /// Admits a new connection from `ip` if the limits allow it.
    fn admit(self: &Arc<Self>, ip: IpAddr) -> Option<ConnectionSlot> {
        if self.services.throttle.check_client(ThrottleScope::SshAuth, &ip.to_string()).is_err() {
            tracing::debug!(%ip, "SSH connection refused: too many failed attempts");
            return None;
        }
        let permit = Arc::clone(&self.connections).try_acquire_owned().ok()?;
        let mut per_ip = self.per_ip.lock().unwrap_or_else(PoisonError::into_inner);
        let count = per_ip.entry(ip).or_default();
        if *count >= self.config.ssh.max_connections_per_ip {
            tracing::debug!(%ip, "SSH connection refused: too many connections from this address");
            return None;
        }
        *count += 1;
        Some(ConnectionSlot { shared: Arc::clone(self), ip, _permit: permit })
    }
}

/// A connection's place within the limits; released when dropped.
#[derive(Debug)]
struct ConnectionSlot {
    shared: Arc<Shared>,
    ip: IpAddr,
    _permit: OwnedSemaphorePermit,
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        let mut per_ip = self.shared.per_ip.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = per_ip.get_mut(&self.ip) {
            *count -= 1;
            if *count == 0 {
                per_ip.remove(&self.ip);
            }
        }
    }
}

/// A running SSH server. Call [`RunningSshServer::shutdown`] to stop it.
#[derive(Debug)]
pub struct RunningSshServer {
    local_addr: SocketAddr,
    shutdown: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl RunningSshServer {
    /// The address actually bound (useful with port 0).
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Stops accepting connections, disconnects open ones and waits (briefly) for them
    /// to finish.
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(true);
        if let Err(error) = self.task.await {
            tracing::warn!(%error, "SSH server task failed");
        }
    }
}

/// Loads (or creates) the host key, binds `[ssh] bind` and starts serving.
pub async fn start(config: Arc<Config>, services: Services) -> Result<RunningSshServer, SshError> {
    let key_path = config.ssh.host_key_path(&config.storage);
    let host_key = tokio::task::spawn_blocking(move || host_key::load_or_generate(&key_path)).await.map_err(SshError::Join)??;
    let listener = TcpListener::bind(config.ssh.bind).await.map_err(|source| SshError::Bind { address: config.ssh.bind, source })?;
    let local_addr = listener.local_addr().map_err(|source| SshError::Bind { address: config.ssh.bind, source })?;
    tracing::info!(
        address = %local_addr,
        host_key = %host_key.public_key().fingerprint(HashAlg::Sha256),
        "SSH server is listening"
    );

    let russh_config = Arc::new(russh_config(&config, host_key));
    let transport = ProcessTransport::new(config.repository.git_binary.clone());
    let connections = Arc::new(Semaphore::new(config.ssh.max_connections.max(1)));
    let shared = Arc::new(Shared { config, services, transport, connections, per_ip: Mutex::new(HashMap::new()) });

    let (shutdown, shutdown_rx) = watch::channel(false);
    let task = tokio::spawn(accept_loop(listener, shared, russh_config, shutdown_rx));
    Ok(RunningSshServer { local_addr, shutdown, task })
}

fn russh_config(config: &Config, host_key: PrivateKey) -> RusshConfig {
    RusshConfig {
        server_id: SshId::Standard(Cow::Borrowed(concat!("SSH-2.0-Rustit_", env!("CARGO_PKG_VERSION")))),
        methods: MethodSet::from(&[MethodKind::PublicKey][..]),
        // Clients probe with "none" first; answering that at once keeps sign-in fast,
        // while real rejections still take a constant second.
        auth_rejection_time_initial: Some(Duration::ZERO),
        keys: vec![host_key],
        max_auth_attempts: 6,
        inactivity_timeout: Some(config.ssh.idle_timeout()),
        nodelay: true,
        ..RusshConfig::default()
    }
}

async fn accept_loop(listener: TcpListener, shared: Arc<Shared>, russh_config: Arc<RusshConfig>, mut shutdown: watch::Receiver<bool>) {
    let mut connections = JoinSet::new();
    let for_connections = shutdown.clone();
    loop {
        tokio::select! {
            () = stopped(&mut shutdown) => break,
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
            accepted = listener.accept() => match accepted {
                Ok((socket, peer)) => {
                    let Some(slot) = shared.admit(peer.ip()) else {
                        drop(socket);
                        continue;
                    };
                    let connection = serve_connection(Arc::clone(&shared), Arc::clone(&russh_config), socket, peer, for_connections.clone());
                    connections.spawn(async move {
                        connection.await;
                        drop(slot);
                    });
                }
                Err(error) => {
                    tracing::warn!(%error, "failed to accept an SSH connection");
                    tokio::time::sleep(ACCEPT_BACKOFF).await;
                }
            },
        }
    }

    drop(listener);
    tracing::info!(open = connections.len(), "SSH server is shutting down");
    if tokio::time::timeout(SHUTDOWN_GRACE, async { while connections.join_next().await.is_some() {} }).await.is_err() {
        connections.abort_all();
    }
}

/// Runs one connection to completion: handshake, authentication within the time limit,
/// then whatever the client asks for, until it leaves or the server shuts down.
async fn serve_connection(
    shared: Arc<Shared>,
    russh_config: Arc<RusshConfig>,
    socket: TcpStream,
    peer: SocketAddr,
    mut shutdown: watch::Receiver<bool>,
) {
    let auth_timeout = shared.config.ssh.auth_timeout();
    let authenticated = Arc::new(AtomicBool::new(false));
    let handler = Connection::new(Arc::clone(&shared), peer, Arc::clone(&authenticated));

    let session = match tokio::time::timeout(auth_timeout, run_stream(russh_config, socket, handler)).await {
        Ok(Ok(session)) => session,
        Ok(Err(error)) => {
            tracing::debug!(%peer, %error, "SSH handshake failed");
            return;
        }
        Err(_) => {
            tracing::debug!(%peer, "SSH handshake timed out");
            return;
        }
    };
    let handle = session.handle();
    let mut session = std::pin::pin!(session);
    let mut late_shutdown = shutdown.clone();

    let reason = tokio::select! {
        result = &mut session => {
            if let Err(error) = result {
                tracing::debug!(%peer, %error, "SSH connection ended with an error");
            }
            return;
        }
        () = tokio::time::sleep(auth_timeout), if !authenticated.load(Ordering::Acquire) => {
            if authenticated.load(Ordering::Acquire) {
                // Authenticated just in time; keep serving until the client leaves.
                tokio::select! {
                    _ = &mut session => return,
                    () = stopped(&mut late_shutdown) => "the server is shutting down",
                }
            } else {
                tracing::debug!(%peer, "SSH client did not authenticate in time");
                "authentication timed out"
            }
        }
        () = stopped(&mut shutdown) => "the server is shutting down",
    };

    let _ = handle.disconnect(Disconnect::ByApplication, reason.to_owned(), String::new()).await;
    let _ = tokio::time::timeout(Duration::from_secs(5), session).await;
}

/// Resolves once shutdown has been requested (or the server is gone).
async fn stopped(shutdown: &mut watch::Receiver<bool>) {
    let _ = shutdown.wait_for(|stop| *stop).await;
}
