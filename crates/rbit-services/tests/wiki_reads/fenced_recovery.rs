//! Actual native Git owner + SQLite fence, still no listener/readiness/publisher.
use super::{
    acceptance::{capacity, command},
    fixture::Fixture,
};
use rbit_core::Timestamp;
use rbit_git::wikis::{WikiObjectWriter, WikiObjects};
use rbit_storage::sqlite::wiki_mutations::{
    WikiMutationError,
    full::{FullWikiWriter, WikiWriteError},
};
use rbit_wikis::ownership::WikiCatalogOwnership;

#[tokio::test]
async fn actual_catalog_bound_git_owner_replays_only_exact_pending_sql_after_real_checkpoint() {
    let fixture = Fixture::new().await;
    let original = fixture.database.wiki_mutations().accept(command(&fixture, 1, "private"), Timestamp(7), capacity()).await.unwrap();
    let expected = (original.page, original.generation, original.accepted_at);
    let full = FullWikiWriter::open(fixture.database.wiki_catalog_lease().await.unwrap()).await.unwrap();
    let object_root = fixture.directory.path().join("fenced-objects");
    let catalog = full.catalog();
    let native = WikiObjects::open(&object_root, catalog.store_id(), fixture.work.clone()).unwrap();
    let native = WikiObjectWriter::open(native).unwrap().bind_catalog(catalog).unwrap();
    let legacy_clone = full.clone();
    let recovery = full.begin_recovery(native).await.unwrap();
    let mode: (String, i64) =
        sqlx::query_as("SELECT mode,epoch FROM wiki_writer_protocol WHERE singleton=1").fetch_one(&fixture.raw).await.unwrap();
    assert_eq!(mode, ("preparing".to_owned(), 1));
    let accepted = recovery.replay(original.command, Timestamp(99), capacity()).await.unwrap();
    assert_eq!((accepted.intent().page, accepted.intent().generation, accepted.intent().accepted_at), expected);
    let mut forbidden = command(&fixture, 1, "private");
    forbidden.actor = fixture.outsider.id();
    assert!(matches!(
        recovery.replay(forbidden, Timestamp(8), capacity()).await,
        Err(WikiWriteError::Acceptance(WikiMutationError::Policy(rbit_wikis::policy::WikiPolicyError::NotFound)))
    ));
    assert!(matches!(
        legacy_clone.accept(command(&fixture, 2, "new"), Timestamp(8), capacity()).await,
        Err(WikiWriteError::FencedOwnerRequired)
    ));
    assert!(matches!(
        recovery.replay(command(&fixture, 2, "new"), Timestamp(8), capacity()).await,
        Err(WikiWriteError::Acceptance(WikiMutationError::MissingIntent))
    ));
    assert!(matches!(
        recovery.replay(command(&fixture, 1, "changed"), Timestamp(8), capacity()).await,
        Err(WikiWriteError::Acceptance(WikiMutationError::ReplayMismatch))
    ));
    assert!(
        !object_root.join(format!("{}/{}.git", fixture.repo.repo.repo.id.0, fixture.wiki.get())).exists(),
        "SQL recovery does not prepare or publish native objects"
    );
    recovery.close().await.unwrap();
}

#[tokio::test]
async fn a_foreign_actual_catalog_root_cannot_switch_main_protocol_or_gain_replay_fact() {
    let fixture = Fixture::new().await;
    let foreign = Fixture::new().await;
    let full = FullWikiWriter::open(fixture.database.wiki_catalog_lease().await.unwrap()).await.unwrap();
    let foreign_full = FullWikiWriter::open(foreign.database.wiki_catalog_lease().await.unwrap()).await.unwrap();
    let catalog = foreign_full.catalog();
    let native = WikiObjects::open(&foreign.directory.path().join("fenced-objects"), catalog.store_id(), foreign.work.clone()).unwrap();
    let native = WikiObjectWriter::open(native).unwrap().bind_catalog(catalog).unwrap();
    let close = full.clone();
    assert!(matches!(full.begin_recovery(native).await, Err(WikiWriteError::Binding)));
    let mode: (String, i64) =
        sqlx::query_as("SELECT mode,epoch FROM wiki_writer_protocol WHERE singleton=1").fetch_one(&fixture.raw).await.unwrap();
    assert_eq!(mode, ("legacy".to_owned(), 0));
    close.close().await.unwrap();
    foreign_full.close().await.unwrap();
}
