use super::*;
use crate::sqlite::wiki_mutations::full::{fencing_tests, tests as fixture};
use rbit_core::{RepoId, Timestamp};
use rbit_wikis::{commands::WikiCommandId, policy::WikiExpected};

async fn add_repository(database: &crate::Database, id: i64, enabled: bool) -> WikiId {
    sqlx::query(
        "INSERT INTO repositories(id,owner_id,name,visibility,default_branch,created_at,updated_at) VALUES(?,1,?,'private','main',0,0)",
    )
    .bind(id)
    .bind(format!("project-{id}"))
    .execute(&database.pool)
    .await
    .unwrap();
    let wiki: i64 = sqlx::query_scalar("INSERT INTO wiki_repositories(repo_id,enabled) VALUES(?,?) RETURNING id")
        .bind(id)
        .bind(enabled)
        .fetch_one(&database.pool)
        .await
        .unwrap();
    WikiId::new(wiki).unwrap()
}

fn pending_command(wiki: WikiId, repository: i64, byte: u8) -> rbit_wikis::commands::WikiWriteCommand {
    rbit_wikis::commands::WikiWriteCommand {
        id: WikiCommandId::new([byte; 16]).unwrap(),
        actor: rbit_core::AccountId(1),
        expected: WikiExpected::Create { repository: RepoId(repository), wiki, head: None },
        path: "Guide".parse().unwrap(),
        title: "Title".parse().unwrap(),
        content: "body".to_owned().try_into().unwrap(),
    }
}

#[tokio::test]
async fn bounded_sorted_resume_includes_disabled_and_orphan_scopes_without_adopting_reused_repository_id() {
    let (directory, database, writer, _command) = fixture::fixture().await;
    let second = add_repository(&database, 2, true).await;
    let third = add_repository(&database, 3, false).await;
    let fourth = add_repository(&database, 4, true).await;
    let capacity = std::num::NonZeroU32::new(2).unwrap();
    database.wiki_mutations().accept(pending_command(second, 2, 2), Timestamp(7), capacity).await.unwrap();
    database.wiki_mutations().accept(pending_command(fourth, 4, 4), Timestamp(7), capacity).await.unwrap();
    sqlx::query("DELETE FROM repositories WHERE id=2").execute(&database.pool).await.unwrap();
    let replacement = add_repository(&database, 2, true).await;
    assert_ne!(replacement, second);
    let recovery = fencing_tests::fenced(writer, directory.path()).await;
    let size = WikiStartupBatchSize::new(2).unwrap();
    let mut cursor = None;
    let mut rows = Vec::new();
    loop {
        let batch = recovery.startup_batch(cursor, size).await.unwrap();
        assert!(batch.scopes().len() <= 2);
        let (scopes, next) = batch.into_parts();
        rows.extend(scopes);
        if next.is_none() {
            break;
        }
        cursor = next;
    }
    assert_eq!(rows.iter().map(|row| row.wiki.get()).collect::<Vec<_>>(), [1, second.get(), third.get(), fourth.get(), replacement.get()]);
    let orphan = rows.iter().find(|row| row.wiki == second).unwrap();
    assert!(orphan.orphaned && orphan.pending && !orphan.enabled && orphan.head.is_none());
    assert_eq!(orphan.repository, RepoId(2));
    let current = rows.iter().find(|row| row.wiki == replacement).unwrap();
    assert!(!current.orphaned && !current.pending && current.enabled);
    assert_eq!(current.repository, RepoId(2));
    assert!(!rows.iter().find(|row| row.wiki == third).unwrap().enabled);
    assert_eq!(rows.iter().filter(|row| row.wiki == fourth).count(), 1, "catalog/pending sources merge without duplicate work");
    recovery.close().await.unwrap();
}

#[tokio::test]
async fn cursor_is_bound_to_actual_owner_and_epoch_and_cannot_resume_another_catalog() {
    let (first_directory, first, first_writer, _one) = fixture::fixture().await;
    let (second_directory, _second, second_writer, _two) = fixture::fixture().await;
    add_repository(&first, 2, true).await;
    let a = fencing_tests::fenced(first_writer, first_directory.path()).await;
    let b = fencing_tests::fenced(second_writer, second_directory.path()).await;
    let size = WikiStartupBatchSize::new(1).unwrap();
    let (_, cursor) = a.startup_batch(None, size).await.unwrap().into_parts();
    assert!(matches!(b.startup_batch(cursor, size).await, Err(WikiWriteError::Binding)));
    let (_, cursor) = a.startup_batch(None, size).await.unwrap().into_parts();
    sqlx::query("UPDATE wiki_writer_protocol SET epoch=epoch+1").execute(&a.writer.0.pool).await.unwrap();
    assert!(matches!(a.startup_batch(cursor, size).await, Err(WikiWriteError::Binding)));
    a.close().await.unwrap();
    b.close().await.unwrap();
}

