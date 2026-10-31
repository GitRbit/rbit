//! Wiring: open storage, build services, run the HTTP server, shut down cleanly.

use std::{io::BufRead, sync::Arc, time::Duration};

use anyhow::Context;
use axum::serve::{ListenerExt, TapIo};
use rbit_core::Config;
use rbit_services::{Services, SignUp, accounts};
use rbit_storage::{Database, KvStore};
use rbit_web::{AppState, Restart};
use tokio::net::{TcpListener, TcpStream};

/// How often expired sessions are purged from the key/value store.
const SESSION_PURGE_INTERVAL: Duration = Duration::from_hours(1);

/// How long open connections may take to finish once a restart is requested. Graceful
/// shutdown otherwise waits for every connection, and a log stream or a slow clone could
/// hold the restart up indefinitely.
const RESTART_DRAIN: Duration = Duration::from_secs(10);

/// Why [`run`] returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    /// Ctrl+C or SIGTERM: exit.
    Shutdown,
    /// An administrator asked for a restart: start again.
    Restart,
}

/// Opened persistent stores.
struct Storage {
    database: Database,
    kv: KvStore,
}

async fn open_storage(config: &Config) -> anyhow::Result<Storage> {
    let database = open_database(config).await?;
    let kv = KvStore::open(&config.storage.kv_path()).context("cannot open key/value store")?;
    Ok(Storage { database, kv })
}

/// Opens (and migrates) only the `SQLite` database. The CLI commands use this alone:
/// the key/value store is held exclusively by a running server, `SQLite` is not.
async fn open_database(config: &Config) -> anyhow::Result<Database> {
    let storage = &config.storage;
    tokio::fs::create_dir_all(storage.repositories_dir())
        .await
        .with_context(|| format!("cannot create data directory {}", storage.data_dir.display()))?;
    Database::open_with_key_file(&storage.sqlite_path(), &storage.secret_key_path()).await.context("cannot open SQLite database")
}

pub async fn run(config: Config) -> anyhow::Result<Stopped> {
    // Refuse to start rather than fail every page.
    rbit_web::assets::ensure_built().map_err(anyhow::Error::msg)?;
    let config = Arc::new(config);
    let storage = open_storage(&config).await?;
    let services = Services::new(Arc::clone(&config), &storage.database, &storage.kv).await?;

    spawn_session_purger(storage.kv.clone());
    services.spawn_background();
    let background = services.clone();
    let state = AppState::new(Arc::clone(&config), services);
    let restart = state.restart.clone();
    crate::ssh::start(&config, &background, &restart);

    let app = rbit_web::router(state);
    let listener =
        http_listener(TcpListener::bind(config.server.bind).await.with_context(|| format!("cannot bind {}", config.server.bind))?);
    tracing::info!(address = %config.server.bind, public_url = %config.server.public_url, data_dir = %config.storage.data_dir.display(), "rbit is listening");
    log_setup_token(&config, &background);

    let server = axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .with_graceful_shutdown(shutdown_signal(restart.clone()))
        .into_future();
    let drain_deadline = async {
        restart.requested().await;
        tokio::time::sleep(RESTART_DRAIN).await;
    };
    tokio::select! {
        result = server => result.context("server error")?,
        () = drain_deadline => tracing::warn!("connections still open after {RESTART_DRAIN:?}; restarting without them"),
    }

    tracing::info!("shutting down");
    // Stop background work before closing the stores it writes to.
    background.shutdown().await;
    storage.database.close().await;
    Ok(if restart.is_requested() { Stopped::Restart } else { Stopped::Shutdown })
}

/// On an instance without users, prints what the first (administrator) sign-up needs.
fn log_setup_token(config: &Config, services: &Services) {
    let Some(token) = services.accounts.setup_token() else { return };
    let url = format!("{}/signup", config.server.public_url.trim_end_matches('/'));
    // A configured token is already known to whoever deploys the server.
    let token = if config.security.setup_token.as_deref().is_some_and(|configured| configured.trim() == token) {
        "(security.setup_token)"
    } else {
        token
    };
    tracing::warn!(
        setup_url = %url,
        setup_token = %token,
        "no account exists yet: sign up at the setup URL with this setup token to create the administrator \
         (or run `rbit user create <name> <email> --admin`)"
    );
}

pub async fn migrate(config: Config) -> anyhow::Result<()> {
    let database = open_database(&config).await?;
    database.close().await;
    tracing::info!("database is up to date");
    Ok(())
}

pub async fn create_user(config: Config, username: String, email: String, admin: bool) -> anyhow::Result<()> {
    let database = open_database(&config).await?;

    eprintln!("Password for {username} (read from stdin):");
    let mut password = String::new();
    std::io::stdin().lock().read_line(&mut password).context("cannot read password")?;
    let password = password.trim_end_matches(['\r', '\n']).to_owned();

    let input = SignUp { username, email, password };
    let user = accounts::create_user_offline(&config, &database.accounts(), input, Some(admin)).await?;
    tracing::info!(user = %user.name(), admin, "user created");
    database.close().await;
    Ok(())
}

/// Accepted connections with Nagle's algorithm off. Responses are written in several
/// small pieces (headers, then body frames); with Nagle on, a piece that follows an
/// unacknowledged one waits for the peer's delayed ACK before it is sent.
fn http_listener(listener: TcpListener) -> TapIo<TcpListener, fn(&mut TcpStream)> {
    listener.tap_io(disable_nagle as fn(&mut TcpStream))
}

fn disable_nagle(stream: &mut TcpStream) {
    if let Err(error) = stream.set_nodelay(true) {
        tracing::debug!(%error, "cannot set TCP_NODELAY on an accepted connection");
    }
}

fn spawn_session_purger(kv: KvStore) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(SESSION_PURGE_INTERVAL);
        loop {
            interval.tick().await;
            match kv.sessions().purge_expired().await {
                Ok(0) => {}
                Ok(removed) => tracing::info!(removed, "purged expired sessions"),
                Err(error) => tracing::warn!(%error, "session purge failed"),
            }
        }
    });
}

/// Resolves on Ctrl+C, SIGTERM on Unix (as sent by systemd and container runtimes), or an
/// administrator's restart.
pub async fn shutdown_signal(restart: Restart) {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
        () = restart.requested() => {},
    }
}

#[cfg(test)]
mod tests {
    use axum::serve::Listener as _;

    use super::*;

    #[tokio::test]
    async fn accepted_connections_disable_nagle() {
        let mut listener = http_listener(TcpListener::bind("127.0.0.1:0").await.unwrap());
        let address = listener.local_addr().unwrap();
        let _client = tokio::net::TcpStream::connect(address).await.unwrap();
        let (stream, _) = listener.accept().await;
        assert!(stream.nodelay().unwrap());
    }
}
