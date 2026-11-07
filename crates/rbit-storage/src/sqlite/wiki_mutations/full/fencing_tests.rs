//! Real SQLite replay/locks, with a physical file-owner port; native Git is tested
//! separately by services' actual `CatalogBoundWikiObjects` composition fixtures.
use super::*;
use fencing::FencedWikiRecovery;
use rbit_wikis::{commands::WikiCommandId, ownership::WikiObjectOwnership};
use std::{fs::File, time::Duration};

pub(super) struct FileObjects {
    file: Arc<File>,
    catalog: WikiCatalogHandle,
}
impl WikiCatalogOwnership for FileObjects {
    type Error = WikiCatalogLeaseError;
    fn check_bound(&self) -> Result<(), Self::Error> {
        self.catalog.check_bound()
    }
    fn store_id(&self) -> WikiStoreId {
        self.catalog.store_id()
    }
    fn catalog_path(&self) -> &Path {
        self.catalog.catalog_path()
    }
}
impl WikiObjectOwnership for FileObjects {
    fn check_objects_bound(&self) -> Result<(), Self::Error> {
        self.check_bound()?;
        if !self.file.metadata()?.is_file() {
            return Err(WikiCatalogLeaseError::Changed);
        }
        Ok(())
    }
}
pub(super) fn objects(writer: &FullWikiWriter, directory: &Path) -> FileObjects {
    let file = File::create_new(directory.join("physical-object-scope")).unwrap();
    file.try_lock().unwrap();
    FileObjects { file: Arc::new(file), catalog: writer.catalog() }
}

fn retry(command: &WikiWriteCommand) -> WikiWriteCommand {
    WikiWriteCommand {
        id: command.id,
        actor: command.actor,
        expected: command.expected,
        path: command.path.as_str().parse().unwrap(),
        title: command.title.as_str().parse().unwrap(),
        content: command.content.as_str().to_owned().try_into().unwrap(),
    }
}

pub(super) async fn fenced(writer: FullWikiWriter, directory: &Path) -> FencedWikiRecovery<FileObjects> {
    let objects = objects(&writer, directory);
    writer.begin_recovery(objects).await.unwrap()
}

#[tokio::test]
async fn actual_normal_pending_replay_is_checkpointed_without_resubmission_or_restamping() {
    let (directory, database, writer, command) = tests::fixture().await;
    let intent = database.wiki_mutations().accept(command, Timestamp(7), tests::capacity()).await.unwrap();
    let expected = (intent.page, intent.generation, intent.accepted_at);
    let recovery = fenced(writer, directory.path()).await;
    let accepted = recovery.replay(intent.command, Timestamp(99), tests::capacity()).await.unwrap();
    assert_eq!((accepted.intent().page, accepted.intent().generation, accepted.intent().accepted_at), expected);
    assert_eq!(format!("{accepted:?}"), "FencedWikiAcceptance(..)");
    let visible: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wiki_pages").fetch_one(&database.pool).await.unwrap();
    assert_eq!(visible, 0);
    let (intent, _epoch, _objects) = accepted.into_parts();
    let mut missing = intent.command;
    missing.id = WikiCommandId::new([2; 16]).unwrap();
    assert!(matches!(
        recovery.replay(missing, Timestamp(9), tests::capacity()).await,
        Err(WikiWriteError::Acceptance(WikiMutationError::MissingIntent))
    ));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wiki_mutation_intents").fetch_one(&database.pool).await.unwrap();
    assert_eq!(count, 1);
    recovery.close().await.unwrap();
}

#[tokio::test]
async fn a_real_older_reader_blocks_replay_receipt_until_checkpoint_can_finish() {
    let (directory, database, writer, command) = tests::fixture().await;
    let mut reader = database.pool.acquire().await.unwrap();
    sqlx::query("BEGIN").execute(&mut *reader).await.unwrap();
    let _: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts").fetch_one(&mut *reader).await.unwrap();
    let intent = database.wiki_mutations().accept(command, Timestamp(7), tests::capacity()).await.unwrap();
    let page = intent.page;
    let recovery = fenced(writer, directory.path()).await;
    {
        // Test-only deterministic busy handler on the actual dedicated physical
        // connection; no production timeout API/default or scheduling sleep.
        let mut connection = recovery.writer.0.pool.acquire().await.unwrap();
        sqlx::query("PRAGMA busy_timeout=0").execute(&mut *connection).await.unwrap();
    }
    let again = retry(&intent.command);
    assert!(matches!(recovery.replay(intent.command, Timestamp(8), tests::capacity()).await, Err(WikiWriteError::ReplaySyncIncomplete)));
    sqlx::query("ROLLBACK").execute(&mut *reader).await.unwrap();
    drop(reader);
    let accepted = recovery.replay(again, Timestamp(99), tests::capacity()).await.unwrap();
    assert_eq!(accepted.intent().page, page);
    assert_eq!(accepted.intent().accepted_at, Timestamp(7));
    recovery.close().await.unwrap();
}

