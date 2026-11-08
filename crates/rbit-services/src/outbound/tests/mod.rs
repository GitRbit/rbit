//! Hostile-input regressions for outbound requests: webhook deliveries, imports, pull
//! and push mirrors and sign-in provider discovery. Every server is a local listener, and
//! names resolve through [`super::scripted`], never the public internet.
//!
//! Under the strict policy (`allow_private_addresses = false`) a local listener is an
//! internal address, so strict tests prove refusals: the listener is never contacted.
//! Mechanics that need a reachable server (pinning, redirects, limits) run with private
//! addresses allowed, which changes only the address classification.

mod addresses;
mod git;
mod limits;
mod proxies;
mod rebinding;
mod redirects;

use std::{
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use rbit_core::{Config, User, Visibility, webhook::WebhookUrl};
use rbit_storage::{Database, KvStore, NewUser};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use crate::{CreateRepository, Services, repositories::RepoContext, webhooks::HookTarget};

/// The loopback address every test server listens on.
pub(super) const LOOPBACK: IpAddr = IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
/// A public address. Strict checks may classify it; no test connects to it.
pub(super) const PUBLIC: IpAddr = IpAddr::V4(std::net::Ipv4Addr::new(93, 184, 215, 14));

/// What a [`Server`] does with a request once it has read its head.
#[derive(Debug, Clone)]
pub(super) enum Reply {
    /// Writes these bytes and closes.
    Bytes(Vec<u8>),
    /// Never answers; waits for the client to close.
    Hang,
    /// Writes this head, then up to this many body bytes while the client reads them,
    /// then waits for the client to close.
    Large(String, u64),
}

impl Reply {
    pub(super) fn status(status: &str, headers: &str, body: &[u8]) -> Self {
        let head = format!("HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n", body.len());
        Self::Bytes([head.as_bytes(), body].concat())
    }

    pub(super) fn redirect(location: &str) -> Self {
        Self::status("302 Found", &format!("Location: {location}\r\n"), b"")
    }
}

/// One accepted connection.
#[derive(Debug, Default)]
pub(super) struct Connection {
    pub head: String,
    /// Body bytes written by an endless reply.
    pub sent: u64,
    /// When the client closed the connection, measured from the accept.
    pub closed_after: Option<Duration>,
}

type Log = Arc<Mutex<Vec<Arc<Mutex<Connection>>>>>;

/// A raw HTTP/1.1 listener on 127.0.0.1 that records every connection.
#[derive(Debug)]
pub(super) struct Server {
    pub address: SocketAddr,
    log: Log,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    pub(super) async fn start(reply: impl Fn(&str) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind((LOOPBACK, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let log = Log::default();
        let reply: Arc<dyn Fn(&str) -> Reply + Send + Sync> = Arc::new(reply);
        let accepted = Arc::clone(&log);
        let task = tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let connection = Arc::new(Mutex::new(Connection::default()));
                lock(&accepted).push(Arc::clone(&connection));
                tokio::spawn(serve(socket, Arc::clone(&reply), connection));
            }
        });
        Self { address, log, task }
    }

    /// A server that answers every request with `reply`.
    pub(super) async fn always(reply: Reply) -> Self {
        Self::start(move |_| reply.clone()).await
    }

    pub(super) fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }

    pub(super) fn connections(&self) -> usize {
        lock(&self.log).len()
    }

    pub(super) fn heads(&self) -> Vec<String> {
        lock(&self.log).iter().map(|connection| lock(connection).head.clone()).collect()
    }

    /// Body bytes written by large replies.
    pub(super) fn sent(&self) -> u64 {
        lock(&self.log).iter().map(|connection| lock(connection).sent).sum()
    }

    /// Waits until every accepted connection was closed by its client; false after `within`.
    pub(super) async fn all_closed(&self, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        loop {
            let open = lock(&self.log).iter().any(|connection| lock(connection).closed_after.is_none());
            if !open {
                return true;
            }
            if Instant::now() > deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(mut socket: TcpStream, reply: Arc<dyn Fn(&str) -> Reply + Send + Sync>, connection: Arc<Mutex<Connection>>) {
    let accepted = Instant::now();
    let mut head = Vec::new();
    let mut buffer = [0_u8; 4096];
    while !head.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
        match socket.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => head.extend_from_slice(&buffer[..read]),
        }
    }
    let text = String::from_utf8_lossy(&head).into_owned();
    lock(&connection).head.clone_from(&text);
    match reply(&text) {
        Reply::Bytes(bytes) => {
            let _ = socket.write_all(&bytes).await;
            let _ = socket.shutdown().await;
            drain(&mut socket).await;
        }
        Reply::Hang => drain(&mut socket).await,
        Reply::Large(head, total) => {
            let _ = socket.write_all(head.as_bytes()).await;
            let chunk = vec![b' '; 64 * 1024];
            let mut sent = 0;
            while sent < total && socket.write_all(&chunk).await.is_ok() {
                sent += chunk.len() as u64;
                lock(&connection).sent = sent;
            }
            drain(&mut socket).await;
        }
    }
    lock(&connection).closed_after = Some(accepted.elapsed());
}

