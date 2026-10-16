//! Wiring: open storage, build services, run the HTTP server, shut down cleanly.

use std::{io::BufRead, sync::Arc, time::Duration};

use anyhow::Context;
use rustit_core::Config;
use rustit_services::{Services, SignUp, accounts};
use rustit_storage::{Database, KvStore};
use rustit_web::AppState;
use tokio::net::TcpListener;

/// How often expired sessions are purged from the key/value store.
const SESSION_PURGE_INTERVAL: Duration = Duration::from_secs(60 * 60);

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

/// Opens (and migrates) only the SQLite database. The CLI commands use this alone:
/// the key/value store is held exclusively by a running server, SQLite is not.
async fn open_database(config: &Config) -> anyhow::Result<Database> {
    let storage = &config.storage;
    tokio::fs::create_dir_all(storage.repositories_dir())
        .await
        .with_context(|| format!("cannot create data directory {}", storage.data_dir.display()))?;
    Database::open(&storage.sqlite_path()).await.context("cannot open SQLite database")
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let config = Arc::new(config);
    let storage = open_storage(&config).await?;
    let services = Services::new(Arc::clone(&config), &storage.database, &storage.kv).await?;

    spawn_session_purger(storage.kv.clone());
    services.spawn_background();
    crate::ssh::start(&config, &services);

    let background = services.clone();
    let app = rustit_web::router(AppState::new(Arc::clone(&config), services));
    let listener = TcpListener::bind(config.server.bind).await.with_context(|| format!("cannot bind {}", config.server.bind))?;
    tracing::info!(address = %config.server.bind, public_url = %config.server.public_url, data_dir = %config.storage.data_dir.display(), "Rustit is listening");

    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;

    tracing::info!("shutting down");
    // Stop background work before closing the stores it writes to.
    background.shutdown().await;
    storage.database.close().await;
    Ok(())
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

/// Resolves on Ctrl+C, or SIGTERM on Unix (as sent by systemd and container runtimes).
async fn shutdown_signal() {
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
    }
}
