//! Whole-wiki commits that replace, remove or keep one page of the flat ID tree.
use rbit_core::{AccountId, RepoId, Timestamp};
use rbit_git::wikis::{
    WikiObjectError, WikiObjects,
    changes::{PrepareWikiChange, WikiTreeChange},
};
use rbit_kernel::BlockingBudget;
use rbit_wikis::{
    WikiContent, WikiGeneration, WikiId, WikiPage, WikiPageId, WikiRevisionId, WikiStoreId, policy::WikiHead, reading::WikiRevisionStore,
};
use std::num::NonZeroUsize;

fn store() -> WikiStoreId {
    WikiStoreId::try_from([3; 16]).unwrap()
}

fn change(page: i64, parent: Option<WikiRevisionId>, change: WikiTreeChange) -> PrepareWikiChange {
    PrepareWikiChange {
        repository: RepoId(5),
        wiki: WikiId::new(2).unwrap(),
        page: WikiPageId::new(page).unwrap(),
        parent,
        change,
        actor: AccountId(9),
        at: Timestamp(50),
    }
}

fn upsert(text: &str) -> WikiTreeChange {
    WikiTreeChange::Upsert(WikiContent::try_from(text.to_owned()).unwrap())
}

fn page(page: i64, revision: WikiRevisionId) -> WikiPage {
    WikiPage {
        repository: RepoId(5),
        wiki: WikiId::new(2).unwrap(),
        id: WikiPageId::new(page).unwrap(),
        path: "Page".parse().unwrap(),
        title: "Page".parse().unwrap(),
        head: WikiHead { generation: WikiGeneration::INITIAL, revision },
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
    }
}

#[tokio::test]
async fn removal_and_rename_commits_keep_other_pages_and_old_snapshots_readable() {
    let directory = tempfile::tempdir().unwrap();
    let objects = WikiObjects::open(directory.path(), store(), BlockingBudget::new(NonZeroUsize::new(1).unwrap())).unwrap();
    let first = objects.prepare_change(change(1, None, upsert("one"))).await.unwrap();
    let second = objects.prepare_change(change(2, Some(first), upsert("two"))).await.unwrap();
    let renamed = objects.prepare_change(change(1, Some(second), WikiTreeChange::Keep)).await.unwrap();
    assert_ne!(renamed, second, "a metadata-only change is still a new commit");
    assert_eq!(objects.read_page(store(), &page(1, renamed)).await.unwrap().as_str(), "one");
    let removed = objects.prepare_change(change(1, Some(renamed), WikiTreeChange::Remove)).await.unwrap();
    assert!(matches!(objects.read_page(store(), &page(1, removed)).await, Err(WikiObjectError::Format)));
    assert_eq!(objects.read_page(store(), &page(2, removed)).await.unwrap().as_str(), "two");
    assert_eq!(objects.read_page(store(), &page(1, second)).await.unwrap().as_str(), "one");
}

#[tokio::test]
async fn removing_or_keeping_a_page_absent_from_the_parent_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let objects = WikiObjects::open(directory.path(), store(), BlockingBudget::new(NonZeroUsize::new(1).unwrap())).unwrap();
    let first = objects.prepare_change(change(1, None, upsert("one"))).await.unwrap();
    for refused in [WikiTreeChange::Remove, WikiTreeChange::Keep] {
        assert!(matches!(objects.prepare_change(change(4, Some(first), refused)).await, Err(WikiObjectError::Format)));
    }
    assert!(matches!(objects.prepare_change(change(1, None, WikiTreeChange::Remove)).await, Err(WikiObjectError::Format)));
}
