//! Release and attached-file persistence against a real, migrated `SQLite` database.

use rbit_core::{
    RepoId, Timestamp, User, Visibility,
    release::{ReleaseId, ReleaseSource},
};
use rbit_storage::{
    Database, NewRepository, NewUser, StorageError,
    sqlite::releases::{NewAsset, NewRelease, ReleaseChanges},
};

async fn setup() -> (tempfile::TempDir, Database, User, RepoId) {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let user = database
        .accounts()
        .create_user(NewUser {
            name: "alice".parse().unwrap(),
            email: "alice@example.com".parse().unwrap(),
            display_name: None,
            password_hash: "not-a-real-hash".to_owned(),
            is_admin: false,
        })
        .await
        .unwrap();
    let repo = database
        .repositories()
        .create(NewRepository {
            owner_id: user.id(),
            name: "demo".parse().unwrap(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".to_owned(),
        })
        .await
        .unwrap();
    (dir, database, user, repo.id)
}

fn release<'a>(repo: RepoId, user: &User, tag: &'a str, is_draft: bool, is_prerelease: bool) -> NewRelease<'a> {
    NewRelease {
        repo_id: repo,
        tag_name: tag,
        name: tag,
        notes: "notes",
        is_draft,
        is_prerelease,
        author_id: Some(user.id()),
        source: ReleaseSource::User,
    }
}

#[tokio::test]
async fn releases_are_unique_per_tag_and_listed_newest_first() {
    let (_dir, database, alice, repo) = setup().await;
    let store = database.releases();
    let first = store.create(release(repo, &alice, "v1", false, false)).await.unwrap();
    let draft = store.create(release(repo, &alice, "v3", true, false)).await.unwrap();
    let second = store.create(release(repo, &alice, "v2", false, true)).await.unwrap();
    assert!(matches!(store.create(release(repo, &alice, "v1", false, false)).await, Err(StorageError::Conflict(_))));

    let stored = store.find(repo, first).await.unwrap().unwrap();
    assert_eq!(stored.author.as_ref().map(|a| a.name.as_str()), Some("alice"));
    assert!(stored.published_at.is_some(), "published on creation");
    assert!(store.find(repo, draft).await.unwrap().unwrap().published_at.is_none());
    assert!(store.find(RepoId(999), first).await.unwrap().is_none(), "scoped to the repository");

    let public: Vec<_> = store.list(repo, false, 10, 0).await.unwrap().into_iter().map(|r| r.id).collect();
    assert_eq!(public, [second, first], "drafts hidden, newest first (same second: higher id first)");
    let all: Vec<_> = store.list(repo, true, 10, 0).await.unwrap().into_iter().map(|r| r.id).collect();
    assert_eq!(all, [draft, second, first], "drafts lead for writers");
    assert_eq!((store.count(repo, false).await.unwrap(), store.count(repo, true).await.unwrap()), (2, 3));
    assert_eq!(store.list(repo, true, 1, 1).await.unwrap()[0].id, second);

    assert_eq!(store.latest(repo).await.unwrap().map(|r| r.id), Some(first), "full releases win over pre-releases");
    assert_eq!(store.find_by_tag(repo, "v2").await.unwrap().map(|r| r.id), Some(second));
}

#[tokio::test]
async fn publishing_a_draft_sets_the_date_once() {
    let (_dir, database, alice, repo) = setup().await;
    let store = database.releases();
    let id = store.create(release(repo, &alice, "v1", true, false)).await.unwrap();
    let changes = |is_draft, published_at| ReleaseChanges {
        tag_name: "v1",
        name: "Version 1",
        notes: "new notes",
        is_draft,
        is_prerelease: false,
        published_at,
    };
    store.update(id, changes(false, Some(Timestamp(1_000)))).await.unwrap();
    let published = store.find(repo, id).await.unwrap().unwrap();
    assert_eq!(
        (published.name.as_str(), published.notes.as_str(), published.published_at),
        ("Version 1", "new notes", Some(Timestamp(1_000)))
    );

    store.update(id, changes(false, Some(Timestamp(2_000)))).await.unwrap();
    assert_eq!(store.find(repo, id).await.unwrap().unwrap().published_at, Some(Timestamp(1_000)), "kept");

    let other = store.create(release(repo, &alice, "v2", false, false)).await.unwrap();
    let clash = ReleaseChanges { tag_name: "v1", ..changes(false, None) };
    assert!(matches!(store.update(other, clash).await, Err(StorageError::Conflict(_))));
}

#[tokio::test]
async fn assets_are_unique_per_release_and_cascade() {
    let (_dir, database, alice, repo) = setup().await;
    let store = database.releases();
    let one = store.create(release(repo, &alice, "v1", false, false)).await.unwrap();
    let two = store.create(release(repo, &alice, "v2", false, false)).await.unwrap();
    let asset = |release: ReleaseId, name: &'static str| NewAsset {
        release_id: release,
        repo_id: repo,
        name,
        size: 3,
        content_type: "application/zip",
        sha256: [7; 32],
        uploader_id: Some(alice.id()),
        source: ReleaseSource::User,
    };
    let stored = store.insert_asset(asset(one, "b.zip")).await.unwrap();
    store.insert_asset(asset(one, "a.zip")).await.unwrap();
    store.insert_asset(asset(two, "b.zip")).await.unwrap();
    assert!(matches!(store.insert_asset(asset(one, "b.zip")).await, Err(StorageError::Conflict(_))));
    assert_eq!((stored.size, stored.sha256, stored.download_count), (3, [7; 32], 0));

    let names: Vec<_> = store.assets(one).await.unwrap().into_iter().map(|a| a.name).collect();
    assert_eq!(names, ["a.zip", "b.zip"]);
    let grouped = store.assets_of(&[one, two]).await.unwrap();
    assert_eq!((grouped[&one].len(), grouped[&two].len()), (2, 1));
    assert_eq!(store.count_assets(one).await.unwrap(), 2);

    store.count_download(stored.id).await.unwrap();
    assert_eq!(store.find_asset(one, "b.zip").await.unwrap().unwrap().download_count, 1);
    assert_eq!(store.find_asset_by_id(repo, stored.id).await.unwrap().map(|a| a.name).as_deref(), Some("b.zip"));
    assert!(store.find_asset_by_id(RepoId(repo.0 + 1), stored.id).await.unwrap().is_none(), "only in its repository");
    store.delete_asset(stored.id).await.unwrap();
    assert!(store.find_asset(one, "b.zip").await.unwrap().is_none());

    store.delete(two).await.unwrap();
    assert!(store.assets(two).await.unwrap().is_empty(), "asset rows go with their release");
}

#[tokio::test]
async fn concurrent_asset_uploads_cannot_exceed_the_release_limit() {
    let (_dir, database, alice, repo) = setup().await;
    let store = database.releases();
    let id = store.create(release(repo, &alice, "v1", false, false)).await.unwrap();
    let asset = |name| NewAsset {
        release_id: id,
        repo_id: repo,
        name,
        size: 3,
        content_type: "application/zip",
        sha256: [7; 32],
        uploader_id: Some(alice.id()),
        source: ReleaseSource::User,
    };
    assert!(store.insert_asset_limited(asset("disabled.zip"), 0).await.unwrap().is_none());
    let (first, second) =
        tokio::join!(store.insert_asset_limited(asset("first.zip"), 1), store.insert_asset_limited(asset("second.zip"), 1),);
    assert_ne!(first.unwrap().is_some(), second.unwrap().is_some(), "exactly one upload claims the final slot");
    assert_eq!(store.count_assets(id).await.unwrap(), 1);
}
