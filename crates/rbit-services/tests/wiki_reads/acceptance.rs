//! Real SQL command acceptance is pending work, never a visible wiki write.
use super::fixture::Fixture;
use rbit_core::{Timestamp, Visibility};
use rbit_services::CreateRepository;
use rbit_storage::sqlite::wiki_mutations::WikiMutationError;
use rbit_wikis::{
    WikiContent,
    commands::{WikiCommandId, WikiWriteCommand},
    policy::{WikiExpected, WikiPolicyError},
    reading::{WikiPageRequest, WikiReadView, WikiReader},
};
use std::num::NonZeroU32;

pub(super) fn command(fixture: &Fixture, byte: u8, text: &str) -> WikiWriteCommand {
    WikiWriteCommand {
        id: WikiCommandId::new([byte; 16]).unwrap(),
        actor: fixture.owner.id(),
        expected: WikiExpected::Create { repository: fixture.repo.repo.repo.id, wiki: fixture.wiki, head: None },
        path: "Guide".parse().unwrap(),
        title: "Title".parse().unwrap(),
        content: WikiContent::try_from(text.to_owned()).unwrap(),
    }
}
pub(super) fn capacity() -> NonZeroU32 {
    NonZeroU32::new(1).unwrap()
}

#[tokio::test]
async fn accepted_command_stays_invisible_and_exact_retry_reuses_committed_page_time_and_generation() {
    let fixture = Fixture::new().await;
    let store = fixture.database.wiki_mutations();
    let first = store.accept(command(&fixture, 1, "private body"), Timestamp(10), capacity()).await.unwrap();
    let retried = store.accept(command(&fixture, 1, "private body"), Timestamp(99), capacity()).await.unwrap();
    assert_eq!((first.page, first.generation, first.accepted_at), (retried.page, retried.generation, retried.accepted_at));
    assert_eq!(first.accepted_at, Timestamp(10));
    assert!(first.target.is_none());
    assert!(!fixture.directory.path().join(format!("wiki-objects/{}", fixture.repo.repo.repo.id.0)).exists());
    let path = "Guide".parse().unwrap();
    let visible = WikiReader::page(
        &fixture.database.wikis(),
        &fixture.objects,
        WikiPageRequest { repository: fixture.repo.repo.repo.id, viewer: Some(fixture.owner.id()), path: &path },
    )
    .await;
    assert!(matches!(visible, Ok(WikiReadView::Empty { .. })));
    let changed = store.accept(command(&fixture, 1, "changed body"), Timestamp(99), capacity()).await;
    assert!(matches!(changed, Err(WikiMutationError::ReplayMismatch)));
    let mut changed = command(&fixture, 1, "private body");
    changed.title = "Different".parse().unwrap();
    assert!(matches!(store.accept(changed, Timestamp(99), capacity()).await, Err(WikiMutationError::ReplayMismatch)));
    assert!(matches!(
        store.accept(command(&fixture, 2, "other"), Timestamp(99), capacity()).await,
        Err(WikiMutationError::Policy(WikiPolicyError::Pending))
    ));
}

#[tokio::test]
async fn racing_acceptances_have_one_winner_and_one_pending_with_no_visible_page() {
    let fixture = Fixture::new().await;
    let store = fixture.database.wiki_mutations();
    let (a, b) = tokio::join!(
        store.accept(command(&fixture, 1, "a"), Timestamp(10), capacity()),
        store.accept(command(&fixture, 2, "b"), Timestamp(10), capacity())
    );
    match (a, b) {
        (Ok(_), Err(WikiMutationError::Policy(WikiPolicyError::Pending)))
        | (Err(WikiMutationError::Policy(WikiPolicyError::Pending)), Ok(_)) => {}
        other => panic!("unexpected acceptance outcome: {other:?}"),
    }
    let visible: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wiki_pages").fetch_one(&fixture.raw).await.unwrap();
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wiki_mutation_intents").fetch_one(&fixture.raw).await.unwrap();
    assert_eq!((visible, pending), (0, 1));
}

