//! Contained functional browser adapter in the already-built web integration binary.
#[cfg(target_os = "linux")]
use super::*;

const MODE: &str = "isolated-linux-v1";
const COMMAND_LIMIT: u64 = 32;

#[derive(Debug, PartialEq, Eq)]
enum Control {
    RevokeWriter,
    Stop,
}
impl Control {
    fn parse(bytes: &[u8]) -> Option<Self> {
        match bytes {
            b"revoke-writer\n" => Some(Self::RevokeWriter),
            b"stop\n" => Some(Self::Stop),
            _ => None,
        }
    }
}

#[test]
fn control_protocol_has_no_paths_urls_or_arbitrary_commands() {
    assert_eq!(Control::parse(b"revoke-writer\n"), Some(Control::RevokeWriter));
    assert_eq!(Control::parse(b"stop\n"), Some(Control::Stop));
    for value in [b"".as_slice(), b"STOP\n", b"stop\nextra", b"http://example.test", b"/private/path"] {
        assert_eq!(Control::parse(value), None);
    }
    assert!(b"revoke-writer\n".len() < usize::try_from(COMMAND_LIMIT).unwrap());
}

#[tokio::test]
#[ignore = "fixed disposable Linux browser container only; no ordinary test starts a listener"]
async fn serve() {
    assert_eq!(std::env::var("RBIT_ACTIONS_VISUAL_FIXTURE").as_deref(), Ok(MODE), "FIXTURE_MODE_REFUSED");
    #[cfg(target_os = "linux")]
    linux::serve().await;
    #[cfg(not(target_os = "linux"))]
    panic!("FIXTURE_PLATFORM_REFUSED");
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use axum::Extension;
    use rbit_core::membership::Permission;
    use rbit_localization::{Locale, LocaleResolver};
    use std::{io::Write, net::SocketAddr, os::unix::fs::DirBuilderExt, time::Duration};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, UnixListener, UnixStream},
    };

    const DIRECTORY: &str = "/tmp/actions-production";
    const SOCKET: &str = "/tmp/actions-production/control.sock";

    fn require_containment() {
        assert_eq!(env!("CARGO_MANIFEST_DIR"), "/home/runner/work/rbit/rbit/crates/rbit-web", "FIXTURE_ASSET_ROOT_REFUSED");
        let status = std::fs::read_to_string("/proc/self/status").expect("FIXTURE_STATUS_REFUSED");
        for line in ["NoNewPrivs:\t1", "Seccomp:\t2", "CapEff:\t0000000000000000", "CapBnd:\t0000000000000000"] {
            assert!(status.lines().any(|actual| actual == line), "FIXTURE_CONTAINMENT_REFUSED");
        }
        let uid = status
            .lines()
            .find_map(|line| line.strip_prefix("Uid:"))
            .and_then(|value| value.split_whitespace().next())
            .and_then(|value| value.parse::<u32>().ok())
            .expect("FIXTURE_UID_REFUSED");
        assert_ne!(uid, 0, "FIXTURE_ROOT_REFUSED");
    }

    async fn seed(fixture: &Fixture) {
        fixture.database.actions_settings().set_enabled(fixture.repo_id().await, true, Timestamp::now()).await.unwrap();
        let labels = vec!["ubuntu-latest".to_owned()];
        let jobs = [NewJob {
            key: "build",
            name: "Build / stable",
            matrix: None,
            needs: &[],
            runs_on: &labels,
            permissions: None,
            payload: b"{}",
            condition: false,
            max_parallel: None,
            group: None,
            environment: None,
        }];
        let run = fixture
            .database
            .actions_runs()
            .create_run(&NewRun {
                repo: fixture.repo_id().await,
                workflow_path: ".rbit/workflows/ci.yml",
                workflow_name: "Continuous integration",
                title: "Review the synthetic fork contribution",
                event: RunEvent::PullRequest,
                reference: "refs/heads/main",
                sha: "0123456789abcdef0123456789abcdef01234567",
                actor: Some(fixture.alice.id()),
                trigger_actor: Some(fixture.alice.id()),
                pull_number: None,
                is_fork: true,
                trusted: false,
                concurrency_group: None,
                cancel_in_progress: false,
                event_payload: "{}",
                dispatch_inputs: None,
                start: RunStart::AwaitingApproval,
                jobs: &jobs,
                error: None,
                now: Timestamp::now(),
            })
            .await
            .unwrap();
        assert_eq!(run.number, 1, "FIXTURE_RUN_ID_REFUSED");
    }

    async fn command(stream: &mut UnixStream) -> Control {
        let mut bytes = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), stream.take(COMMAND_LIMIT + 1).read_to_end(&mut bytes))
            .await
            .expect("FIXTURE_CONTROL_TIMEOUT")
            .expect("FIXTURE_CONTROL_READ_REFUSED");
        Control::parse(&bytes).expect("FIXTURE_CONTROL_REFUSED")
    }

    pub(super) async fn serve() {
        require_containment();
        let listener = TcpListener::bind("127.0.0.1:4189").await.expect("FIXTURE_BIND_REFUSED");
        std::fs::DirBuilder::new().mode(0o700).create(DIRECTORY).expect("FIXTURE_DIRECTORY_REFUSED");
        let controls = UnixListener::bind(SOCKET).expect("FIXTURE_CONTROL_BIND_REFUSED");
        let fixture = Fixture::new_with_public_url("http://127.0.0.1:4189").await;
        seed(&fixture).await;
        let repo = fixture.services.repositories.open(Some(&fixture.alice), "alice", "rocket").await.unwrap();
        fixture.services.collaborators.add(&fixture.alice, &repo, "bob", Permission::Write).await.unwrap();
        let router = fixture.router.clone().layer(Extension(LocaleResolver::new(Locale::English, true).unwrap()));
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>())
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
        });
        // Private process pipe only: the Node owner never logs or artifacts this token.
        let cookie = fixture.reader.strip_prefix("rbit_session=").expect("FIXTURE_COOKIE_REFUSED");
        println!("RBIT_ACTIONS_FIXTURE_READY {}", serde_json::json!({"version": 1, "cookie": cookie}));
        std::io::stdout().flush().expect("FIXTURE_READY_REFUSED");
        let deadline = tokio::time::sleep(Duration::from_secs(120));
        tokio::pin!(deadline);
        let mut stop_stream = loop {
            let (mut stream, _) = tokio::select! {
                accepted = controls.accept() => accepted.expect("FIXTURE_CONTROL_ACCEPT_REFUSED"),
                () = &mut deadline => panic!("FIXTURE_DEADLINE"),
            };
            match command(&mut stream).await {
                Control::RevokeWriter => {
                    fixture.services.collaborators.remove(&fixture.alice, &repo, "bob").await.unwrap();
                    stream.write_all(b"REVOKED\n").await.expect("FIXTURE_ACK_REFUSED");
                }
                Control::Stop => break stream,
            }
        };
        stop.send(()).expect("FIXTURE_STOP_REFUSED");
        tokio::time::timeout(Duration::from_secs(10), server)
            .await
            .expect("FIXTURE_DRAIN_TIMEOUT")
            .expect("FIXTURE_JOIN_REFUSED")
            .expect("FIXTURE_SERVE_REFUSED");
        drop(repo);
        let Fixture { _directory: directory, database, services, router, alice, owner, reader } = fixture;
        drop(router);
        services.shutdown().await;
        drop((services, alice, owner, reader));
        database.close().await;
        drop(database);
        directory.close().expect("FIXTURE_DATA_RELEASE_REFUSED");
        drop(controls);
        std::fs::remove_file(SOCKET).expect("FIXTURE_SOCKET_RELEASE_REFUSED");
        std::fs::remove_dir(DIRECTORY).expect("FIXTURE_DIRECTORY_RELEASE_REFUSED");
        stop_stream.write_all(b"STOPPED\n").await.expect("FIXTURE_STOP_ACK_REFUSED");
    }
}
