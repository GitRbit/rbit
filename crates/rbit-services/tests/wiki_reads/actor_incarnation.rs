//! Numeric account ID reuse is not continued authority over an older command.
use super::{
    acceptance::{capacity, command},
    fixture::Fixture,
};
use rbit_core::{Permission, Timestamp, Visibility};
use rbit_services::CreateRepository;
use rbit_storage::sqlite::{admin::Moderation, wiki_mutations::WikiMutationError};
use rbit_wikis::{
    WikiId,
    policy::{WikiExpected, WikiPolicyError},
};
use std::num::NonZeroU32;

#[tokio::test]
async fn deleted_highest_account_recreation_cannot_replay_or_adopt_its_previous_pending_command() {
    let fixture = Fixture::new().await;
    fixture.services.collaborators.add(&fixture.owner, &fixture.repo, fixture.outsider.name().as_str(), Permission::Write).await.unwrap();
    let store = fixture.database.wiki_mutations();
    let foreign = || {
        let mut command = command(&fixture, 1, "original body");
        command.actor = fixture.outsider.id();
        command
    };
    let first = store.accept(foreign(), Timestamp(1), capacity()).await.unwrap();
    assert!(matches!(fixture.database.admin().delete_user(fixture.outsider.id()).await.unwrap(), Moderation::Applied));
    let recreated = Fixture::user(&fixture.database, "replacement").await;
    assert_eq!(recreated.id(), fixture.outsider.id(), "exercise actual SQLite account-ID reuse");
    fixture.services.collaborators.add(&fixture.owner, &fixture.repo, recreated.name().as_str(), Permission::Write).await.unwrap();
    // Same numeric actor, exact nonce/body/head and fresh write grant still cannot
    // recover the old actor's command. Its retained evidence blocks a new writer.
    assert!(matches!(store.accept(foreign(), Timestamp(2), capacity()).await, Err(WikiMutationError::Policy(WikiPolicyError::Pending))));
    let current: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wiki_actor_incarnations").fetch_one(&fixture.raw).await.unwrap();
    assert_eq!(current, 0, "deletion removed the old binding; refusal did not allocate a new one");
    fixture
        .services
        .repositories
        .create(&fixture.owner, CreateRepository { name: "second".into(), description: String::new(), visibility: Visibility::Private })
        .await
        .unwrap();
    let second = fixture.services.repositories.open(Some(&fixture.owner), "owner", "second").await.unwrap();
    fixture.services.collaborators.add(&fixture.owner, &second, recreated.name().as_str(), Permission::Write).await.unwrap();
    let wiki: i64 = sqlx::query_scalar("INSERT INTO wiki_repositories(repo_id,enabled) VALUES(?,1) RETURNING id")
        .bind(second.repo.repo.id.0)
        .fetch_one(&fixture.raw)
        .await
        .unwrap();
    let fresh = || {
        let mut command = foreign();
        command.expected = WikiExpected::Create { repository: second.repo.repo.id, wiki: WikiId::new(wiki).unwrap(), head: None };
        command
    };
    assert!(
        matches!(store.accept(fresh(), Timestamp(2), capacity()).await, Err(WikiMutationError::Capacity)),
        "orphan pending work still consumes capacity until verified recovery"
    );
    let accepted = store.accept(fresh(), Timestamp(2), NonZeroU32::new(2).unwrap()).await.unwrap();
    assert!(accepted.actor_incarnation.get() > first.actor_incarnation.get());
    assert!(matches!(
        store.accept(foreign(), Timestamp(3), NonZeroU32::new(2).unwrap()).await,
        Err(WikiMutationError::Policy(WikiPolicyError::Pending))
    ));
    let old_binding: i64 = sqlx::query_scalar("SELECT actor_incarnation FROM wiki_mutation_intents WHERE wiki_id=?")
        .bind(fixture.wiki.get())
        .fetch_one(&fixture.raw)
        .await
        .unwrap();
    assert_eq!(old_binding, first.actor_incarnation.get());
}
