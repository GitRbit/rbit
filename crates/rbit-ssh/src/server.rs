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

use rbit_core::Config;
use rbit_services::{Services, ThrottleScope};
use rbit_transport::ProcessTransport;
use russh::{
    Disconnect, MethodKind, MethodSet, SshId,
    keys::{HashAlg, PrivateKey},
    server::{Config as RusshConfig, run_stream},
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore, watch},
    task::{JoinHandle, JoinSet},
};

use crate::{SshError, host_key, session::Connection, tasks::SshTasks};

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
    pub(crate) tasks: SshTasks,
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
    task: Option<JoinHandle<()>>,
    tasks: SshTasks,
}

impl RunningSshServer {
    /// The address actually bound (useful with port 0).
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Stops accepting connections and waits for owned SSH handlers and Git work.
    /// Network grace is bounded; started process/store cleanup is still awaited.
    /// Cancelling this wait does not establish that descendants have stopped.
    pub async fn shutdown(mut self) {
        self.tasks.stop();
        let _ = self.shutdown.send(true);
        if let Some(task) = self.task.take()
            && let Err(error) = task.await
        {
            tracing::warn!(%error, "SSH server task failed");
        }
        // Even a panicked listener can have children awaiting cancellation.
        self.tasks.close();
        self.tasks.wait().await;
    }
}

impl Drop for RunningSshServer {
    fn drop(&mut self) {
        // Dropping/unpolled-cancelling shutdown requests stop immediately. The
        // detached listener still owns its cleanup; Drop never claims a drain.
        self.tasks.close();
        let _ = self.shutdown.send(true);
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
    let transport = ProcessTransport::new(config.repository.git_binary.clone()).with_max_push_bytes(config.repository.max_push_bytes());
    let connections = Arc::new(Semaphore::new(config.ssh.max_connections.max(1)));
    let tasks = SshTasks::new();
    let shared = Arc::new(Shared { config, services, transport, tasks: tasks.clone(), connections, per_ip: Mutex::new(HashMap::new()) });

    let (shutdown, shutdown_rx) = watch::channel(false);
    let task = tokio::spawn(accept_loop(listener, shared, russh_config, shutdown_rx));
    Ok(RunningSshServer { local_addr, shutdown, task: Some(task), tasks })
}

fn russh_config(config: &Config, host_key: PrivateKey) -> RusshConfig {
    RusshConfig {
        server_id: SshId::Standard(Cow::Borrowed(if config.updates.report_version {
            concat!("SSH-2.0-rbit_", env!("CARGO_PKG_VERSION"))
        } else {
            "SSH-2.0-rbit"
        })),
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
    let _stop_on_drop = shared.tasks.stop_on_drop();
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
                    shared.tasks.connection(&mut connections, async move {
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
    shared.tasks.stop();
    tracing::info!(open = connections.len(), "SSH server is shutting down");
    if tokio::time::timeout(SHUTDOWN_GRACE, async { while connections.join_next().await.is_some() {} }).await.is_err() {
        connections.abort_all();
    }
    // abort_all only requests cancellation; joins acknowledge future destruction.
    while connections.join_next().await.is_some() {}
    shared.tasks.close();
    shared.tasks.wait().await;
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
    let Some(handler) = Connection::new(Arc::clone(&shared), peer, Arc::clone(&authenticated)) else { return };
    let stop = shared.tasks.cancellation().child_token();
    let close_stream = crate::socket::CloseOnDrop(stop.clone());
    let socket = crate::socket::CancellableSocket::new(socket, stop);

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
    let (finished, mut session_done) = tokio::sync::oneshot::channel();
    // This independent, tracked owner is never aborted with its network waiter.
    // Awaiting RunningSession joins russh's actual internal task/future destruction.
    shared.tasks.session(async move {
        let _ = finished.send(session.await);
    });
    let mut late_shutdown = shutdown.clone();

    let reason = tokio::select! {
        result = &mut session_done => {
            match result {
                Ok(Ok(())) => {},
                Ok(Err(error)) => tracing::debug!(%peer, %error, "SSH connection ended with an error"),
                Err(error) => tracing::warn!(%peer, %error, "SSH session join owner failed"),
            }
            return;
        }
        () = tokio::time::sleep(auth_timeout), if !authenticated.load(Ordering::Acquire) => {
            if authenticated.load(Ordering::Acquire) {
                // Authenticated just in time; keep serving until the client leaves.
                tokio::select! {
                    _ = &mut session_done => return,
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
    if tokio::time::timeout(Duration::from_secs(5), &mut session_done).await.is_err() {
        // Interrupt underlying I/O, then retain ownership until the real session exits.
        close_stream.0.cancel();
        let _ = session_done.await;
    }
}

/// Resolves once shutdown has been requested (or the server is gone).
async fn stopped(shutdown: &mut watch::Receiver<bool>) {
    let _ = shutdown.wait_for(|stop| *stop).await;
}
