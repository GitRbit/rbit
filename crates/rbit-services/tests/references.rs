//! Resolving references for rendering (workstream I): only existing threads of the
//! repository and existing accounts resolve, from Markdown text only.

use std::sync::Arc;

use rbit_core::{Config, IssueKind, IssueNumber, User, Visibility};
use rbit_services::{CreateRepository, Services, issues::NewIssueInput};
use rbit_storage::{Database, KvStore, NewUser};

async fn user(database: &Database, name: &str) -> User {
    database
        .accounts()
        .create_user(NewUser {
            name: name.parse().unwrap(),
            email: format!("{name}@example.com").parse().unwrap(),
            display_name: None,
            password_hash: "not-a-real-hash".to_owned(),
            is_admin: false,
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn resolves_existing_numbers_and_accounts_in_one_batch() {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rustit.db")).await.unwrap();
    let kv = KvStore::open(&dir.path().join("rustit.redb")).unwrap();
    let mut config = Config::default();
    config.storage.data_dir = dir.path().to_path_buf();
    let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
    let alice = user(&database, "alice").await;
    user(&database, "Bob").await;
    let input = CreateRepository { name: "demo".to_owned(), description: String::new(), visibility: Visibility::Public };
    services.repositories.create(&alice, input).await.unwrap();
    let repo = services.repositories.open(Some(&alice), "alice", "demo").await.unwrap();
    services.issues.create(&alice, &repo, NewIssueInput { title: "Crash".into(), ..NewIssueInput::default() }).await.unwrap();

    let texts = ["See #1 and #2, thanks @bob", "`#1 @alice` and @nobody", "[#1](x) other/repo#5"];
    let set = services.references.resolve(&repo, &texts).await.unwrap();
    let one = IssueNumber::new(1).unwrap();
    assert_eq!(set.issues.keys().copied().collect::<Vec<_>>(), [one], "#2 does not exist");
    assert_eq!((set.issues[&one].title.as_str(), set.issues[&one].kind), ("Crash", IssueKind::Issue));
    assert_eq!(set.account("BOB"), Some("Bob"), "the stored spelling");
    assert_eq!(set.account("alice"), None, "@alice only appears in code");
    assert_eq!(set.account("nobody"), None);
    assert_eq!(services.references.resolve(&repo, &[]).await.unwrap(), rbit_services::references::ReferenceSet::default());
}
