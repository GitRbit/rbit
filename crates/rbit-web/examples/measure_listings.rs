//! Local-only listing probe using a read-only source repository and temporary app data.
//! Run: `cargo run -p rbit-web --example measure_listings -- SOURCE.git OUTPUT_DIRECTORY`
//! The port is ephemeral, the fixture is anonymous/public, and the server exits after
//! two minutes. Debug timings are diagnostic only; never compare them with release.
use axum::{body::Body, http::Request};
use rbit_core::{Config, Visibility};
use rbit_services::{CreateRepository, Services, SignUp};
use rbit_storage::{Database, KvStore};
use std::{
    path::PathBuf,
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};
use tower::ServiceExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    // Keep a normal absolute Windows path: canonicalize adds a verbatim prefix
    // which Git may parse as an SSH host rather than a local source.
    let source = std::path::absolute(PathBuf::from(args.next().ok_or("source repository required")?))?;
    let output = PathBuf::from(args.next().ok_or("output directory required")?);
    std::fs::create_dir_all(&output)?;
    let temp = tempfile::tempdir()?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    let mut config = Config::default();
    config.repository.default_branch = "master".into();
    config.storage.data_dir = temp.path().join("data");
    config.server.public_url = base.clone();
    config.security.rate_limit = false;
    std::fs::create_dir_all(config.storage.repositories_dir())?;
    let db = Database::open(&config.storage.sqlite_path()).await?;
    let kv = KvStore::open(&config.storage.kv_path())?;
    let config = Arc::new(config);
    let services = Services::new(Arc::clone(&config), &db, &kv).await?;
    let owner = services
        .accounts
        .create_user(
            SignUp { username: "realworld".into(), email: "probe@example.com".into(), password: "local fixture password".into() },
            Some(false),
        )
        .await?;
    let repo = services
        .repositories
        .create(&owner, CreateRepository { name: "git".into(), description: String::new(), visibility: Visibility::Public })
        .await?;
    let path = rbit_git::RepositoryStorage::new(config.storage.repositories_dir()).path_for(repo.owner.id, repo.repo.id);
    let status = Command::new("git")
        .current_dir(&path)
        .args(["fetch", "--quiet"])
        .arg(&source)
        .args(["+refs/heads/*:refs/heads/*", "+refs/tags/*:refs/tags/*"])
        .status()?;
    if !status.success() {
        return Err("fixture fetch failed".into());
    }
    let reader = rbit_git::RepositoryReader::new(path);
    let head = reader.commit_id("master".into()).await?;
    let router = rbit_web::router(rbit_web::AppState::new(config, services));
    let server = router.clone();
    let handle =
        tokio::spawn(async move { axum::serve(listener, server.into_make_service_with_connect_info::<std::net::SocketAddr>()).await });
    let probes = [
        ("tree", format!("/realworld/git/tree/{head}")),
        ("metadata", format!("/realworld/git/tree-commits/{head}")),
        ("tags", "/realworld/git/tags".into()),
        ("directory", format!("/realworld/git/tree/{head}/t")),
        ("graph", "/realworld/git/branches?view=tree".into()),
        ("languages", format!("/realworld/git/language-files?revision={head}&language=C")),
    ];
    let mut samples = Vec::new();
    for round in 0..2 {
        for (name, url) in &probes {
            let started = Instant::now();
            let response = router.clone().oneshot(Request::builder().uri(url).body(Body::empty())?).await?;
            let status = response.status().as_u16();
            let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            if status != 200 {
                return Err(format!("{name}: HTTP {status}").into());
            }
            if round == 0 {
                std::fs::write(output.join(format!("{name}.html")), &body)?;
            }
            samples.push(
                serde_json::json!({"route":name,"path":url,"round":round+1,"status":status,"htmlBytes":body.len(),"inProcessMs":elapsed}),
            );
        }
    }
    let report = serde_json::json!({"source":source,"head":head,"profile":if cfg!(debug_assertions){"debug"}else{"release"},"method":"Isolated temporary data, in-process router, two sequential rounds. No network latency, no deployment, no production speed claim.","base":base,"samples":samples});
    std::fs::write(output.join("candidate-listings.json"), serde_json::to_vec_pretty(&report)?)?;
    println!("BENCHMARK_URL={base}");
    println!("HEAD={head}");
    println!("REPORT={}", output.join("candidate-listings.json").display());
    tokio::time::sleep(Duration::from_secs(120)).await;
    handle.abort();
    Ok(())
}
