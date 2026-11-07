//! Closed temporary catalogs; no native publication or power-loss simulation.
use super::*;
use rbit_core::{AccountId, RepoId};
use rbit_wikis::{WikiId, commands::WikiCommandId, policy::WikiExpected};
use std::time::Duration;

pub(super) async fn fixture() -> (tempfile::TempDir, crate::Database, FullWikiWriter, WikiWriteCommand) {
    let (directory, database) = crate::sqlite::plans::database().await;
    sqlx::raw_sql(
        "INSERT INTO accounts(id,kind,name,created_at,updated_at) VALUES(1,'user','owner',0,0);
         INSERT INTO user_logins(account_id,email,password_hash) VALUES(1,'owner@example.test','');
         INSERT INTO repositories(id,owner_id,name,visibility,default_branch,created_at,updated_at) VALUES(1,1,'project','private','main',0,0);
         INSERT INTO wiki_repositories(repo_id,enabled) VALUES(1,1);",
    ).execute(&database.pool).await.unwrap();
    let wiki: i64 = sqlx::query_scalar("SELECT id FROM wiki_repositories WHERE repo_id=1").fetch_one(&database.pool).await.unwrap();
    let writer = FullWikiWriter::open(database.wiki_catalog_lease().await.unwrap()).await.unwrap();
    let command = WikiWriteCommand {
        id: WikiCommandId::new([1; 16]).unwrap(),
        actor: AccountId(1),
        expected: WikiExpected::Create { repository: RepoId(1), wiki: WikiId::new(wiki).unwrap(), head: None },
        path: "Guide".parse().unwrap(),
        title: "Title".parse().unwrap(),
        content: "Exact body".to_owned().try_into().unwrap(),
    };
    (directory, database, writer, command)
}

pub(super) fn capacity() -> NonZeroU32 {
    NonZeroU32::new(1).unwrap()
}

#[tokio::test]
async fn full_configuration_is_verified_on_replacement_without_changing_general_pool() {
    let (_directory, database, writer, _command) = fixture().await;
    let normal: i64 = sqlx::query_scalar("PRAGMA synchronous").fetch_one(&database.pool).await.unwrap();
    assert_eq!(normal, 1);
    for _ in 0..2 {
        let mut connection = writer.0.pool.acquire().await.unwrap();
        let full: i64 = sqlx::query_scalar("PRAGMA synchronous").fetch_one(&mut *connection).await.unwrap();
        assert_eq!(full, 2);
        writer.0.state.close_physical(connection).await.unwrap();
    }
    writer.close().await.unwrap();
}

#[tokio::test]
async fn successful_receipt_requires_committed_invisible_intent_and_retains_catalog() {
    let (_directory, database, writer, command) = fixture().await;
    let receipt = writer.accept(command, Timestamp(7), capacity()).await.unwrap();
    let row: (i64, String) =
        sqlx::query_as("SELECT accepted_at,content FROM wiki_mutation_intents").fetch_one(&database.pool).await.unwrap();
    assert_eq!(row, (7, "Exact body".to_owned()));
    let visible: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wiki_pages").fetch_one(&database.pool).await.unwrap();
    assert_eq!(visible, 0);
    assert_eq!(receipt.intent().accepted_at, Timestamp(7));
    assert_eq!(format!("{receipt:?}"), "CommittedWikiAcceptance(..)");
    writer.close().await.unwrap();
    drop(writer);
    assert!(matches!(database.wiki_catalog_lease().await, Err(WikiCatalogLeaseError::Busy)));
    drop(receipt);
    database.wiki_catalog_lease().await.unwrap();
}

#[tokio::test]
async fn final_commit_failure_produces_no_receipt_and_rolls_back_all_acceptance_allocations() {
    let (_directory, database, writer, command) = fixture().await;
    // Connection-local deferred FK failure executes the real acceptance statements,
    // then fails only COMMIT. No external DDL invalidates another pooled connection.
    let mut connection = writer.0.pool.acquire().await.unwrap();
    sqlx::raw_sql(
        "CREATE TEMP TABLE commit_parent(id INTEGER PRIMARY KEY);
         CREATE TEMP TABLE commit_child(parent INTEGER REFERENCES commit_parent(id) DEFERRABLE INITIALLY DEFERRED);
         CREATE TEMP TRIGGER fail_acceptance_commit AFTER INSERT ON main.wiki_mutation_intents BEGIN INSERT INTO commit_child VALUES(1); END;",
    ).execute(&mut *connection).await.unwrap();
    drop(connection);
    // Migration 0066 may already record a zero sequence row; only advancement matters.
    let sequence = "SELECT seq FROM sqlite_sequence WHERE name='wiki_page_allocations'";
    let before: Option<i64> = sqlx::query_scalar(sequence).fetch_optional(&database.pool).await.unwrap();
    assert!(before.is_none_or(|seq| seq == 0));
    assert!(matches!(
        writer.accept(command, Timestamp(7), capacity()).await,
        Err(WikiWriteError::Acceptance(WikiMutationError::Storage(_)))
    ));
    writer.close().await.unwrap();
    for table in ["wiki_mutation_intents", "wiki_actor_incarnations", "wiki_page_allocations"] {
        let query = sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}"));
        let rows: i64 = sqlx::query_scalar(query).fetch_one(&database.pool).await.unwrap();
        assert_eq!(rows, 0, "{table} rolled back");
    }
    let allocated: Option<i64> = sqlx::query_scalar(sequence).fetch_optional(&database.pool).await.unwrap();
    assert_eq!(allocated, before);
}

