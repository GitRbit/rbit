//! Closed temporary persistence fixtures, not a production publication/backdoor API.
use rbit_core::{Config, Timestamp, User, Visibility};
use rbit_git::wikis::{PrepareWikiPage, WikiObjects};
use rbit_kernel::BlockingBudget;
use rbit_services::{CreateRepository, RepoContext, Services};
use rbit_storage::{Database, KvStore, NewUser, sqlite::writer_compatibility::register};
use rbit_wikis::{WikiContent, WikiPageId, WikiRevisionId};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use std::{num::NonZeroUsize, sync::Arc};

pub(super) struct Fixture {
    pub directory: tempfile::TempDir,
    pub database: Database,
    pub raw: SqlitePool,
    pub services: Services,
    pub owner: User,
    pub outsider: User,
    pub repo: RepoContext,
    pub objects: WikiObjects,
    pub work: BlockingBudget,
}

impl Fixture {
    pub async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("wiki.db");
        let database = Database::open(&path).await.unwrap();
        let raw = SqlitePool::connect_with(register(SqliteConnectOptions::new().filename(path).foreign_keys(true))).await.unwrap();
        let kv = KvStore::open(&directory.path().join("wiki.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let owner = Self::user(&database, "owner").await;
        let outsider = Self::user(&database, "outsider").await;
        services
            .repositories
            .create(&owner, CreateRepository { name: "wiki-project".into(), description: String::new(), visibility: Visibility::Private })
            .await
            .unwrap();
        let repo = services.repositories.open(Some(&owner), "owner", "wiki-project").await.unwrap();
        sqlx::query("INSERT INTO wiki_repositories(repo_id,enabled) VALUES(?,1)").bind(repo.repo.repo.id.0).execute(&raw).await.unwrap();
        let identity = database.wikis().identity().await.unwrap();
        let work = BlockingBudget::new(NonZeroUsize::new(1).unwrap());
        let objects = WikiObjects::open(&directory.path().join("wiki-objects"), identity, work.clone()).unwrap();
        Self { directory, database, raw, services, owner, outsider, repo, objects, work }
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

    pub async fn objects(&self, page: i64, text: &str, parent: Option<WikiRevisionId>, at: i64) -> WikiRevisionId {
        self.objects
            .prepare(PrepareWikiPage {
                repository: self.repo.repo.repo.id,
                page: WikiPageId::new(page).unwrap(),
                parent,
                content: WikiContent::try_from(text.to_owned()).unwrap(),
                actor: self.owner.id(),
                at: Timestamp(at),
            })
            .await
            .unwrap()
    }

    /// Seed an already-finalized state ONLY in this closed fixture. This is explicitly
    /// not the required future current-ACL/intent/Git-CAS/recovery publisher.
    pub async fn committed(&self, page: i64, path: &str, title: &str, generation: i64, revision: WikiRevisionId) {
        let mut tx = self.raw.begin().await.unwrap();
        let repository = self.repo.repo.repo.id.0;
        sqlx::query("INSERT INTO wiki_revisions(repo_id,generation,revision,created_at) VALUES(?,?,?,?)")
            .bind(repository)
            .bind(generation)
            .bind(revision.to_string())
            .bind(generation)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO wiki_pages(id,repo_id,path,title,created_at,updated_at) VALUES(?,?,?,?,1,?) ON CONFLICT(id) DO UPDATE SET path=excluded.path,title=excluded.title,updated_at=excluded.updated_at")
            .bind(page).bind(repository).bind(path).bind(title).bind(generation).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO wiki_page_revisions(repo_id,page_id,generation,path,title) VALUES(?,?,?,?,?)")
            .bind(repository)
            .bind(page)
            .bind(generation)
            .bind(path)
            .bind(title)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("UPDATE wiki_repositories SET generation=? WHERE repo_id=?")
            .bind(generation)
            .bind(repository)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
}
