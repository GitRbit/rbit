//! Mandatory permission regression group, SSH ingress: every account of the shared world
//! (`rbit-services/tests/support/permission_matrix.rs`) fetches and pushes over a real
//! in-process SSH connection, and each decision equals the oracle, archived repositories
//! included. SSH has no anonymous access; a suspended account is refused on connections
//! opened before the suspension and cannot authenticate afterwards.

use std::{sync::Arc, time::Duration};

use rbit_core::Config;
use rbit_services::{Services, ssh_keys::AddSshKey};
use rbit_storage::{Database, KvStore};
use russh::{
    ChannelMsg, client,
    keys::{PrivateKey, PrivateKeyWithHashAlg, PublicKeyOrCertificate, ssh_key::private::Ed25519Keypair},
};

use crate::permission_world::{ACTORS, Actor, SHAPES, Shape, World, git, oracle};

/// The branch prefix of the refs SSH pushes may delete.
const PROBE: &str = "ssh";

struct Client;

#[allow(unknown_lints, clippy::unused_async_trait_impl)]
impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(&mut self, _: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

fn key(actor: Actor) -> PrivateKey {
    let seed = u8::try_from(ACTORS.iter().position(|candidate| *candidate == actor).unwrap()).unwrap() + 100;
    PrivateKey::from(Ed25519Keypair::from_seed(&[seed; 32]))
}

/// A client signed in with `actor`'s key, or `None` when the key is refused.
async fn connect(port: u16, actor: Actor) -> Option<client::Handle<Client>> {
    let config = client::Config { inactivity_timeout: Some(Duration::from_secs(30)), ..Default::default() };
    let mut client = client::connect(Arc::new(config), ("127.0.0.1", port), Client).await.unwrap();
    let key = PrivateKeyWithHashAlg::new(Arc::new(key(actor)), None);
    client.authenticate_publickey("git", key).await.unwrap().success().then_some(client)
}

/// Runs `command`, answering the ref advertisement (the first flush) with `reply`; the exit
/// status and everything the server wrote.
async fn exchange(client: &client::Handle<Client>, command: &str, reply: &[u8]) -> (u32, String) {
    let work = async {
        let mut channel = client.channel_open_session().await.unwrap();
        channel.exec(true, command).await.unwrap();
        let (mut output, mut status, mut replied) = (Vec::new(), None, false);
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } => {
                    output.extend_from_slice(&data);
                    if !replied && output.ends_with(b"0000") {
                        replied = true;
                        channel.data(reply).await.unwrap();
                        channel.eof().await.unwrap();
                    }
                }
                ChannelMsg::ExtendedData { data, .. } => output.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                _ => {}
            }
        }
        (status.expect("command exited"), String::from_utf8_lossy(&output).into_owned())
    };
    tokio::time::timeout(Duration::from_secs(20), work).await.unwrap()
}

struct Harness {
    _dir: tempfile::TempDir,
    database: Database,
    world: World,
    server: rbit_ssh::RunningSshServer,
}

async fn start() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = dir.path().join("data");
    config.security.rate_limit = false;
    config.ssh.enabled = true;
    config.ssh.bind = "127.0.0.1:0".parse().unwrap();
    std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
    let config = Arc::new(config);
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
    let world = World::build(&services, &database, dir.path(), &[PROBE]).await;
    for (actor, user) in &world.users {
        let input = AddSshKey { title: format!("{actor:?}"), key: key(*actor).public_key().to_openssh().unwrap() };
        services.ssh_keys.add_user_key(user, input).await.unwrap();
    }
    let server = rbit_ssh::start(config, services).await.unwrap();
    Harness { _dir: dir, database, world, server }
}

fn case(actor: Actor, shape: Shape) -> String {
    format!("{actor:?} on {}", shape.full_name())
}

/// A fetch that only reads the advertisement, and a push that deletes `actor`'s probe branch.
async fn probe(harness: &Harness, client: &client::Handle<Client>, actor: Actor, shape: Shape) -> ((u32, String), (u32, String)) {
    let repo = harness.world.repo(shape);
    let fetch = exchange(client, &format!("git-upload-pack '{}'", shape.full_name()), b"0000").await;
    let branch = format!("refs/heads/{PROBE}-{}", actor.login().unwrap());
    let mut commands = rbit_transport::pkt_line::encode(&format!("{} {} {branch}\0report-status\n", repo.head, "0".repeat(40)));
    commands.extend_from_slice(b"0000");
    let push = exchange(client, &format!("git-receive-pack '{}'", shape.full_name()), &commands).await;
    (fetch, push)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ssh_fetch_and_push_follow_the_oracle_and_the_archive_guard() {
    let harness = Box::pin(start()).await;
    // Connected before the suspension, which every later command must still observe.
    let suspended = connect(harness.server.local_addr().port(), Actor::Suspended).await.expect("not yet suspended");
    harness.world.suspend(&harness.database).await;
    assert!(connect(harness.server.local_addr().port(), Actor::Suspended).await.is_none(), "suspended keys cannot sign in");

    for actor in ACTORS.into_iter().filter(|actor| actor.authenticates()) {
        let client = connect(harness.server.local_addr().port(), actor).await.expect("key accepted");
        for shape in SHAPES {
            let access = oracle(actor, shape);
            let ((fetched, fetch_output), (pushed, report)) = probe(&harness, &client, actor, shape).await;
            let branch = format!("refs/heads/{PROBE}-{}", actor.login().unwrap());
            let exists = git(&harness.world.repo(shape).disk_path, &["for-each-ref", &branch]).contains(&branch);
            assert_eq!(fetched == 0, access.can_read(), "fetch {}: {fetch_output}", case(actor, shape));
            if !access.can_read() {
                assert!(fetch_output.contains("not found") && report.contains("not found"), "{}: {report}", case(actor, shape));
                assert!(exists, "untouched: {}", case(actor, shape));
            } else if !access.can_write() {
                // Readers reach the push gate, which declines every update like a pre-receive hook.
                assert!(report.contains(&format!("ng {branch} ")), "{}: {pushed} {report}", case(actor, shape));
                let reason = if shape.archived { "archived" } else { "not allowed to push" };
                assert!(report.contains(reason), "{}: {report}", case(actor, shape));
                assert!(exists, "untouched: {}", case(actor, shape));
            } else if shape.archived {
                assert!(report.contains(&format!("ng {branch} ")) && report.contains("archived"), "{}: {report}", case(actor, shape));
                assert!(exists, "archived refs stay: {}", case(actor, shape));
            } else {
                assert_eq!(pushed, 0, "{}: {report}", case(actor, shape));
                assert!(report.contains(&format!("ok {branch}")), "{}: {report}", case(actor, shape));
                assert!(!exists, "deleted: {}", case(actor, shape));
            }
        }
        client.disconnect(russh::Disconnect::ByApplication, "done", "").await.unwrap();
    }

    for shape in SHAPES {
        let ((fetched, output), (pushed, report)) = probe(&harness, &suspended, Actor::Suspended, shape).await;
        // The key is reloaded before every command, and a suspended holder counts as no one.
        let refused = |text: &str| text.contains("no longer authorized") || text.contains("suspended");
        assert!(fetched != 0 && refused(&output), "{}: {output}", case(Actor::Suspended, shape));
        assert!(pushed != 0 && refused(&report), "{}: {report}", case(Actor::Suspended, shape));
        assert!(!report.contains(&format!("ok refs/heads/{PROBE}-sam")), "{}: {report}", case(Actor::Suspended, shape));
    }
    suspended.disconnect(russh::Disconnect::ByApplication, "done", "").await.unwrap();
    harness.server.shutdown().await;
}
