use super::*;
use rbit_core::lfs::LfsOid;
use sha2::{Digest, Sha256};

fn input(visibility: Visibility) -> CreateRepository {
    CreateRepository { name: "from-template".to_owned(), description: String::new(), visibility }
}

fn pointer(oid: &LfsOid, size: usize) -> String {
    format!("version https://git-lfs.github.com/spec/v1\noid sha256:{oid}\nsize {size}\n")
}

#[tokio::test]
async fn repository_template_copies_only_the_default_branch_tree_and_stands_alone() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    let source = fixture.parent(&alice, Visibility::Public).await;
    let services = &fixture.services;
    let upstream = services.repositories.open(Some(&bob), "alice", "demo").await.unwrap();
    services.forks.fork(&bob, &upstream, ForkInput::default()).await.unwrap();
    let fork = services.repositories.open(Some(&bob), "bob", "demo").await.unwrap();
    let shared = b"shared object".as_slice();
    let omitted = b"historical object".as_slice();
    let shared_oid = LfsOid::from_digest(Sha256::digest(shared).into());
    let omitted_oid = LfsOid::from_digest(Sha256::digest(omitted).into());
    services.lfs.store(&source, &shared_oid, shared.len() as u64, shared).await.unwrap();
    services.lfs.store(&fork, &omitted_oid, omitted.len() as u64, omitted).await.unwrap();

    let work = fixture.dir.path().join("work");
    std::fs::write(work.join("README.md"), "snapshot\n").unwrap();
    std::fs::write(work.join("asset.bin"), pointer(&shared_oid, shared.len())).unwrap();
    git(&work, &["add", "README.md", "asset.bin"]);
    git(&work, &["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "--quiet", "-m", "second"]);
    git(&work, &["push", "--quiet", fork.disk_path.to_str().unwrap(), "main"]);
    services.repositories.create_from_template(&bob, "", input(Visibility::Public), "bob/demo", &services.lfs).await.unwrap();
    let target = services.repositories.open(Some(&bob), "bob", "from-template").await.unwrap();
    assert!(target.repo.repo.pushed_at.is_some());
    assert_eq!(target.reader.refs().await.unwrap().len(), 1);
    assert!(services.forks.parent(None, &target).await.unwrap().is_none());
    let checkout = fixture.dir.path().join("copy");
    git(fixture.dir.path(), &["clone", "--quiet", target.disk_path.to_str().unwrap(), checkout.to_str().unwrap()]);
    assert_eq!(target.reader.history("main".into(), 0, 10).await.unwrap().len(), 1);
    assert_eq!(std::fs::read_to_string(checkout.join("README.md")).unwrap().lines().collect::<Vec<_>>(), ["snapshot"]);
    assert!(!target.disk_path.join("objects/info/alternates").exists());
    assert_eq!(services.lfs.find(&target, None, &shared_oid).await.unwrap(), Some(shared.len() as u64));
    assert_eq!(services.lfs.find(&target, None, &omitted_oid).await.unwrap(), None);
}

#[tokio::test]
async fn repository_template_protects_private_sources_and_rolls_back_empty_sources() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    let source = fixture.parent(&alice, Visibility::Private).await;
    let services = &fixture.services;
    let repos = &services.repositories;
    let lfs = &services.lfs;
    let inaccessible = repos.create_from_template(&bob, "", input(Visibility::Private), "alice/demo", lfs).await;
    assert!(matches!(inaccessible, Err(ServiceError::Invalid(_))));
    fixture.database.repositories().grant_collaborator(source.repo.repo.id, bob.id(), rbit_core::Permission::Read).await.unwrap();
    let read_only = repos.create_from_template(&bob, "", input(Visibility::Private), "alice/demo", lfs).await;
    assert!(matches!(read_only, Err(ServiceError::Forbidden)));
    let public_copy = repos.create_from_template(&alice, "", input(Visibility::Public), "alice/demo", lfs).await;
    assert!(matches!(public_copy, Err(ServiceError::Invalid(_))));
    let empty = CreateRepository { name: "empty".to_owned(), ..input(Visibility::Public) };
    repos.create(&alice, empty).await.unwrap();
    let empty_copy = repos.create_from_template(&alice, "", input(Visibility::Private), "alice/empty", lfs).await;
    assert!(matches!(empty_copy, Err(ServiceError::Invalid(_))));
    assert!(fixture.database.repositories().find_by_path("alice", "from-template").await.unwrap().is_none());
    assert!(fixture.database.repositories().find_by_path("bob", "from-template").await.unwrap().is_none());
    let created = repos.create_from_template(&alice, "", input(Visibility::Private), "alice/demo", lfs).await.unwrap();
    assert_eq!(created.repo.visibility, Visibility::Private);
}