#[tokio::test]
async fn cancellation_and_close_waiter_drop_do_not_abandon_admitted_sql_work() {
    use std::{future::Future, task::Poll};
    let (_directory, database, writer, command) = fixture().await;
    let mut blocker = database.pool.acquire().await.unwrap();
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *blocker).await.unwrap();
    // Admission happens at the method call, even if the returned future is unpolled.
    let waiting = writer.accept(command, Timestamp(7), capacity());
    assert_eq!(writer.0.admission.available_permits(), 0);
    drop(waiting);
    let mut drain = Box::pin(writer.close());
    std::future::poll_fn(|cx| match drain.as_mut().poll(cx) {
        Poll::Pending => Poll::Ready(()),
        Poll::Ready(_) => panic!("admitted command is still blocked"),
    })
    .await;
    drop(drain);
    assert!(writer.0.tasks.is_closed());
    sqlx::query("ROLLBACK").execute(&mut *blocker).await.unwrap();
    drop(blocker);
    tokio::time::timeout(Duration::from_secs(5), writer.close()).await.unwrap().unwrap();
    let accepted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wiki_mutation_intents").fetch_one(&database.pool).await.unwrap();
    assert_eq!(accepted, 1);
    let retained = writer.catalog();
    drop(writer);
    assert!(matches!(database.wiki_catalog_lease().await, Err(WikiCatalogLeaseError::Busy)));
    drop(retained);
    database.wiki_catalog_lease().await.unwrap();
}

#[tokio::test]
async fn saturated_and_closed_admission_do_not_acquire_sql_connections() {
    let (_directory, _database, writer, command) = fixture().await;
    let held = Arc::clone(&writer.0.admission).try_acquire_owned().unwrap();
    let (busy, work) = crate::count_work(writer.accept(command, Timestamp(7), capacity())).await;
    assert!(matches!(busy, Err(WikiWriteError::Busy)));
    assert_eq!(work, crate::Work::default());
    assert_eq!(writer.0.tasks.len(), 0);
    drop(held);
    writer.close().await.unwrap();
    let (_dir, _db, other, command) = fixture().await;
    let (closed, work) = crate::count_work(writer.accept(command, Timestamp(7), capacity())).await;
    assert!(matches!(closed, Err(WikiWriteError::Closed)));
    assert_eq!(work, crate::Work::default());
    assert_eq!(writer.0.tasks.len(), 0);
    other.close().await.unwrap();
}

#[tokio::test]
async fn read_only_replay_never_restamps_or_claims_full_durability_for_normal_origin() {
    let (_directory, database, writer, command) = fixture().await;
    let intent = database.wiki_mutations().accept(command, Timestamp(7), capacity()).await.unwrap();
    let page = intent.page;
    let generation = intent.generation;
    assert!(matches!(writer.accept(intent.command, Timestamp(99), capacity()).await, Err(WikiWriteError::ReplayRequiresFence)));
    writer.close().await.unwrap();
    let row: (i64, i64, i64, String) =
        sqlx::query_as("SELECT page_id,generation,accepted_at,content FROM wiki_mutation_intents").fetch_one(&database.pool).await.unwrap();
    assert_eq!(row, (page.get(), generation.get(), 7, "Exact body".to_owned()));
}

#[tokio::test]
async fn full_new_acceptance_keeps_the_original_statement_and_acquisition_budget() {
    let (_one, normal, first, command) = fixture().await;
    let (_two, _database, full, next) = fixture().await;
    let (a, normal_work) = crate::count_work(normal.wiki_mutations().accept(command, Timestamp(7), capacity())).await;
    a.unwrap();
    // Count inside the actual worker boundary: task-local counters intentionally do
    // not propagate through Tokio spawn. This calls the exact shared SQL worker.
    let (b, full_work) = crate::count_work(accept_on(&full.0.commands, next, Timestamp(7), capacity(), AcceptancePolicy::AllowNew)).await;
    assert!(b.unwrap().inserted);
    assert_eq!(full_work, normal_work);
    assert_eq!(full_work.acquisitions, 1);
    first.close().await.unwrap();
    full.close().await.unwrap();
}
