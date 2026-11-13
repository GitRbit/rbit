//! Tombstones, restoring deleted pages, rename aliases, and the wiki following its
//! repository through a transfer and a deletion.
use super::{Fixture, input, page, path};
use rbit_core::Visibility;
use rbit_services::{
    orgs::CreateOrganization,
    wikis::{
        errors::WikiError,
        pages::{WikiLookup, WikiPageState},
    },
};
use rbit_wikis::change::WikiChange;

#[tokio::test]
async fn unicode17_titles_paths_history_and_aliases_survive_dependency_updates() {
    let fixture = Fixture::new(Visibility::Public).await;
    let wikis = &fixture.services.wikis;
    let repo = fixture.repo(&fixture.writer).await;
    let title = "a\u{1ade}\u{05ae}b";
    let distinct = "a\u{05ae}\u{1ade}b";
    let original = "Caf\u{00e9}/One";
    let renamed = "Caf\u{00e9}/Moved";
    let created = wikis.create(&repo, &fixture.writer, &input(title, original, "legacy bytes")).await.unwrap();
    let first = page(wikis.page(&repo, None, &created).await.unwrap());
    assert_eq!(first.page.title.as_str().as_bytes(), title.as_bytes());
    let other = wikis.create(&repo, &fixture.writer, &input(distinct, "Caf\u{00e9}/Two", "distinct")).await.unwrap();
    let other = page(wikis.page(&repo, None, &other).await.unwrap());
    assert_ne!(first.page.id, other.page.id);
    assert_eq!(other.page.title.as_str().as_bytes(), distinct.as_bytes());
    wikis.edit(&repo, &fixture.writer, first.page.id, first.page.head.generation, &input(title, renamed, "legacy bytes")).await.unwrap();
    assert!(matches!(wikis.page(&repo, None, &path(original)).await.unwrap(), WikiLookup::Moved(to) if to.as_str() == renamed));
    let current = page(wikis.page(&repo, None, &path(renamed)).await.unwrap());
    assert_eq!((current.page.id, current.page.title.as_str()), (first.page.id, title));
    let history = wikis.history(&repo, None, first.page.id, None).await.unwrap();
    assert!(history.entries.iter().all(|entry| entry.title.as_str().as_bytes() == title.as_bytes()));
    let historical = wikis.revision(&repo, None, first.page.id, first.page.head.generation).await.unwrap();
    assert_eq!((historical.page.path.as_str(), historical.page.title.as_str()), (original, title));
    wikis.delete(&repo, &fixture.writer, first.page.id, current.page.head.generation).await.unwrap();
    let deleted = wikis.deleted(&repo, None, None).await.unwrap();
    assert_eq!((deleted.entries[0].path.as_str(), deleted.entries[0].title.as_str()), (renamed, title));
    let restored = wikis.undelete(&repo, &fixture.writer, first.page.id, deleted.entries[0].deleted).await.unwrap();
    let restored = page(wikis.page(&repo, None, &restored).await.unwrap());
    assert_eq!((restored.page.id, restored.page.path.as_str(), restored.page.title.as_str()), (first.page.id, renamed, title));
}

