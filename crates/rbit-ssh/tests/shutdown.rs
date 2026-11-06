//! Shutdown owns russh handlers and Git children, not just listener waiters.

use rbit_core::Config;
use rbit_services::Services;
use rbit_storage::{Database, KvStore};
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncBufReadExt, BufReader};

struct Host {
    directory: tempfile::TempDir,
    config: Config,
    database: Database,
    kv: KvStore,
    services: Services,
}

async fn stalled_peer(address: std::net::SocketAddr) -> BufReader<tokio::net::TcpStream> {
    let peer = tokio::net::TcpStream::connect(address).await.unwrap();
    let mut peer = BufReader::new(peer);
    let mut banner = String::new();
    tokio::time::timeout(Duration::from_secs(3), peer.read_line(&mut banner)).await.unwrap().unwrap();
    assert!(banner.starts_with("SSH-2.0-"), "server must have accepted and started its handshake");
    // Never send the client's identification: the server is now blocked in handshake.
    peer
}

impl Host {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        config.ssh.enabled = true;
        config.ssh.bind = "127.0.0.1:0".parse().unwrap();
        config.updates.check_for_updates = false;
        config.repository.max_git_processes = 1;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(Arc::new(config.clone()), &database, &kv).await.unwrap();
        Self { directory: dir, config, database, kv, services }
    }
}

#[tokio::test]
async fn shutdown_destroys_a_stalled_pre_authentication_handler() {
    let host = Host::new().await;
    let server = rbit_ssh::start(Arc::new(host.config.clone()), host.services.clone()).await.unwrap();
    let _peer = stalled_peer(server.local_addr()).await;
    tokio::time::timeout(Duration::from_secs(3), server.shutdown()).await.unwrap();
    let path = host.config.storage.kv_path();
    drop(host.services);
    drop(host.kv);
    KvStore::open(&path).unwrap();
    host.database.close().await;
    assert!(host.directory.path().is_dir());
}

#[tokio::test]
async fn dropping_server_signals_cleanup_without_retaining_store_owners_forever() {
    let host = Host::new().await;
    let server = rbit_ssh::start(Arc::new(host.config.clone()), host.services.clone()).await.unwrap();
    let _peer = stalled_peer(server.local_addr()).await;
    drop(server);
    let path = host.config.storage.kv_path();
    drop(host.services);
    drop(host.kv);
    // Drop only signals. Observe actual release instead of claiming it drained.
    let reopened = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(store) = KvStore::open(&path) {
                break store;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    drop(reopened);
    host.database.close().await;
}

#[cfg(unix)]
mod unix {
    use super::*;
    use rbit_core::Visibility;
    use rbit_services::{CreateRepository, ssh_keys::AddSshKey};
    use rbit_storage::NewUser;
    use russh::{
        client,
        keys::{PrivateKey, PrivateKeyWithHashAlg, PublicKeyOrCertificate, ssh_key::private::Ed25519Keypair},
    };
    use std::{
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
    };

    struct Peer;
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    impl client::Handler for Peer {
        type Error = russh::Error;
        async fn check_server_key(&mut self, _: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn shutdown_reaps_live_git_before_releasing_its_permit_and_stores() {
        let host = Host::new().await;
        let user = host
            .database
            .accounts()
            .create_user(NewUser {
                name: "alice".parse().unwrap(),
                email: "alice@example.com".parse().unwrap(),
                display_name: None,
                password_hash: "unused".to_owned(),
                is_admin: false,
            })
            .await
            .unwrap();
        host.services
            .repositories
            .create(&user, CreateRepository { name: "project".to_owned(), description: String::new(), visibility: Visibility::Private })
            .await
            .unwrap();
        let key = PrivateKey::from(Ed25519Keypair::from_seed(&[93; 32]));
        host.services
            .ssh_keys
            .add_user_key(&user, AddSshKey { title: "test".to_owned(), key: key.public_key().to_openssh().unwrap() })
            .await
            .unwrap();
        let pid_file = host.directory.path().join("owned-git.pid");
        let script = host.directory.path().join("stalled-git");
        let quoted = pid_file.to_string_lossy().replace('\'', "'\\''");
        std::fs::write(&script, format!("#!/bin/sh\nprintf '%s' \"$$\" > '{quoted}'\nexec sleep 60\n")).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut ssh_config = host.config.clone();
        ssh_config.repository.git_binary = script;
        let server = rbit_ssh::start(Arc::new(ssh_config), host.services.clone()).await.unwrap();
        let mut peer = client::connect(Arc::new(client::Config::default()), server.local_addr(), Peer).await.unwrap();
        assert!(peer.authenticate_publickey("git", PrivateKeyWithHashAlg::new(Arc::new(key), None)).await.unwrap().success());
        let channel = peer.channel_open_session().await.unwrap();
        channel.exec(true, "git-upload-pack 'alice/project.git'").await.unwrap();
        let pid = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(pid) = tokio::fs::read_to_string(&pid_file).await
                    && !pid.is_empty()
                {
                    break pid;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(host.services.git_processes.try_acquire(None).is_err(), "live Git owns its admission slot");
        tokio::time::timeout(Duration::from_secs(5), server.shutdown()).await.unwrap();
        assert!(
            !Command::new("kill").args(["-0", &pid]).stderr(Stdio::null()).status().unwrap().success(),
            "shutdown returned before its owned Git process exited"
        );
        let _slot = host.services.git_processes.try_acquire(None).unwrap();
        let path = host.config.storage.kv_path();
        drop(host.services);
        drop(host.kv);
        KvStore::open(&path).unwrap();
    }
}
