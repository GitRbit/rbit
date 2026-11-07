//! Production catalog + native object adapters through the feature read application.
mod acceptance;
mod actor_incarnation;
mod fixture;
mod incarnation;
mod ownership;

use fixture::Fixture;
use rbit_core::{Permission, Timestamp};
use rbit_storage::count_work;
use rbit_wikis::{
    WikiGeneration, WikiPageId,
    reading::{
        WikiCatalog, WikiHistoryLimit, WikiHistoryRequest, WikiPageRequest, WikiReadError, WikiReadView, WikiReader, WikiRevisionRequest,
    },
};

#[tokio::test]
async fn enabled_empty_and_denied_private_reads_skip_git_even_when_work_is_closed() {
    let fixture = Fixture::new().await;
    let unselected = fixture.objects(1, "not published", None, 1).await;
    std::fs::write(
        fixture.directory.path().join("wiki-objects").join(format!("{}/{}.git/HEAD", fixture.repo.repo.repo.id.0, fixture.wiki.get())),
        format!("{unselected}\n"),
    )
    .unwrap();
    fixture.work.close();
    let catalog = fixture.database.wikis();
    let path = "Guide".parse().unwrap();
    let request = WikiPageRequest { repository: fixture.repo.repo.repo.id, viewer: Some(fixture.owner.id()), path: &path };
    let (empty, work) = count_work(WikiReader::page(&catalog, &fixture.objects, request)).await;
    assert!(matches!(empty, Ok(WikiReadView::Empty { .. })));
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    for viewer in [None, Some(fixture.outsider.id())] {
        let (hidden, work) = count_work(WikiReader::page(&catalog, &fixture.objects, WikiPageRequest { viewer, ..request })).await;
        assert!(matches!(hidden, Err(WikiReadError::NotFound)));
        assert_eq!((work.acquisitions, work.statements), (1, 1));
    }
}

#[tokio::test]
async fn finalized_and_historical_reads_use_catalog_oids_and_bounded_history() {
    let fixture = Fixture::new().await;
    let first = fixture.objects(1, "old <script>source text</script>", None, 1).await;
    fixture.committed(1, "Guide", "Original", 1, first).await;
    let second = fixture.objects(1, "new", Some(first), 2).await;
    fixture.committed(1, "Renamed", "Renamed", 2, second).await;
    let catalog = fixture.database.wikis();
    let path = "Renamed".parse().unwrap();
    let request = WikiPageRequest { repository: fixture.repo.repo.repo.id, viewer: Some(fixture.owner.id()), path: &path };
    let (selected, work) = count_work(WikiReader::page(&catalog, &fixture.objects, request)).await;
    assert!(matches!(selected, Ok(WikiReadView::Page { page, content }) if page.head.revision == second && content.as_str() == "new"));
    assert_eq!((work.acquisitions, work.statements), (1, 2));
    let history = WikiHistoryRequest {
        repository: request.repository,
        viewer: request.viewer,
        page: WikiPageId::new(1).unwrap(),
        before: None,
        limit: WikiHistoryLimit::new(1).unwrap(),
    };
    let (first_page, work) = count_work(WikiReader::history(&catalog, history)).await;
    let first_page = first_page.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 3));
    assert_eq!(first_page.entries.len(), 1);
    assert_eq!(first_page.entries[0].head.revision, second);
    assert_eq!(first_page.next.unwrap().get(), 2);
    let older = WikiReader::history(&catalog, WikiHistoryRequest { before: first_page.next, ..history }).await.unwrap();
    assert_eq!(older.entries.len(), 1);
    assert_eq!(older.entries[0].head.revision, first);
    assert!(older.next.is_none());
    let restored_view = WikiReader::revision(
        &catalog,
        &fixture.objects,
        WikiRevisionRequest {
            repository: request.repository,
            viewer: request.viewer,
            page: history.page,
            generation: WikiGeneration::INITIAL,
        },
    )
    .await;
    assert!(
        matches!(restored_view, Ok(WikiReadView::Page { page, content }) if page.path.as_str() == "Guide" && content.as_str() == "old <script>source text</script>")
    );
    let (future, work) = count_work(WikiReader::revision(
        &catalog,
        &fixture.objects,
        WikiRevisionRequest {
            repository: request.repository,
            viewer: request.viewer,
            page: history.page,
            generation: WikiGeneration::new(3).unwrap(),
        },
    ))
    .await;
    assert!(matches!(future, Err(WikiReadError::NotFound)));
    assert_eq!((work.acquisitions, work.statements), (1, 1), "future generation never reads page metadata or Git");
}