#[tokio::test]
async fn a_batch_has_three_statements_one_acquisition_and_both_ranges_use_indexes() {
    let (directory, database, writer, _command) = fixture::fixture().await;
    let recovery = fencing_tests::fenced(writer, directory.path()).await;
    let size = WikiStartupBatchSize::new(1).unwrap();
    // Counter intentionally runs inside the exact private SQL worker; task-local
    // metrics do not propagate into independently owned background jobs.
    let (rows, work) = crate::count_work(read_rows(&recovery.writer.0.commands, recovery.epoch, 0, size)).await;
    let (catalog, pending) = rows.unwrap();
    assert_eq!(catalog.len(), 1);
    assert_eq!(pending, Vec::<PendingRow>::new());
    assert_eq!(work.acquisitions, 1);
    assert_eq!(work.statements, 3);
    assert_catalog_plans(&database).await;
    crate::sqlite::plans::assert_no_scans(&database, PENDING, &[]).await;
    recovery.close().await.unwrap();
}

/// CATALOG keyset reads `w` by its rowid range; the left-joined revision `v` stays a
/// primary-key lookup under every statistics scenario.
async fn assert_catalog_plans(database: &crate::Database) {
    use crate::sqlite::plans::{Stats, query_plan_with};
    let mut required = Vec::new();
    for stats in [Stats::None, Stats::Large] {
        let plan = query_plan_with(database, stats, CATALOG).await;
        assert!(plan.iter().any(|detail| detail.starts_with("SEARCH w USING INTEGER PRIMARY KEY")), "{stats:?}: {plan:?}");
        assert!(!plan.iter().any(|detail| detail.starts_with("SCAN")), "{stats:?}: {plan:?}");
        let others: Vec<String> = plan.into_iter().filter(|detail| !detail.starts_with("SEARCH w ")).collect();
        assert!(others.iter().all(|detail| detail.starts_with("SEARCH v USING PRIMARY KEY")), "{stats:?}: {others:?}");
        assert!(required.is_empty() || required == others, "{stats:?}: {others:?} differs from {required:?}");
        required = others;
    }
    // As the planner-scenario contract in `plans.rs` and the login precedent document,
    // one-row statistics can make a rowid range cheaper as a scan of `w`. Allow only
    // that table's scan; every other table keeps its None/Large access exactly.
    let tiny = query_plan_with(database, Stats::Tiny, CATALOG).await;
    let (own, others): (Vec<String>, Vec<String>) =
        tiny.iter().cloned().partition(|detail| detail == "SCAN w" || detail.starts_with("SEARCH w USING INTEGER PRIMARY KEY"));
    assert_eq!(own.len(), 1, "unexpected Tiny-statistics access: {tiny:?}");
    assert_eq!(others, required, "unexpected Tiny-statistics access: {tiny:?}");
}

#[test]
fn malformed_catalog_head_flags_and_cross_source_repository_mismatch_are_refused() {
    let row = || CatalogRow { wiki: 1, repository: 1, enabled: 1, generation: None, revision: None };
    let mut invalid = row();
    invalid.enabled = 2;
    assert!(matches!(merge::scopes(vec![invalid], vec![], 1), Err(WikiWriteError::Binding)));
    let mut invalid = row();
    invalid.generation = Some(1);
    assert!(matches!(merge::scopes(vec![invalid], vec![], 1), Err(WikiWriteError::Binding)));
    assert!(matches!(merge::scopes(vec![row()], vec![PendingRow { wiki: 1, repository: 2 }], 1), Err(WikiWriteError::Binding)));
}

#[tokio::test]
async fn dropping_unpolled_batch_response_does_not_remove_adopted_sql_work_or_release_its_catalog() {
    let (directory, database, writer, _command) = fixture::fixture().await;
    let recovery = fencing_tests::fenced(writer, directory.path()).await;
    let connection = recovery.writer.0.pool.acquire().await.unwrap();
    let response = recovery.startup_batch(None, WikiStartupBatchSize::new(1).unwrap());
    assert_eq!(recovery.writer.0.tasks.len(), 1, "adoption precedes polling the response");
    drop(response);
    let mut drain = std::pin::pin!(recovery.close());
    std::future::poll_fn(|cx| {
        assert!(drain.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    assert!(matches!(database.wiki_catalog_lease().await, Err(super::super::WikiCatalogLeaseError::Busy)));
    drop(connection);
    drain.await.unwrap();
}
