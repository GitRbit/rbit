//! Real catalog deletion/recreation cannot adopt an orphaned immutable object scope.
use super::fixture::Fixture;
use rbit_core::{Timestamp, Visibility};
use rbit_git::wikis::PrepareWikiPage;
use rbit_services::CreateRepository;
use rbit_storage::count_work;
use rbit_wikis::{
    WikiContent, WikiId, WikiPageId,
    reading::{WikiCatalog, WikiPageRequest, WikiReadError, WikiReadView, WikiReader},
};

#[tokio::test]
async fn reused_repository_id_gets_a_new_wiki_and_cannot_select_old_objects() {
    let fixture = Fixture::new().await;
    let old_oid = fixture.objects(1, "old private content", None, 1).await;
    fixture.committed(1, "Guide", "Old", 1, old_oid).await;
    let repository = fixture.repo.repo.repo.id;
    let old_root = fixture.directory.path().join(format!("wiki-objects/{}/{}.git", repository.0, fixture.wiki.get()));
    fixture.services.repo_settings.delete(&fixture.owner, &fixture.repo, "owner/wiki-project").await.unwrap();
    assert!(old_root.exists(), "object cleanup is not implemented; the orphan must remain isolated");
    fixture
        .services
        .repositories
        .create(
            &fixture.owner,
            CreateRepository { name: "replacement".into(), description: String::new(), visibility: Visibility::Private },
        )
        .await
        .unwrap();
    let replacement = fixture.services.repositories.open(Some(&fixture.owner), "owner", "replacement").await.unwrap();
    assert_eq!(replacement.repo.repo.id, repository, "fixture exercises actual SQLite repository-ID reuse");
    let new_id: i64 = sqlx::query_scalar("INSERT INTO wiki_repositories(repo_id,enabled) VALUES(?,1) RETURNING id")
        .bind(repository.0)
        .fetch_one(&fixture.raw)
        .await
        .unwrap();
    let wiki = WikiId::new(new_id).unwrap();
    assert!(wiki > fixture.wiki);
    let catalog = fixture.database.wikis();
    let path = "Guide".parse().unwrap();
    let request = WikiPageRequest { repository, viewer: Some(fixture.owner.id()), path: &path };
    let (empty, work) = count_work(WikiReader::page(&catalog, &fixture.objects, request)).await;
    assert!(matches!(empty, Ok(WikiReadView::Empty { wiki: selected, .. }) if selected == wiki));
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    // Deliberately seed an invalid finalized pointer in the closed test catalog.
    // A caller-supplied old OID cannot make the native adapter fall back to its orphan.
    fixture.committed(2, "Guide", "New", 1, old_oid).await;
    let selected = catalog.page(request).await.unwrap().unwrap().page.unwrap();
    assert_eq!(selected.wiki, wiki);
    let (refused, work) = count_work(WikiReader::page(&catalog, &fixture.objects, request)).await;
    assert!(matches!(refused, Err(WikiReadError::Revision(_))));
    assert_eq!((work.acquisitions, work.statements), (1, 2));
    let prepared = fixture
        .objects
        .prepare(PrepareWikiPage {
            repository,
            wiki,
            page: WikiPageId::new(2).unwrap(),
            parent: None,
            content: WikiContent::try_from("replacement content".to_owned()).unwrap(),
            actor: fixture.owner.id(),
            at: Timestamp(2),
        })
        .await
        .unwrap();
    fixture.committed(2, "Guide", "New", 2, prepared).await;
    assert!(matches!(WikiReader::page(&catalog, &fixture.objects, request).await,
        Ok(WikiReadView::Page { page, content }) if page.wiki == wiki && content.as_str() == "replacement content"));
    assert!(old_root.exists());
}
