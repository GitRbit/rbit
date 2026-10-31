//! Restart the real executable, then verify configuration reload, sessions, audit data
//! and both listeners. The fixture only terminates processes started for this test.
use std::{
    net::{SocketAddr, TcpListener},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

use rbit_core::Config;
use rbit_services::{Services, SignUp};
use rbit_storage::{Database, KvStore};

struct Server {
    child: Child,
    log: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        // On Windows the replacement has its own PID; on Unix exec preserves it.
        #[cfg(windows)]
        if let Some(pid) = std::fs::read_to_string(&self.log).ok().and_then(|log| {
            log.rsplit("process_id=").next().and_then(|tail| tail.split_whitespace().next()).and_then(|pid| pid.parse::<u32>().ok())
        }) {
            use std::os::windows::process::CommandExt as _;
            let _ = Command::new("taskkill.exe")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .creation_flags(0x0800_0000)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn address() -> SocketAddr {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap()
}

async fn seed(data: &Path) -> String {
    let mut config = Config::default();
    config.storage.data_dir = data.to_owned();
    std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
    let admin = services
        .accounts
        .create_user(
            SignUp { username: "root".into(), email: "root@example.com".into(), password: "correct horse battery".into() },
            Some(true),
        )
        .await
        .unwrap();
    let session = services.accounts.start_session(&admin).await.unwrap();
    database.close().await;
    format!("rbit_session={}", session.encode())
}

fn configuration(data: &Path, http: SocketAddr, ssh: SocketAddr, metrics: bool) -> String {
    format!(
        r#"[server]
bind = "{http}"
public_url = "http://{http}"
[storage]
data_dir = {}
[security]
rate_limit = false
[updates]
check_for_updates = false
[ssh]
enabled = true
bind = "{ssh}"
[operations]
metrics = {metrics}
"#,
        serde_json::to_string(&data.to_string_lossy()).unwrap()
    )
}

async fn wait_for_ssh(address: SocketAddr) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if tokio::net::TcpStream::connect(address).await.is_ok() {
            return;
        }
        assert!(Instant::now() < deadline, "SSH listener did not return");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn application_restart_reopens_storage_and_listeners_and_reloads_configuration() {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    let cookie = seed(&data).await;
    let http = address();
    let ssh = address();
    let url = format!("http://{http}");
    let config = directory.path().join("server.toml");
    std::fs::write(&config, configuration(&data, http, ssh, false)).unwrap();
    let log = directory.path().join("server.log");
    let output = std::fs::File::create(&log).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_rbit"));
    command
        .args(["--config", config.to_str().unwrap(), "serve"])
        .env("NO_COLOR", "1")
        .env("RUST_LOG", "info")
        .env_remove("RBIT_CONFIG")
        .stdin(Stdio::null())
        .stdout(output.try_clone().unwrap())
        .stderr(output);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x0800_0000);
    }
    let server = Server { child: command.spawn().unwrap(), log };
    let client = reqwest::Client::builder().timeout(Duration::from_secs(2)).redirect(reqwest::redirect::Policy::none()).build().unwrap();
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if client.get(format!("{url}/healthz")).send().await.is_ok_and(|response| response.status().is_success()) {
            break;
        }
        assert!(Instant::now() < deadline, "server did not start: {}", std::fs::read_to_string(&server.log).unwrap_or_default());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    wait_for_ssh(ssh).await;
    assert_eq!(client.get(format!("{url}/metrics")).send().await.unwrap().status(), 404);
    let page = client.get(format!("{url}/admin")).header("Cookie", &cookie).send().await.unwrap();
    assert_eq!(page.status(), 200);
    assert!(page.text().await.unwrap().contains(r#"href="/admin/restart""#));
    for metrics in [true, false] {
        std::fs::write(&config, configuration(&data, http, ssh, metrics)).unwrap();
        let response = client
            .post(format!("{url}/admin/restart"))
            .header("Cookie", &cookie)
            .header("Origin", &url)
            .form(&[("confirmation", "restart")])
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 202);
        assert!(response.text().await.unwrap().contains("Restart requested"));
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            if let Ok(response) = client.get(format!("{url}/admin/restart/status")).header("Cookie", &cookie).send().await
                && let Ok(status) = response.json::<serde_json::Value>().await
                && status["restarting"] == false
            {
                #[cfg(windows)]
                assert_ne!(status["pid"].as_u64().unwrap(), u64::from(server.child.id()), "Windows must launch a new application process");
                break;
            }
            assert!(Instant::now() < deadline, "restart did not complete: {}", std::fs::read_to_string(&server.log).unwrap_or_default());
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        wait_for_ssh(ssh).await;
        assert_eq!(
            client.get(format!("{url}/metrics")).send().await.unwrap().status(),
            if metrics { 200 } else { 404 },
            "the new process loaded the changed config file"
        );
    }
    let audit = client.get(format!("{url}/admin/audit")).header("Cookie", &cookie).send().await.unwrap();
    assert_eq!(audit.status(), 200, "the persisted session survives restart");
    assert!(audit.text().await.unwrap().contains("Requested a server restart"));
    assert_eq!(std::fs::read_to_string(&server.log).unwrap().matches("rbit is listening").count(), 3);
}
