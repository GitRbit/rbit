//! Reference aliases and actual selected-batch SQL costs.
use super::*;
use rbit_storage::count_work;
#[test]
fn legacy_reference_types_are_exact_feature_owner_aliases() {
    use rbit_collaboration::references as owner;
    use std::any::TypeId;
    assert_eq!(TypeId::of::<rbit_core::references::Scan>(), TypeId::of::<owner::Scan>());
    assert_eq!(TypeId::of::<rbit_core::references::Found>(), TypeId::of::<owner::Found>());
    assert_eq!(TypeId::of::<rbit_services::references::ReferenceSet>(), TypeId::of::<owner::ReferenceSet>());
}
#[tokio::test]
async fn empty_remote_only_and_combined_render_batches_keep_zero_zero_and_two_queries() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(&directory.path().join("rbit.db")).await.unwrap();
    let kv = KvStore::open(&directory.path().join("rbit.redb")).unwrap();
    let mut config = Config::default();
    config.storage.data_dir = directory.path().to_path_buf();
    let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
    let alice = user(&database, "alice").await;
    services
        .repositories
        .create(&alice, CreateRepository { name: "demo".into(), description: String::new(), visibility: Visibility::Public })
        .await
        .unwrap();
    let repo = services.repositories.open(Some(&alice), "alice", "demo").await.unwrap();
    services.issues.create(&alice, &repo, NewIssueInput { title: "Target".into(), ..NewIssueInput::default() }).await.unwrap();
    for texts in [Vec::<&str>::new(), vec!["alice/private#1"], vec!["`#1 @alice` [#1](target)"]] {
        let (result, work) = count_work(services.references.resolve(&repo, &texts)).await;
        assert_eq!(result.unwrap(), rbit_services::references::ReferenceSet::default());
        assert_eq!((work.acquisitions, work.statements), (0, 0));
    }
    let (result, work) = count_work(services.references.resolve(&repo, &["#1 #1 @alice @ALICE", "#1 @alice"])).await;
    let result = result.unwrap();
    assert_eq!(result.issues.len(), 1);
    assert_eq!(result.accounts.len(), 1);
    assert_eq!((work.acquisitions, work.statements), (2, 2));
}