#[tokio::test]
async fn fresh_private_permissions_and_archive_precede_existing_command_metadata() {
    let fixture = Fixture::new().await;
    let store = fixture.database.wiki_mutations();
    store.accept(command(&fixture, 1, "private"), Timestamp(10), capacity()).await.unwrap();
    let mut foreign = command(&fixture, 1, "private");
    foreign.actor = fixture.outsider.id();
    assert!(matches!(store.accept(foreign, Timestamp(11), capacity()).await, Err(WikiMutationError::Policy(WikiPolicyError::NotFound))));
    fixture.services.repo_settings.set_archived(&fixture.owner, &fixture.repo, true).await.unwrap();
    assert!(matches!(
        store.accept(command(&fixture, 1, "private"), Timestamp(11), capacity()).await,
        Err(WikiMutationError::Policy(WikiPolicyError::Archived))
    ));
}

#[tokio::test]
async fn global_pending_capacity_refuses_another_wiki_without_allocating_its_page() {
    let fixture = Fixture::new().await;
    let store = fixture.database.wiki_mutations();
    store.accept(command(&fixture, 1, "a"), Timestamp(10), capacity()).await.unwrap();
    fixture
        .services
        .repositories
        .create(&fixture.owner, CreateRepository { name: "second".into(), description: String::new(), visibility: Visibility::Private })
        .await
        .unwrap();
    let second = fixture.services.repositories.open(Some(&fixture.owner), "owner", "second").await.unwrap();
    let wiki: i64 = sqlx::query_scalar("INSERT INTO wiki_repositories(repo_id,enabled) VALUES(?,1) RETURNING id")
        .bind(second.repo.repo.id.0)
        .fetch_one(&fixture.raw)
        .await
        .unwrap();
    let mut next = command(&fixture, 2, "b");
    next.expected = WikiExpected::Create { repository: second.repo.repo.id, wiki: rbit_wikis::WikiId::new(wiki).unwrap(), head: None };
    assert!(matches!(store.accept(next, Timestamp(11), capacity()).await, Err(WikiMutationError::Capacity)));
    let allocated: i64 =
        sqlx::query_scalar("SELECT seq FROM sqlite_sequence WHERE name='wiki_page_allocations'").fetch_one(&fixture.raw).await.unwrap();
    assert_eq!(allocated, 1);
}

#[tokio::test]
async fn page_allocation_respects_catalog_highwater_and_exhaustion_does_not_leave_an_intent() {
    let fixture = Fixture::new().await;
    let revision = fixture.objects(19, "existing", None, 1).await;
    fixture.committed(19, "Existing", "Existing", 1, revision).await;
    let mut next = command(&fixture, 1, "next");
    next.expected = WikiExpected::Create {
        repository: fixture.repo.repo.repo.id,
        wiki: fixture.wiki,
        head: Some(rbit_wikis::policy::WikiHead { generation: rbit_wikis::WikiGeneration::INITIAL, revision }),
    };
    let intent = fixture.database.wiki_mutations().accept(next, Timestamp(10), capacity()).await.unwrap();
    assert_eq!(intent.page.get(), 20);
    let empty = Fixture::new().await;
    sqlx::query("INSERT INTO wiki_page_allocations(id) VALUES(?)").bind(i64::MAX).execute(&empty.raw).await.unwrap();
    sqlx::query("DELETE FROM wiki_page_allocations").execute(&empty.raw).await.unwrap();
    assert!(matches!(
        empty.database.wiki_mutations().accept(command(&empty, 1, "refused"), Timestamp(10), capacity()).await,
        Err(WikiMutationError::Exhausted)
    ));
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wiki_mutation_intents").fetch_one(&empty.raw).await.unwrap();
    assert_eq!(pending, 0);
}