#[tokio::test]
async fn cached_default_dml_denies_after_fence_and_private_scope_does_not_downgrade() {
    const UPDATE: &str = "UPDATE wiki_mutation_intents SET accepted_at=accepted_at WHERE page_id=1";
    let (directory, database, writer, command) = tests::fixture().await;
    let intent = database.wiki_mutations().accept(command, Timestamp(7), tests::capacity()).await.unwrap();
    let mut legacy = database.pool.acquire().await.unwrap();
    sqlx::query(UPDATE).execute(&mut *legacy).await.unwrap();
    let clone = writer.clone();
    let recovery = fenced(writer, directory.path()).await;
    let error = sqlx::query(UPDATE).execute(&mut *legacy).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().message(), "wiki writer protocol refused");
    let error = sqlx::query("INSERT INTO wiki_page_allocations DEFAULT VALUES").execute(&mut *legacy).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().message(), "wiki writer protocol refused");
    let error = sqlx::query("DELETE FROM wiki_mutation_intents").execute(&mut *legacy).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().message(), "wiki writer protocol refused");
    let error = sqlx::query("DELETE FROM repositories WHERE id=1").execute(&mut *legacy).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().message(), "wiki writer protocol refused");
    let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM repositories WHERE id=1").fetch_one(&mut *legacy).await.unwrap();
    assert_eq!(existing, 1);
    assert!(matches!(clone.accept(intent.command, Timestamp(8), tests::capacity()).await, Err(WikiWriteError::FencedOwnerRequired)));
    let downgrade = sqlx::query("UPDATE wiki_writer_protocol SET mode='legacy'").execute(&recovery.writer.0.pool).await.unwrap_err();
    assert_eq!(downgrade.as_database_error().unwrap().message(), "wiki protocol epoch or downgrade refused");
    recovery.close().await.unwrap();
}

#[tokio::test]
async fn stale_recovery_epoch_cannot_ack_even_matching_stored_command() {
    let (directory, database, writer, command) = tests::fixture().await;
    let intent = database.wiki_mutations().accept(command, Timestamp(7), tests::capacity()).await.unwrap();
    let recovery = fenced(writer, directory.path()).await;
    sqlx::query("UPDATE wiki_writer_protocol SET epoch=epoch+1").execute(&recovery.writer.0.pool).await.unwrap();
    assert!(matches!(
        recovery.replay(intent.command, Timestamp(8), tests::capacity()).await,
        Err(WikiWriteError::Acceptance(WikiMutationError::StaleProtocol))
    ));
    recovery.close().await.unwrap();
}

#[tokio::test]
async fn cancelled_replay_waiter_keeps_real_owner_and_finishes_the_owned_job() {
    use std::{future::Future, task::Poll};
    let (directory, database, writer, command) = tests::fixture().await;
    let intent = database.wiki_mutations().accept(command, Timestamp(7), tests::capacity()).await.unwrap();
    let recovery = fenced(writer, directory.path()).await;
    let mut blocker = recovery.writer.0.pool.acquire().await.unwrap();
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *blocker).await.unwrap();
    let waiting = recovery.replay(intent.command, Timestamp(8), tests::capacity());
    drop(waiting);
    let mut drain = Box::pin(recovery.close());
    std::future::poll_fn(|cx| match drain.as_mut().poll(cx) {
        Poll::Pending => Poll::Ready(()),
        Poll::Ready(_) => panic!("job is still blocked on the physical connection"),
    })
    .await;
    drop(drain);
    sqlx::query("ROLLBACK").execute(&mut *blocker).await.unwrap();
    drop(blocker);
    tokio::time::timeout(Duration::from_secs(5), recovery.close()).await.unwrap().unwrap();
    let exact: (i64, String) =
        sqlx::query_as("SELECT accepted_at,content FROM wiki_mutation_intents").fetch_one(&database.pool).await.unwrap();
    assert_eq!(exact, (7, "Exact body".to_owned()));
}

