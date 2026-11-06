//! The production service delegates public discovery to the domain application and SQL port.

use std::{any::TypeId, sync::Arc};

use rbit_core::{AccountId, Config, RepoId, RepoSort, Timestamp, Visibility, repository::ArchivedFilter};
use rbit_services::{RepoSearch, RepoSearchResults, Services};
use rbit_storage::{Database, KvStore, NewRepository, NewUser, count_work};

struct Fixture {
    _directory: tempfile::TempDir,
    database: Database,
    services: Services,
}
impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(&directory.path().join("database.sqlite")).await.unwrap();
        let kv = KvStore::open(&directory.path().join("values.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().into();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        Self { _directory: directory, database, services }
    }

    async fn owner(&self, name: &str) -> AccountId {
        self.database
            .accounts()
            .create_user(NewUser {
                name: name.parse().unwrap(),
                email: format!("{name}@example.com").parse().unwrap(),
                display_name: None,
                password_hash: "not-a-real-hash".into(),
                is_admin: false,
            })
            .await
            .unwrap()
            .id()
    }

    async fn repository(&self, owner_id: AccountId, name: &str, description: Option<&str>, visibility: Visibility) -> RepoId {
        self.database
            .repositories()
            .create(NewRepository {
                owner_id,
                name: name.parse().unwrap(),
                description: description.map(str::to_owned),
                visibility,
                default_branch: "main".into(),
            })
            .await
            .unwrap()
            .id
    }
}

#[test]
fn discovery_legacy_paths_are_exact_domain_types() {
    assert_eq!(TypeId::of::<RepoSearch>(), TypeId::of::<rbit_repositories::discovery::RepoSearch>());
    assert_eq!(TypeId::of::<rbit_services::repositories::RepoSearch>(), TypeId::of::<RepoSearch>());
    assert_eq!(TypeId::of::<RepoSearchResults>(), TypeId::of::<rbit_repositories::discovery::RepoSearchResults>());
    assert_eq!(rbit_services::repositories::EXPLORE_PAGE_SIZE, rbit_repositories::discovery::EXPLORE_PAGE_SIZE);
}

#[tokio::test]
async fn literals_privacy_archives_and_two_statement_work_survive_service_activation() {
    let fixture = Fixture::new().await;
    let owner = fixture.owner("catalog-owner").await;
    let visible = fixture.repository(owner, "visible", Some("literal % _ \\ marker"), Visibility::Public).await;
    fixture.repository(owner, "plain", Some("marker"), Visibility::Public).await;
    fixture.repository(owner, "hidden", Some("literal % _ \\ marker"), Visibility::Private).await;

    for literal in ["%", "_", "\\"] {
        let search = RepoSearch { text: format!("  {literal}  "), ..RepoSearch::default() };
        let (result, work) = count_work(fixture.services.repositories.explore(&search)).await;
        let result = result.unwrap();
        assert_eq!((result.total, result.page, result.repos.len()), (1, 1, 1));
        assert_eq!(result.repos[0].repo.id, visible);
        assert_eq!((work.statements, work.acquisitions, work.cache_writes), (2, 2, 0));
    }

    fixture.database.repo_settings().archive(visible, owner, Timestamp(0)).await.unwrap();
    for (archived, total) in [(ArchivedFilter::Include, 1), (ArchivedFilter::Exclude, 0), (ArchivedFilter::Only, 1)] {
        let search = RepoSearch { text: "%".into(), archived, ..RepoSearch::default() };
        let found = fixture.services.repositories.explore(&search).await.unwrap();
        assert_eq!(found.total, total);
        assert_eq!(u64::try_from(found.repos.len()).unwrap(), total);
    }
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn case_insensitive_name_ties_keep_owner_order_without_changing_sql() {
    let fixture = Fixture::new().await;
    let b = fixture.owner("b-owner").await;
    let a = fixture.owner("a-owner").await;
    fixture.repository(b, "same", None, Visibility::Public).await;
    fixture.repository(a, "Same", None, Visibility::Public).await;
    let found = fixture.services.repositories.explore(&RepoSearch { sort: RepoSort::Name, ..RepoSearch::default() }).await.unwrap();
    let names: Vec<_> = found.repos.into_iter().map(|repo| repo.full_name()).collect();
    assert_eq!(names, ["a-owner/Same", "b-owner/same"]);
    fixture.services.shutdown().await;
}
