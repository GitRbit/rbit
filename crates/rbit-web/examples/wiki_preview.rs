//! Local-only wiki preview for visual checks, with temporary app data.
//! Run: `cargo run -p rbit-web --example wiki_preview -- [PORT]`
//! Serves `127.0.0.1:PORT` (default 3990) for ten minutes with a public repository
//! `editor/handbook` whose wiki has a few pages and revisions, and the diagnostic
//! locales enabled (`Accept-Language: ar-XB` or `en-XA`). It prints a session cookie for
//! the `editor` account so write views can be inspected without a password.
use rbit_core::{Config, Visibility};
use rbit_localization::{Locale, LocaleResolver};
use rbit_services::{CreateRepository, Services, wikis::publish::WikiPageInput};
use rbit_storage::{Database, KvStore, NewUser};
use std::{net::SocketAddr, sync::Arc, time::Duration};

const HOME: &str = "# Handbook\n\nWelcome to the **handbook**. Start with [Getting started](Getting-started).\n\n\
## Contents\n\n| Area | Page |\n| --- | --- |\n| Setup | [Getting started](Getting-started) |\n| Operations | [Deploy](Guides/Deploy) |\n\n\
```sh\ncargo run --release\n```\n\n> Pages keep their history; anything can be restored.\n";

fn page(title: &str, path: &str, content: &str) -> WikiPageInput {
    WikiPageInput { title: title.into(), path: path.into(), content: content.into() }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port: u16 = std::env::args().nth(1).map_or(Ok(3990), |port| port.parse())?;
    let temp = tempfile::tempdir()?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let mut config = Config::default();
    config.storage.data_dir = temp.path().join("data");
    config.server.public_url = format!("http://127.0.0.1:{port}");
    config.security.rate_limit = false;
    std::fs::create_dir_all(config.storage.repositories_dir())?;
    let database = Database::open(&config.storage.sqlite_path()).await?;
    let kv = KvStore::open(&config.storage.kv_path())?;
    let config = Arc::new(config);
    let services = Services::new(Arc::clone(&config), &database, &kv).await?;
    let editor = database
        .accounts()
        .create_user(NewUser {
            name: "editor".parse()?,
            email: "editor@example.com".parse()?,
            display_name: None,
            password_hash: "preview fixture without a password".into(),
            is_admin: false,
        })
        .await?;
    let description = "Team documentation".to_owned();
    services
        .repositories
        .create(&editor, CreateRepository { name: "handbook".into(), description, visibility: Visibility::Public })
        .await?;
    let repo = services.repositories.open(Some(&editor), "editor", "handbook").await?;
    let wikis = &services.wikis;
    wikis.set_enabled(&repo, true).await?;
    wikis.create(&repo, &editor, &page("Home", "", HOME)).await?;
    wikis.create(&repo, &editor, &page("Getting started", "", "# Getting started\n\n1. Install Rust.\n2. Clone the repository.\n")).await?;
    wikis.create(&repo, &editor, &page("Deploy", "Guides/Deploy", "Build, copy the binary, restart the service.\n")).await?;
    wikis.create(&repo, &editor, &page("Backups", "Guides/Backups", "Run `rbit backup` nightly.\n")).await?;
    let started = wikis.pages(&repo, None, None).await?.entries.into_iter().find(|entry| entry.path.as_str() == "Getting-started");
    if let Some(started) = started {
        let current = wikis.page_by_id(&repo, None, started.id).await?;
        let text = "# Getting started\n\n1. Install Rust 1.94 or newer.\n2. Clone the repository.\n3. Run `npm run build`.\n";
        wikis.edit(&repo, &editor, started.id, current.page.head.generation, &page("Getting started", "Getting-started", text)).await?;
    }
    let token = services.accounts.start_session(&editor).await?.encode();
    println!("http://127.0.0.1:{port}/editor/handbook/wiki");
    println!("cookie: {}={token}", rbit_web::middleware::SESSION_COOKIE);
    let router = rbit_web::router(rbit_web::AppState::new(config, services.clone()))
        .layer(axum::Extension(LocaleResolver::new(Locale::English, true).map_err(|_| "resolver")?));
    let serve = axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>());
    tokio::select! {
        result = serve => result?,
        () = tokio::time::sleep(Duration::from_secs(600)) => {}
    }
    Ok(())
}
