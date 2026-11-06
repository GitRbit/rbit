//! Real suggestion query counts and stable labels.
use super::*;
use rbit_core::{IssueText, IssueTitle, RepoAccess};
use rbit_services::ServiceError;
use rbit_storage::{count_work, sqlite::issues::NewIssue};
#[tokio::test]
async fn account_number_and_empty_search_keep_one_query_and_denial_zero() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(&directory.path().join("rbit.db")).await.unwrap();
    let kv = KvStore::open(&directory.path().join("rbit.redb")).unwrap();
    let mut config = Config::default();
    config.storage.data_dir = directory.path().to_path_buf();
    let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
    let alice = user(&database, "alice").await;
    services
        .repositories
        .create(&alice, CreateRepository { name: "demo".to_owned(), description: String::new(), visibility: Visibility::Public })
        .await
        .unwrap();
    let mut repo = services.repositories.open(Some(&alice), "alice", "demo").await.unwrap();
    let issue = database
        .issues()
        .create(&NewIssue {
            repo: repo.repo.repo.id,
            kind: IssueKind::Issue,
            title: IssueTitle::parse("Issue title").unwrap(),
            body: IssueText::body("").unwrap(),
            author: alice.id(),
            at: rbit_core::Timestamp(1),
        })
        .await
        .unwrap();
    let pull = database
        .issues()
        .create(&NewIssue {
            repo: repo.repo.repo.id,
            kind: IssueKind::Pull,
            title: IssueTitle::parse("Pull title").unwrap(),
            body: IssueText::body("").unwrap(),
            author: alice.id(),
            at: rbit_core::Timestamp(2),
        })
        .await
        .unwrap();
    let (accounts, work) = count_work(services.references.suggestions(&repo, "ali", true)).await;
    assert_eq!(accounts.unwrap(), [("@alice".to_owned(), "alice".to_owned())]);
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    let (found, work) = count_work(services.references.suggestions(&repo, &pull.number.get().to_string(), false)).await;
    assert_eq!(found.unwrap(), [(format!("#{}", pull.number.get()), "PR · Pull title".to_owned())]);
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    let (all, work) = count_work(services.references.suggestions(&repo, "", false)).await;
    assert_eq!(
        all.unwrap(),
        [
            (format!("#{}", pull.number.get()), "PR · Pull title".to_owned()),
            (format!("#{}", issue.number.get()), "Issue · Issue title".to_owned())
        ]
    );
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    repo.access = RepoAccess::None;
    let (denied, work) = count_work(services.references.suggestions(&repo, "ali", true)).await;
    assert!(matches!(denied, Err(ServiceError::NotFound)));
    assert_eq!((work.acquisitions, work.statements), (0, 0));
}
