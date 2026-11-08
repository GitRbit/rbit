//! Wiki permissions follow repository access, rechecked when a change is published.
use super::{Fixture, input, page, path};
use rbit_core::{Permission, Visibility};
use rbit_services::wikis::{errors::WikiError, pages::WikiStatus};

#[tokio::test]
async fn readers_read_writers_write_and_public_wikis_are_readable_anonymously() {
    let fixture = Fixture::new(Visibility::Public).await;
    let wikis = &fixture.services.wikis;
    let writer_repo = fixture.repo(&fixture.writer).await;
    let created = wikis.create(&writer_repo, &fixture.writer, &input("Home", "", "public text")).await.unwrap();
    for viewer in [None, Some(&fixture.outsider), Some(&fixture.reader)] {
        assert_eq!(page(wikis.page(&writer_repo, viewer, &created).await.unwrap()).content.as_str(), "public text");
    }
    for user in [&fixture.reader, &fixture.outsider] {
        let repo = fixture.repo(user).await;
        assert!(matches!(wikis.create(&repo, user, &input("Mine", "", "x")).await, Err(WikiError::Forbidden)));
        let home = page(wikis.page(&repo, Some(user), &created).await.unwrap());
        let edit = wikis.edit(&repo, user, home.page.id, home.page.head.generation, &input("Home", "Home", "defaced")).await;
        assert!(matches!(edit, Err(WikiError::Forbidden)));
        assert!(matches!(wikis.delete(&repo, user, home.page.id, home.page.head.generation).await, Err(WikiError::Forbidden)));
        assert!(matches!(wikis.set_enabled(&repo, false).await, Err(WikiError::Forbidden)));
    }
    assert!(matches!(wikis.set_enabled(&writer_repo, false).await, Err(WikiError::Forbidden)), "writers do not administer");
}

#[tokio::test]
async fn private_wikis_hide_everything_from_people_without_access() {
    let fixture = Fixture::new(Visibility::Private).await;
    let wikis = &fixture.services.wikis;
    let writer_repo = fixture.repo(&fixture.writer).await;
    let created = wikis.create(&writer_repo, &fixture.writer, &input("Secret", "", "private text")).await.unwrap();
    let secret = page(wikis.page(&writer_repo, Some(&fixture.writer), &created).await.unwrap());
    assert!(fixture.services.repositories.open(Some(&fixture.outsider), "owner", "handbook").await.is_err());
    // Even with a repository context obtained by someone else, the catalog
    // authorizes the actual viewer in SQL before any object is read.
    for viewer in [None, Some(&fixture.outsider)] {
        assert!(matches!(wikis.page(&writer_repo, viewer, &created).await, Err(WikiError::NotFound)));
        assert!(matches!(wikis.history(&writer_repo, viewer, secret.page.id, None).await, Err(WikiError::NotFound)));
        assert!(matches!(wikis.pages(&writer_repo, viewer, None).await, Err(WikiError::NotFound)));
        assert!(matches!(wikis.search(&writer_repo, viewer, "Secret").await, Err(WikiError::NotFound)));
    }
}

#[tokio::test]
async fn revoked_access_and_archiving_defeat_a_change_prepared_with_older_access() {
    let fixture = Fixture::new(Visibility::Private).await;
    let wikis = &fixture.services.wikis;
    let stale = fixture.repo(&fixture.writer).await;
    let created = wikis.create(&stale, &fixture.writer, &input("Home", "", "v1")).await.unwrap();
    let home = page(wikis.page(&stale, Some(&fixture.writer), &created).await.unwrap());

    let owner_repo = fixture.repo(&fixture.owner).await;
    fixture.services.collaborators.change(&fixture.owner, &owner_repo, "writer", Permission::Read).await.unwrap();
    let downgraded = wikis.edit(&stale, &fixture.writer, home.page.id, home.page.head.generation, &input("Home", "Home", "v2")).await;
    assert!(matches!(downgraded, Err(WikiError::Forbidden)), "{downgraded:?}");
    fixture.services.collaborators.remove(&fixture.owner, &owner_repo, "writer").await.unwrap();
    let removed = wikis.edit(&stale, &fixture.writer, home.page.id, home.page.head.generation, &input("Home", "Home", "v2")).await;
    assert!(matches!(removed, Err(WikiError::NotFound)), "{removed:?}");

    let owner_stale = fixture.repo(&fixture.owner).await;
    fixture.archive().await;
    let archived = wikis.edit(&owner_stale, &fixture.owner, home.page.id, home.page.head.generation, &input("Home", "Home", "v2")).await;
    assert!(matches!(archived, Err(WikiError::Archived)), "{archived:?}");
    let fresh = fixture.repo(&fixture.owner).await;
    assert!(matches!(wikis.create(&fresh, &fixture.owner, &input("New", "", "x")).await, Err(WikiError::Archived)));
    assert!(matches!(wikis.set_enabled(&fresh, false).await, Err(WikiError::Archived)));
    assert_eq!(
        page(wikis.page(&fresh, Some(&fixture.reader), &path("Home")).await.unwrap()).content.as_str(),
        "v1",
        "archived wikis stay readable"
    );
}

#[tokio::test]
async fn a_turned_off_wiki_reads_as_missing_until_an_administrator_turns_it_on() {
    let fixture = Fixture::new(Visibility::Public).await;
    let wikis = &fixture.services.wikis;
    let owner_repo = fixture.repo(&fixture.owner).await;
    let created = wikis.create(&owner_repo, &fixture.owner, &input("Home", "", "text")).await.unwrap();
    wikis.set_enabled(&owner_repo, false).await.unwrap();
    assert_eq!(wikis.status(&owner_repo).await.unwrap(), WikiStatus::Off);
    for viewer in [None, Some(&fixture.owner)] {
        assert!(matches!(wikis.page(&owner_repo, viewer, &created).await, Err(WikiError::NotFound)));
    }
    let writer_repo = fixture.repo(&fixture.writer).await;
    assert!(matches!(wikis.create(&writer_repo, &fixture.writer, &input("Other", "", "x")).await, Err(WikiError::NotFound)));
    wikis.set_enabled(&owner_repo, true).await.unwrap();
    assert_eq!(
        page(wikis.page(&owner_repo, None, &created).await.unwrap()).content.as_str(),
        "text",
        "turning it on again keeps the pages"
    );
}