#[tokio::test]
async fn collaborator_write_is_fresh_and_revocation_precedes_pending_replay() {
    let fixture = Fixture::new().await;
    fixture
        .services
        .collaborators
        .add(&fixture.owner, &fixture.repo, fixture.outsider.name().as_str(), rbit_core::Permission::Read)
        .await
        .unwrap();
    let foreign = || {
        let mut command = command(&fixture, 1, "body");
        command.actor = fixture.outsider.id();
        command
    };
    let store = fixture.database.wiki_mutations();
    assert!(matches!(store.accept(foreign(), Timestamp(1), capacity()).await, Err(WikiMutationError::Policy(WikiPolicyError::Forbidden))));
    fixture
        .database
        .collaborators()
        .set_permission(fixture.repo.repo.repo.id, fixture.outsider.id(), rbit_core::Permission::Write)
        .await
        .unwrap();
    let accepted = store.accept(foreign(), Timestamp(1), capacity()).await.unwrap();
    assert_eq!(accepted.command.actor, fixture.outsider.id());
    fixture.services.collaborators.remove(&fixture.owner, &fixture.repo, fixture.outsider.name().as_str()).await.unwrap();
    assert!(matches!(store.accept(foreign(), Timestamp(2), capacity()).await, Err(WikiMutationError::Policy(WikiPolicyError::NotFound))));
}

#[tokio::test]
async fn edit_reuses_page_identity_and_never_silently_renames_or_refreshes_head() {
    let fixture = Fixture::new().await;
    let revision = fixture.objects(1, "old", None, 1).await;
    fixture.committed(1, "Guide", "Original", 1, revision).await;
    let head = rbit_wikis::policy::WikiHead { generation: rbit_wikis::WikiGeneration::INITIAL, revision };
    let editing = || {
        let mut next = command(&fixture, 1, "edited");
        next.expected = WikiExpected::Change {
            repository: fixture.repo.repo.repo.id,
            wiki: fixture.wiki,
            page: rbit_wikis::WikiPageId::new(1).unwrap(),
            head,
        };
        next
    };
    let store = fixture.database.wiki_mutations();
    let mut renamed = editing();
    renamed.path = "Elsewhere".parse().unwrap();
    assert!(matches!(store.accept(renamed, Timestamp(2), capacity()).await, Err(WikiMutationError::Policy(WikiPolicyError::Conflict))));
    assert!(matches!(
        store.accept(command(&fixture, 1, "stale"), Timestamp(2), capacity()).await,
        Err(WikiMutationError::Policy(WikiPolicyError::Conflict))
    ));
    let accepted = store.accept(editing(), Timestamp(2), capacity()).await.unwrap();
    assert_eq!(accepted.page.get(), 1);
    assert_eq!(accepted.generation.get(), 2);
}

#[tokio::test]
async fn schema_refuses_rebinding_an_accepted_command_body_metadata_or_existing_target() {
    let fixture = Fixture::new().await;
    let store = fixture.database.wiki_mutations();
    store.accept(command(&fixture, 1, "original"), Timestamp(1), capacity()).await.unwrap();
    for query in [
        "UPDATE wiki_mutation_intents SET content='replacement'",
        "UPDATE wiki_mutation_intents SET title='replacement'",
        "UPDATE wiki_mutation_intents SET accepted_at=2",
        "UPDATE wiki_mutation_intents SET page_id=2",
    ] {
        assert!(sqlx::query(query).execute(&fixture.raw).await.is_err());
    }
    // Raw target data is seeded ONLY to test its immutable binding. It is not
    // closure evidence and this test never makes a page/ref visible.
    sqlx::query("UPDATE wiki_mutation_intents SET target_revision=?").bind("a".repeat(40)).execute(&fixture.raw).await.unwrap();
    assert!(sqlx::query("UPDATE wiki_mutation_intents SET target_revision=?").bind("b".repeat(40)).execute(&fixture.raw).await.is_err());
    assert!(sqlx::query("UPDATE wiki_mutation_intents SET target_revision=NULL").execute(&fixture.raw).await.is_err());
    let retried = store.accept(command(&fixture, 1, "original"), Timestamp(3), capacity()).await.unwrap();
    assert_eq!(retried.target.unwrap().to_string(), "a".repeat(40));
}