#[tokio::test]
async fn acl_revocation_disabled_and_suspended_viewers_are_rechecked_before_objects() {
    let fixture = Fixture::new().await;
    let revision = fixture.objects(1, "private", None, 1).await;
    fixture.committed(1, "Guide", "Guide", 1, revision).await;
    fixture.services.collaborators.add(&fixture.owner, &fixture.repo, fixture.outsider.name().as_str(), Permission::Read).await.unwrap();
    let catalog = fixture.database.wikis();
    let path = "Guide".parse().unwrap();
    let request = WikiPageRequest { repository: fixture.repo.repo.repo.id, viewer: Some(fixture.outsider.id()), path: &path };
    assert!(matches!(WikiReader::page(&catalog, &fixture.objects, request).await, Ok(WikiReadView::Page { .. })));
    fixture.services.collaborators.remove(&fixture.owner, &fixture.repo, fixture.outsider.name().as_str()).await.unwrap();
    fixture.work.close();
    assert!(matches!(WikiReader::page(&catalog, &fixture.objects, request).await, Err(WikiReadError::NotFound)));
    sqlx::query("UPDATE user_logins SET suspended_at=1 WHERE account_id=?").bind(fixture.owner.id().0).execute(&fixture.raw).await.unwrap();
    assert!(matches!(
        WikiReader::page(&catalog, &fixture.objects, WikiPageRequest { viewer: Some(fixture.owner.id()), ..request }).await,
        Err(WikiReadError::NotFound)
    ));
    sqlx::query("UPDATE wiki_repositories SET enabled=0 WHERE repo_id=?").bind(request.repository.0).execute(&fixture.raw).await.unwrap();
    assert!(
        catalog
            .history(WikiHistoryRequest {
                repository: request.repository,
                viewer: request.viewer,
                page: WikiPageId::new(1).unwrap(),
                before: None,
                limit: WikiHistoryLimit::new(50).unwrap()
            })
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn archived_reads_remain_available_and_noncanonical_stored_metadata_fails_closed() {
    let fixture = Fixture::new().await;
    let revision = fixture.objects(1, "body", None, 1).await;
    fixture.committed(1, "Guide", "Guide", 1, revision).await;
    fixture.database.repo_settings().archive(fixture.repo.repo.repo.id, fixture.owner.id(), Timestamp(2)).await.unwrap();
    let catalog = fixture.database.wikis();
    let path = "Guide".parse().unwrap();
    let request = WikiPageRequest { repository: fixture.repo.repo.repo.id, viewer: Some(fixture.owner.id()), path: &path };
    assert!(matches!(WikiReader::page(&catalog, &fixture.objects, request).await, Ok(WikiReadView::Page { .. })));
    sqlx::query("UPDATE wiki_pages SET title=' noncanonical ' WHERE repo_id=?")
        .bind(request.repository.0)
        .execute(&fixture.raw)
        .await
        .unwrap();
    fixture.work.close();
    assert!(matches!(WikiReader::page(&catalog, &fixture.objects, request).await, Err(WikiReadError::Catalog(_))));
    assert!(fixture.directory.path().join("wiki-objects").is_dir());
}

#[tokio::test]
async fn exhausted_generation_remains_readable_and_history_cursor_does_not_omit_it() {
    let fixture = Fixture::new().await;
    let revision = fixture.objects(1, "last accepted generation", None, 1).await;
    fixture.committed(1, "Guide", "Guide", i64::MAX, revision).await;
    let catalog = fixture.database.wikis();
    let request = WikiHistoryRequest {
        repository: fixture.repo.repo.repo.id,
        viewer: Some(fixture.owner.id()),
        page: WikiPageId::new(1).unwrap(),
        before: None,
        limit: WikiHistoryLimit::new(50).unwrap(),
    };
    let newest = WikiReader::history(&catalog, request).await.unwrap();
    assert_eq!(newest.entries.len(), 1);
    assert_eq!(newest.entries[0].head.generation.get(), i64::MAX);
    let older = WikiReader::history(&catalog, WikiHistoryRequest { before: WikiGeneration::new(i64::MAX), ..request }).await.unwrap();
    assert!(matches!(older.entries.as_slice(), []));
    assert!(older.next.is_none());
}