#[tokio::test]
async fn a_deleted_page_keeps_its_history_and_can_be_restored_by_writers() {
    let fixture = Fixture::new(Visibility::Public).await;
    let wikis = &fixture.services.wikis;
    let repo = fixture.repo(&fixture.writer).await;
    let created = wikis.create(&repo, &fixture.writer, &input("Notes", "", "first")).await.unwrap();
    let notes = page(wikis.page(&repo, None, &created).await.unwrap());
    wikis.edit(&repo, &fixture.writer, notes.page.id, notes.page.head.generation, &input("Notes", "Notes", "second")).await.unwrap();
    let head = page(wikis.page(&repo, None, &created).await.unwrap()).page.head.generation;
    wikis.delete(&repo, &fixture.writer, notes.page.id, head).await.unwrap();

    let history = wikis.history(&repo, None, notes.page.id, None).await.unwrap();
    let kinds: Vec<_> = history.entries.iter().map(|entry| entry.change).collect();
    assert_eq!(kinds, [Some(WikiChange::Delete), Some(WikiChange::Edit), Some(WikiChange::Create)]);
    let deleted_at = history.entries[0].head.generation;
    assert!(matches!(wikis.revision(&repo, None, notes.page.id, deleted_at).await, Err(WikiError::NotFound)), "no content");
    assert_eq!(wikis.revision(&repo, None, notes.page.id, head).await.unwrap().content.as_str(), "second");
    assert!(matches!(wikis.page_state(&repo, None, notes.page.id).await.unwrap(), WikiPageState::Deleted(_)));
    let listed = wikis.deleted(&repo, None, None).await.unwrap();
    assert_eq!(listed.entries.iter().map(|entry| (entry.path.as_str(), entry.deleted)).collect::<Vec<_>>(), [("Notes", deleted_at)]);
    assert_eq!(listed.entries[0].deleted_by.as_deref(), Some("writer"));

    let reader = fixture.repo(&fixture.reader).await;
    assert!(matches!(wikis.undelete(&reader, &fixture.reader, notes.page.id, deleted_at).await, Err(WikiError::Forbidden)));
    assert!(matches!(wikis.undelete(&repo, &fixture.writer, notes.page.id, head).await, Err(WikiError::Conflict)), "stale");
    // Another page took the path meanwhile: refused until it moves.
    let other = wikis.create(&repo, &fixture.writer, &input("Notes", "", "other")).await.unwrap();
    assert!(matches!(wikis.undelete(&repo, &fixture.writer, notes.page.id, deleted_at).await, Err(WikiError::PathTaken)));
    let other = page(wikis.page(&repo, None, &other).await.unwrap());
    wikis.edit(&repo, &fixture.writer, other.page.id, other.page.head.generation, &input("Other", "Other", "other")).await.unwrap();

    let restored = wikis.undelete(&repo, &fixture.writer, notes.page.id, deleted_at).await.unwrap();
    let back = page(wikis.page(&repo, None, &restored).await.unwrap());
    assert_eq!((back.page.id, back.page.path.as_str(), back.content.as_str()), (notes.page.id, "Notes", "second"));
    assert_eq!(back.page.created_at, notes.page.created_at, "the same page comes back");
    assert!(wikis.deleted(&repo, None, None).await.unwrap().entries.is_empty());
    let undeleted = back.page.head.generation;
    assert!(matches!(wikis.undelete(&repo, &fixture.writer, notes.page.id, undeleted).await, Err(WikiError::NotFound)), "twice");
    let history = wikis.history(&repo, None, notes.page.id, None).await.unwrap();
    assert_eq!(history.entries.len(), 4);
    assert_eq!(history.entries[0].change, Some(WikiChange::Restore));
    assert_eq!(wikis.previous(&repo, None, notes.page.id, undeleted).await.unwrap(), Some(head), "the deletion is skipped");
}

#[tokio::test]
async fn renamed_pages_leave_aliases_that_new_pages_can_reclaim() {
    let fixture = Fixture::new(Visibility::Public).await;
    let wikis = &fixture.services.wikis;
    let repo = fixture.repo(&fixture.writer).await;
    let created = wikis.create(&repo, &fixture.writer, &input("Setup", "", "steps")).await.unwrap();
    let setup = page(wikis.page(&repo, None, &created).await.unwrap());
    let base = setup.page.head.generation;
    wikis.edit(&repo, &fixture.writer, setup.page.id, base, &input("Setup", "Guide/Setup", "steps")).await.unwrap();
    let moved = |lookup: WikiLookup| match lookup {
        WikiLookup::Moved(to) => to.to_string(),
        other => panic!("not moved: {other:?}"),
    };
    assert_eq!(moved(wikis.page(&repo, None, &path("Setup")).await.unwrap()), "Guide/Setup");

    // Renamed again: both old paths lead to the current one, never through each other.
    let base = page(wikis.page(&repo, None, &path("Guide/Setup")).await.unwrap()).page.head.generation;
    wikis.edit(&repo, &fixture.writer, setup.page.id, base, &input("Setup", "Install", "steps")).await.unwrap();
    assert_eq!(moved(wikis.page(&repo, None, &path("Setup")).await.unwrap()), "Install");
    assert_eq!(moved(wikis.page(&repo, None, &path("Guide/Setup")).await.unwrap()), "Install");

    // Moving back onto an old path drops that alias: the page itself answers.
    let base = page(wikis.page(&repo, None, &path("Install")).await.unwrap()).page.head.generation;
    wikis.edit(&repo, &fixture.writer, setup.page.id, base, &input("Setup", "Setup", "steps")).await.unwrap();
    assert_eq!(page(wikis.page(&repo, None, &path("Setup")).await.unwrap()).page.id, setup.page.id);
    assert_eq!(moved(wikis.page(&repo, None, &path("Install")).await.unwrap()), "Setup");

    // A new page reclaims an alias; a deleted page's aliases lead nowhere.
    wikis.create(&repo, &fixture.writer, &input("Install", "", "new")).await.unwrap();
    assert_eq!(page(wikis.page(&repo, None, &path("Install")).await.unwrap()).content.as_str(), "new");
    let head = page(wikis.page(&repo, None, &path("Install")).await.unwrap()).page.head.generation;
    wikis.delete(&repo, &fixture.writer, setup.page.id, head).await.unwrap();
    assert!(matches!(wikis.page(&repo, None, &path("Guide/Setup")).await.unwrap(), WikiLookup::Missing { empty: false }));
}

