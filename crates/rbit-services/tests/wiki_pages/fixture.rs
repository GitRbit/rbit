//! A temporary instance with one repository, its wiki turned on, and four people.
use rbit_core::{Config, Permission, Timestamp, Visibility};
use rbit_identity_models::account::User;
use rbit_services::{CreateRepository, RepoContext, Services, wikis::publish::WikiPageInput};
use rbit_storage::{Database, KvStore, NewUser};
use std::sync::Arc;

pub(super) struct Fixture {
    pub _directory: tempfile::TempDir,
    pub database: Database,
    pub services: Services,
    pub owner: User,
    pub writer: User,
    pub reader: User,
    pub outsider: User,
}

impl Fixture {
    pub async fn new(visibility: Visibility) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(&directory.path().join("wiki.db")).await.unwrap();
        let kv = KvStore::open(&directory.path().join("wiki.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let owner = Self::user(&database, "owner").await;
        let writer = Self::user(&database, "writer").await;
        let reader = Self::user(&database, "reader").await;
        let outsider = Self::user(&database, "outsider").await;
        services
            .repositories
            .create(&owner, CreateRepository { name: "handbook".into(), description: String::new(), visibility })
            .await
            .unwrap();
        let fixture = Self { _directory: directory, database, services, owner, writer, reader, outsider };
        let repo = fixture.repo(&fixture.owner).await;
        fixture.services.collaborators.add(&fixture.owner, &repo, "writer", Permission::Write).await.unwrap();
        fixture.services.collaborators.add(&fixture.owner, &repo, "reader", Permission::Read).await.unwrap();
        fixture.services.wikis.set_enabled(&repo, true).await.unwrap();
        fixture
    }

    async fn user(database: &Database, name: &str) -> User {
        database
            .accounts()
            .create_user(NewUser {
                name: name.parse().unwrap(),
                email: format!("{name}@example.com").parse().unwrap(),
                display_name: None,
                password_hash: "synthetic wiki fixture".into(),
                is_admin: false,
            })
            .await
            .unwrap()
    }

    /// The repository as `user` sees it now (access recomputed on every call).
    pub async fn repo(&self, user: &User) -> RepoContext {
        self.services.repositories.open(Some(user), "owner", "handbook").await.unwrap()
    }

    pub async fn archive(&self) {
        self.database.repo_settings().archive(self.repo(&self.owner).await.repo.repo.id, self.owner.id(), Timestamp(2)).await.unwrap();
    }
}

pub(super) fn input(title: &str, path: &str, content: &str) -> WikiPageInput {
    WikiPageInput { title: title.into(), path: path.into(), content: content.into() }
}