/// Reads until the client closes its side.
async fn drain(socket: &mut TcpStream) {
    let mut buffer = [0_u8; 4096];
    while matches!(socket.read(&mut buffer).await, Ok(1..)) {}
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A distinct scripted name for one test.
pub(super) fn name(test: &str) -> String {
    format!("{test}.rbit-outbound.invalid")
}

// ------------------------------------------------------------------ services

/// Real services over temporary `SQLite` and redb stores.
pub(super) struct Fixture {
    _dir: tempfile::TempDir,
    pub database: Database,
    pub services: Services,
}

impl Fixture {
    /// Every outbound policy starts strict; `configure` may relax it.
    pub(super) async fn new(configure: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.updates.check_for_updates = false;
        config.storage.data_dir = dir.path().join("data");
        "https://git.example.com".clone_into(&mut config.server.public_url);
        configure(&mut config);
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        Self { _dir: dir, database, services }
    }

    pub(super) async fn user(&self, name: &str) -> User {
        let user = NewUser {
            name: name.parse().unwrap(),
            email: format!("{name}@example.com").parse().unwrap(),
            display_name: None,
            password_hash: "not-a-real-hash".to_owned(),
            is_admin: false,
        };
        self.database.accounts().create_user(user).await.unwrap()
    }

    /// `owner/name`, owned by `actor` (whose name is `owner`).
    pub(super) async fn repo(&self, actor: &User, owner: &str, name: &str) -> RepoContext {
        let input = CreateRepository { name: name.to_owned(), description: String::new(), visibility: Visibility::Private };
        self.services.repositories.create_in(actor, owner, input).await.unwrap();
        self.services.repositories.open(Some(actor), owner, name).await.unwrap()
    }

    /// Stores a hook for `url` directly (as one saved before a policy or DNS change),
    /// pings it and delivers the ping once; returns the recorded delivery.
    pub(super) async fn deliver_ping(&self, actor: &User, repo: &RepoContext, url: &str) -> rbit_core::webhook::Delivery {
        use rbit_core::webhook::{WebhookEvent, WebhookOwner};
        let new = rbit_storage::sqlite::webhooks::NewWebhook {
            id: None,
            owner: WebhookOwner::Repository(repo.repo.repo.id),
            url: WebhookUrl::parse(url).unwrap(),
            secret: None,
            events: vec![WebhookEvent::Push],
            active: true,
            created_by: None,
        };
        let hook = self.database.webhooks().create(new).await.unwrap();
        let target = HookTarget::Repository(repo);
        let ping = self.services.webhooks.ping(actor, target, hook.id).await.unwrap();
        self.services.webhooks.deliver_due(rbit_core::Timestamp(rbit_core::Timestamp::now().seconds() + 365 * 24 * 60 * 60)).await.unwrap();
        self.services.webhooks.delivery(target, hook.id, ping.id).await.unwrap()
    }
}
