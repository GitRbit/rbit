//! Real SQLite blocking plus explicit first polls reproduce both admission orders.

use super::*;
use crate::{CreateRepository, Services};
use rbit_core::{
    Config, Visibility,
    import::{ImportOptions, ImportStatus, SourceKind},
};
use rbit_storage::{Database, KvStore, NewUser, Work, count_work};
use sqlx::{Connection, Executor, SqliteConnection, sqlite::SqliteConnectOptions};
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    sync::Arc,
    task::Poll,
};

struct Fixture {
    services: Services,
    database: Database,
    account: rbit_core::AccountId,
    old: RepoId,
    new: RepoId,
    directory: tempfile::TempDir,
}
impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.updates.check_for_updates = false;
        config.storage.data_dir = directory.path().join("data");
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let actor = database
            .accounts()
            .create_user(NewUser {
                name: "alice".parse().unwrap(),
                email: "alice@example.test".parse().unwrap(),
                display_name: None,
                password_hash: "unused".to_owned(),
                is_admin: false,
            })
            .await
            .unwrap();
        let mut repositories = Vec::new();
        for name in ["old", "new"] {
            repositories.push(
                services
                    .repositories
                    .create(&actor, CreateRepository { name: name.to_owned(), visibility: Visibility::Public, ..Default::default() })
                    .await
                    .unwrap()
                    .repo
                    .id,
            );
        }
        Self { services, database, account: actor.id(), old: repositories[0], new: repositories[1], directory }
    }
    fn record(&self, repo: RepoId) -> NewImport<'static> {
        NewImport {
            repo,
            importer: self.account,
            kind: SourceKind::GitHub,
            source_url: "https://github.com/acme/rocket",
            options: ImportOptions::default(),
        }
    }
    async fn blocker(&self) -> SqliteConnection {
        // Use the configured filename rather than assuming the storage basename.
        let mut config = Config::default();
        config.storage.data_dir = self.directory.path().join("data");
        let mut connection =
            SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(config.storage.sqlite_path())).await.unwrap();
        connection.execute("BEGIN IMMEDIATE").await.unwrap();
        connection
    }
    async fn assert_final(&self) {
        let store = self.database.imports();
        assert_eq!(store.find(self.old).await.unwrap().unwrap().status, ImportStatus::Failed);
        assert_eq!(store.find(self.new).await.unwrap().unwrap().status, ImportStatus::Queued);
        self.services.shutdown().await;
    }
}

async fn pending<F: Future>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending(), "database/gate must suspend this first poll");
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn sweep_retains_admission_gate_until_its_sql_finishes() {
    let fixture = Fixture::new().await;
    fixture.database.imports().create(&fixture.record(fixture.old)).await.unwrap();
    let mut blocker = fixture.blocker().await;
    let service = &fixture.services.imports;
    let mut sweep = std::pin::pin!(service.fail_interrupted());
    pending(sweep.as_mut()).await;
    assert!(service.inner.running.try_lock().is_err(), "snapshot must not release the gate before its SQL await");
    let record = fixture.record(fixture.new);
    let mut creation = std::pin::pin!(service.create_record(&record));
    let ((), blocked_work) = count_work(pending(creation.as_mut())).await;
    assert_eq!(blocked_work, Work::default(), "waiting creation performs no premature SQL");
    blocker.execute("ROLLBACK").await.unwrap();
    assert_eq!(sweep.await.unwrap(), 1);
    let (created, work) = count_work(creation).await;
    created.unwrap();
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    fixture.assert_final().await;
}

#[tokio::test]
async fn record_admission_retains_the_gate_until_its_insert_then_sweep_excludes_it() {
    let fixture = Fixture::new().await;
    fixture.database.imports().create(&fixture.record(fixture.old)).await.unwrap();
    let mut blocker = fixture.blocker().await;
    let service = &fixture.services.imports;
    let record = fixture.record(fixture.new);
    let mut creation = std::pin::pin!(service.create_record(&record));
    pending(creation.as_mut()).await;
    assert!(service.inner.running.try_lock().is_err(), "admission must retain the gate across its insert");
    let mut sweep = std::pin::pin!(service.fail_interrupted());
    let ((), blocked_work) = count_work(pending(sweep.as_mut())).await;
    assert_eq!(blocked_work, Work::default());
    blocker.execute("ROLLBACK").await.unwrap();
    creation.await.unwrap();
    let (swept, work) = count_work(sweep).await;
    assert_eq!(swept.unwrap(), 1);
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    fixture.assert_final().await;
}

#[tokio::test]
async fn rejected_record_removes_its_reservation_and_releases_the_lifecycle_gate() {
    let fixture = Fixture::new().await;
    let service = &fixture.services.imports;
    let absent = RepoId(i64::MAX);
    assert!(service.create_record(&fixture.record(absent)).await.is_err());
    let running = service.inner.running.try_lock().expect("failed insertion released the gate");
    assert!(!running.contains(&absent), "failed SQL must not leave a running reservation");
    drop(running);
    assert_eq!(service.fail_interrupted().await.unwrap(), 0);
    fixture.services.shutdown().await;
}
