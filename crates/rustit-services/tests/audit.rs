//! The security audit log against a real, temporary SQLite database.

use std::sync::Arc;

use rustit_core::{Config, User};
use rustit_services::{
    ServiceError, Services,
    admin::{ListQuery, PAGE_SIZE},
    audit::{AuditAction, AuditCategory, AuditEntry},
};
use rustit_storage::{Database, KvStore, NewUser};

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        let config = Arc::new(config);
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(config, &database, &kv).await.unwrap();
        Self { _dir: dir, database, services }
    }

    async fn user(&self, name: &str, is_admin: bool) -> User {
        self.database
            .accounts()
            .create_user(NewUser {
                name: name.parse().unwrap(),
                email: format!("{name}@example.com").parse().unwrap(),
                display_name: None,
                password_hash: "not-a-real-hash".to_owned(),
                is_admin,
            })
            .await
            .unwrap()
    }
}

fn query(search: &str, page: u32) -> ListQuery {
    ListQuery { search: search.to_owned(), page }
}

#[tokio::test]
async fn records_and_filters_events_newest_first() {
    let fixture = Fixture::new().await;
    let admin = fixture.user("root", true).await;
    let alice = fixture.user("alice", false).await;
    let audit = &fixture.services.audit;
    let ip = Some("198.51.100.7".parse().unwrap());

    audit.record(AuditEntry::new(AuditAction::SignInFailed).by_login("alice").from_ip(ip).detail("wrong password")).await;
    audit.record(AuditEntry::new(AuditAction::SignIn).by(&alice).from_ip(ip)).await;
    audit.record(AuditEntry::new(AuditAction::UserSuspended).by(&admin).on("mallory")).await;

    let all = audit.list(&admin, None, &query("", 1)).await.unwrap();
    assert_eq!(all.total, 3);
    let actions: Vec<_> = all.items.iter().map(|event| event.action).collect();
    assert_eq!(actions, [AuditAction::UserSuspended, AuditAction::SignIn, AuditAction::SignInFailed]);
    assert_eq!(all.items[1].actor_id, Some(alice.id()));
    assert_eq!(all.items[2].actor_id, None, "failed sign-ins are not attributed to an account");
    assert_eq!(all.items[2].detail, "wrong password");

    let sign_ins = audit.list(&admin, Some(AuditCategory::Authentication), &query("", 1)).await.unwrap();
    assert_eq!(sign_ins.total, 2);

    let about_mallory = audit.list(&admin, None, &query("MALL", 1)).await.unwrap();
    assert_eq!(about_mallory.items.len(), 1);
    assert_eq!(about_mallory.items[0].target_name.as_deref(), Some("mallory"));

    let by_address = audit.list(&admin, None, &query("198.51.100", 1)).await.unwrap();
    assert_eq!(by_address.total, 2);

    let wildcard = audit.list(&admin, None, &query("%", 1)).await.unwrap();
    assert_eq!(wildcard.total, 0, "LIKE wildcards are matched literally");

    let own = audit.recent_for(&alice, 10).await.unwrap();
    assert_eq!(own.len(), 1);
    assert_eq!(own[0].action, AuditAction::SignIn);
}

#[tokio::test]
async fn only_administrators_read_the_log_and_it_pages() {
    let fixture = Fixture::new().await;
    let admin = fixture.user("root", true).await;
    let alice = fixture.user("alice", false).await;
    let audit = &fixture.services.audit;

    assert!(matches!(audit.list(&alice, None, &query("", 1)).await, Err(ServiceError::Forbidden)));

    for _ in 0..=PAGE_SIZE {
        audit.record(AuditEntry::new(AuditAction::TokenCreated).by(&alice).on("laptop")).await;
    }
    let first = audit.list(&admin, Some(AuditCategory::Tokens), &query("", 1)).await.unwrap();
    assert_eq!(first.items.len(), PAGE_SIZE as usize);
    assert!(first.has_next());
    let second = audit.list(&admin, Some(AuditCategory::Tokens), &query("", 2)).await.unwrap();
    assert_eq!(second.items.len(), 1);
    assert!(second.items[0].id < first.items.last().unwrap().id);
}