#[tokio::test]
async fn deleted_pages_and_aliases_are_hidden_from_people_who_cannot_read_the_wiki() {
    let fixture = Fixture::new(Visibility::Private).await;
    let wikis = &fixture.services.wikis;
    let repo = fixture.repo(&fixture.writer).await;
    let created = wikis.create(&repo, &fixture.writer, &input("Secret", "", "x")).await.unwrap();
    let secret = page(wikis.page(&repo, Some(&fixture.writer), &created).await.unwrap());
    let base = secret.page.head.generation;
    wikis.edit(&repo, &fixture.writer, secret.page.id, base, &input("Secret", "Hidden", "x")).await.unwrap();
    let base = page(wikis.page(&repo, Some(&fixture.writer), &path("Hidden")).await.unwrap()).page.head.generation;
    wikis.delete(&repo, &fixture.writer, secret.page.id, base).await.unwrap();
    for viewer in [None, Some(&fixture.outsider)] {
        assert!(matches!(wikis.deleted(&repo, viewer, None).await, Err(WikiError::NotFound)));
        assert!(matches!(wikis.deleted_page(&repo, viewer, secret.page.id).await, Err(WikiError::NotFound)));
        assert!(matches!(wikis.history(&repo, viewer, secret.page.id, None).await, Err(WikiError::NotFound)));
        assert!(matches!(wikis.page(&repo, viewer, &path("Secret")).await, Err(WikiError::NotFound)));
    }
    assert_eq!(wikis.deleted(&repo, Some(&fixture.reader), None).await.unwrap().entries.len(), 1);
}

#[tokio::test]
async fn the_wiki_follows_a_transferred_repository_and_goes_with_a_deleted_one() {
    let fixture = Fixture::new(Visibility::Private).await;
    let wikis = &fixture.services.wikis;
    let repo = fixture.repo(&fixture.owner).await;
    let created = wikis.create(&repo, &fixture.owner, &input("Home", "", "kept")).await.unwrap();
    let id = repo.repo.repo.id;
    let store = wikis.root().join(id.0.to_string());
    assert!(store.is_dir(), "the objects live under the stable repository ID");

    let org = CreateOrganization { name: "acme".into(), ..CreateOrganization::default() };
    fixture.services.orgs.create(&fixture.owner, org).await.unwrap();
    fixture.services.transfers.transfer(&fixture.owner, &repo, "acme", "owner/handbook").await.unwrap();
    let moved = fixture.services.repositories.open(Some(&fixture.owner), "acme", "handbook").await.unwrap();
    assert_eq!(moved.repo.repo.id, id);
    assert_eq!(page(wikis.page(&moved, Some(&fixture.owner), &created).await.unwrap()).content.as_str(), "kept");
    assert!(matches!(wikis.page(&moved, Some(&fixture.outsider), &created).await, Err(WikiError::NotFound)));
    assert_eq!(wikis.remove_repository_objects(id).await.unwrap(), 0, "a live wiki is never removed");

    fixture.services.repo_settings.delete(&fixture.owner, &moved, "acme/handbook").await.unwrap();
    assert_eq!(wikis.remove_repository_objects(id).await.unwrap(), 1);
    assert!(!store.exists(), "the deleted repository's wiki objects are gone");
    assert_eq!(wikis.remove_repository_objects(id).await.unwrap(), 0, "removal is idempotent");
}