#[tokio::test]
async fn actual_guard_retention_and_fresh_full_open_do_not_leak_closed_permission() {
    let (directory, database, writer, command) = tests::fixture().await;
    sqlx::raw_sql("INSERT INTO repositories(id,owner_id,name,visibility,default_branch,created_at,updated_at) VALUES(2,1,'second','private','main',0,0); INSERT INTO wiki_repositories(repo_id,enabled) VALUES(2,1);")
        .execute(&database.pool).await.unwrap();
    let wiki: i64 = sqlx::query_scalar("SELECT id FROM wiki_repositories WHERE repo_id=2").fetch_one(&database.pool).await.unwrap();
    let mut second = retry(&command);
    second.id = WikiCommandId::new([2; 16]).unwrap();
    second.expected = rbit_wikis::policy::WikiExpected::Create {
        repository: rbit_core::RepoId(2),
        wiki: rbit_wikis::WikiId::new(wiki).unwrap(),
        head: None,
    };
    let original = database.wiki_mutations().accept(command, Timestamp(7), tests::capacity()).await.unwrap();
    let clone = writer.clone();
    let recovery = fenced(writer, directory.path()).await;
    assert!(clone.0.pool.is_closed(), "initial LEGACY physical generation was closed before private setup");
    assert!(clone.0.state.accounted());
    assert!(!Arc::ptr_eq(&clone.0, &recovery.writer.0));
    let private = recovery.writer.clone();
    let accepted = recovery.replay(original.command, Timestamp(9), tests::capacity()).await.unwrap();
    drop(recovery);
    assert!(matches!(database.wiki_catalog_lease().await, Err(WikiCatalogLeaseError::Busy)));
    drop(accepted);
    // Its callback remains immutable and the real root remains owned through
    // physical close. No last-result drop mutates registered comparison answers.
    let still_private: i64 =
        sqlx::query_scalar("SELECT 'managed' COLLATE rbit_wiki_writer_v1 = 'permit'").fetch_one(&private.0.pool).await.unwrap();
    assert_eq!(still_private, 1);
    assert!(matches!(database.wiki_catalog_lease().await, Err(WikiCatalogLeaseError::Busy)));
    private.close().await.unwrap();
    assert!(private.0.state.accounted());
    drop(private);
    clone.close().await.unwrap();
    drop(clone);
    let fresh = FullWikiWriter::open(database.wiki_catalog_lease().await.unwrap()).await.unwrap();
    let only_legacy: i64 =
        sqlx::query_scalar("SELECT 'managed' COLLATE rbit_wiki_writer_v1 = 'permit'").fetch_one(&fresh.0.pool).await.unwrap();
    assert_eq!(only_legacy, 0);
    let result = fresh.accept(second, Timestamp(10), NonZeroU32::new(2).unwrap()).await;
    match result {
        Err(WikiWriteError::Acceptance(WikiMutationError::Storage(crate::StorageError::Sqlite(error)))) => {
            assert_eq!(error.as_database_error().unwrap().message(), "wiki writer protocol refused");
        }
        other => panic!("fresh unfenced FULL owner must not write managed data: {other:?}"),
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wiki_mutation_intents").fetch_one(&database.pool).await.unwrap();
    assert_eq!(count, 1);
    fresh.close().await.unwrap();
}

#[tokio::test]
async fn discarded_unpolled_handoff_keeps_root_until_initial_connection_and_replacement_really_close() {
    let (directory, database, writer, _command) = tests::fixture().await;
    let initial = writer.clone();
    let mut borrowed = initial.0.pool.acquire().await.unwrap();
    let native = objects(&writer, directory.path());
    let response = writer.begin_recovery(native);
    drop(response);
    tokio::time::timeout(Duration::from_secs(5), async {
        while !initial.0.tasks.is_closed() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let legacy: i64 =
        sqlx::query_scalar("SELECT 'managed' COLLATE rbit_wiki_writer_v1 = 'permit'").fetch_one(&mut *borrowed).await.unwrap();
    assert_eq!(legacy, 0, "no private generation exists while the first is physically borrowed");
    let object_file = File::options().read(true).write(true).open(directory.path().join("physical-object-scope")).unwrap();
    assert!(matches!(object_file.try_lock(), Err(std::fs::TryLockError::WouldBlock)));
    assert!(matches!(database.wiki_catalog_lease().await, Err(WikiCatalogLeaseError::Busy)));
    drop(borrowed);
    initial.close().await.unwrap();
    drop(initial);
    let next = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match database.wiki_catalog_lease().await {
                Ok(lease) => break lease,
                Err(WikiCatalogLeaseError::Busy) => tokio::task::yield_now().await,
                Err(error) => panic!("unexpected catalog failure: {error:?}"),
            }
        }
    })
    .await
    .unwrap();
    let mode: (String, i64) =
        sqlx::query_as("SELECT mode,epoch FROM wiki_writer_protocol WHERE singleton=1").fetch_one(&database.pool).await.unwrap();
    assert_eq!(mode, ("preparing".to_owned(), 1));
    object_file.try_lock().unwrap();
    drop(next);
}
