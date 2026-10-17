//! Upgrading a milestone 1 database: a database is built at the schema of migrations
//! 0001–0008 (the last ones milestone 1 shipped), filled with representative rows
//! (`fixtures/milestone1.sql`), and then opened with [`Database::open`], which applies
//! every later migration. The rows must survive unchanged, and the new tables, columns,
//! defaults and triggers must behave on rows that predate them.

use std::{path::Path, str::FromStr};

use rbit_core::{AccessTokenId, AccountId, BasePermission, OrgRole, Permission, RepoId, Timestamp, TokenScope};
use rbit_storage::{Database, NewAccessToken, sqlite::ssh_keys::KeyHolder};
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

/// The last migration milestone 1 shipped.
const MILESTONE_1_SCHEMA: i64 = 8;

const MILESTONE_1_ROWS: &str = include_str!("fixtures/milestone1.sql");

const ALICE: AccountId = AccountId(1);
const BOB: AccountId = AccountId(2);
const ACME: AccountId = AccountId(10);
const BARE: AccountId = AccountId(11);
const ROCKET: RepoId = RepoId(6);

async fn pool(path: &Path) -> SqlitePool {
    let options = SqliteConnectOptions::from_str("sqlite:").unwrap().filename(path).create_if_missing(true).foreign_keys(true);
    SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap()
}

/// A database file as milestone 1 left it, with the fixture rows.
async fn milestone_1_database(path: &Path) {
    let pool = pool(path).await;
    sqlx::migrate!("./migrations").run_to(MILESTONE_1_SCHEMA, &pool).await.unwrap();
    let applied: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations").fetch_one(&pool).await.unwrap();
    assert_eq!(applied, MILESTONE_1_SCHEMA);
    let collaborators: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE name = 'collaborators'").fetch_one(&pool).await.unwrap();
    assert_eq!(collaborators, 0, "the base is really the milestone 1 schema");
    sqlx::raw_sql(MILESTONE_1_ROWS).execute(&pool).await.unwrap();
    pool.close().await;
}

struct Upgraded {
    dir: tempfile::TempDir,
    database: Database,
    /// A second, plain connection for checks the stores do not expose.
    raw: SqlitePool,
}

async fn upgraded() -> Upgraded {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rbit.db");
    milestone_1_database(&path).await;
    let database = Database::open(&path).await.unwrap();
    let raw = pool(&path).await;
    Upgraded { dir, database, raw }
}

impl Upgraded {
    async fn count(&self, sql: &str) -> i64 {
        // Only fixed table names from this file are ever formatted into `sql`.
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned())).fetch_one(&self.raw).await.unwrap()
    }

    async fn exists(&self, kind: &str, name: &str) -> bool {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sqlite_master WHERE type = ? AND name = ?")
            .bind(kind)
            .bind(name)
            .fetch_one(&self.raw)
            .await
            .unwrap()
            == 1
    }
}

#[tokio::test]
async fn every_migration_applies_and_the_database_stays_consistent() {
    let db = upgraded().await;
    let embedded: Vec<i64> = sqlx::migrate!("./migrations").iter().map(|m| m.version).collect();
    let applied: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version").fetch_all(&db.raw).await.unwrap();
    assert_eq!(applied, embedded);
    assert!(applied.contains(&16), "the redirects migration is among them");

    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check").fetch_one(&db.raw).await.unwrap();
    assert_eq!(integrity, "ok");
    let violations = sqlx::query("PRAGMA foreign_key_check").fetch_all(&db.raw).await.unwrap();
    assert!(violations.is_empty(), "no dangling references after the upgrade");

    // Opening again is a no-op.
    db.database.close().await;
    let path = db.dir.path().join("rbit.db");
    Database::open(&path).await.unwrap().close().await;
}

#[tokio::test]
async fn milestone_1_rows_survive() {
    let db = upgraded().await;
    for (table, rows) in [
        ("accounts", 5),
        ("user_logins", 3),
        ("org_profiles", 1),
        ("org_members", 3),
        ("account_profiles", 1),
        ("repositories", 3),
        ("stars", 1),
        ("access_tokens", 2),
        ("instance_settings", 1),
        ("two_factor", 1),
        ("recovery_codes", 2),
        ("email_tokens", 1),
        ("audit_events", 2),
        ("ssh_keys", 1),
        ("deploy_keys", 1),
    ] {
        assert_eq!(db.count(&format!("SELECT COUNT(*) FROM {table}")).await, rows, "{table}");
    }

    let alice = db.database.accounts().find_user(ALICE).await.unwrap().unwrap();
    assert_eq!(alice.name().as_str(), "alice");
    assert!(db.database.accounts().is_suspended(AccountId(3)).await.unwrap(), "carol stays suspended");
    let login = db.database.accounts().find_login("alice@example.com").await.unwrap();
    assert!(login.is_some(), "sign-in by email still works");

    let rocket = db.database.repositories().find_by_path("ACME", "Rocket").await.unwrap().unwrap();
    assert_eq!(rocket.repo.id, ROCKET);
    assert_eq!(rocket.repo.default_branch, "trunk");

    let row = sqlx::query("SELECT secret, last_used_step FROM two_factor WHERE account_id = 1").fetch_one(&db.raw).await.unwrap();
    assert_eq!(row.get::<Vec<u8>, _>("secret").len(), 20);
    assert_eq!(row.get::<i64, _>("last_used_step"), 56_666_700);
    assert_eq!(db.count("SELECT COUNT(*) FROM recovery_codes WHERE used_at IS NULL").await, 1);
    assert_eq!(db.count("SELECT COUNT(*) FROM audit_events WHERE action = 'user_suspended' AND detail = 'spam'").await, 1);

    match db.database.ssh_keys().find_by_fingerprint("SHA256:aliceAliceAliceAliceAliceAliceAliceAliceAli").await.unwrap() {
        Some(KeyHolder::User { key, user, suspended }) => {
            assert_eq!(key.id.0, 3);
            assert_eq!(user.id(), ALICE);
            assert!(!suspended);
        }
        other => panic!("alice's key: {other:?}"),
    }
    match db.database.ssh_keys().find_by_fingerprint("SHA256:deployDeployDeployDeployDeployDeployDeployDe").await.unwrap() {
        Some(KeyHolder::Deploy { key, repo }) => {
            assert!(!key.read_only);
            assert_eq!(repo.repo.id, ROCKET);
        }
        other => panic!("the deploy key: {other:?}"),
    }
    // The milestone 1 triggers are still in place.
    let clash = sqlx::query(
        "INSERT INTO ssh_keys (account_id, title, public_key, algorithm, fingerprint, created_at) \
         VALUES (2, 'x', 'ssh-ed25519 AAAA', 'ssh-ed25519', 'SHA256:deployDeployDeployDeployDeployDeployDeployDe', 1)",
    )
    .execute(&db.raw)
    .await;
    assert!(clash.is_err(), "a deploy key's fingerprint cannot become a user key");
}

#[tokio::test]
async fn the_access_token_rebuild_keeps_ids_and_columns() {
    let db = upgraded().await;
    let store = db.database.access_tokens();

    let (laptop, owner) = store.find_by_digest(&[1; 32]).await.unwrap().unwrap();
    assert_eq!(owner.id(), ALICE);
    assert_eq!(laptop.id, AccessTokenId(7));
    assert_eq!(laptop.name, "laptop");
    assert_eq!(laptop.hint, "ab12");
    assert_eq!(laptop.scope, TokenScope::Write);
    assert_eq!(laptop.created_at, Timestamp(1_700_005_000));
    assert_eq!(laptop.last_used_at, Some(Timestamp(1_700_005_500)));
    assert_eq!(laptop.expires_at, None);

    let (ci, owner) = store.find_by_digest(&[2; 32]).await.unwrap().unwrap();
    assert_eq!(owner.id(), BOB);
    assert_eq!(ci.id, AccessTokenId(42));
    assert_eq!(ci.scope, TokenScope::Read);
    assert_eq!(ci.expires_at, Some(Timestamp(4_000_000_000)));

    // The new scope is accepted, and new ids continue after the highest kept one.
    let admin = store
        .create(NewAccessToken {
            account_id: ALICE,
            name: "automation".to_owned(),
            digest: [9; 32],
            hint: "ef56".to_owned(),
            scope: TokenScope::Admin,
            expires_at: None,
        })
        .await
        .unwrap();
    assert_eq!(admin.scope, TokenScope::Admin);
    assert!(admin.id.0 > 42, "no id is reused: {}", admin.id.0);
    let listed: Vec<_> = store.list_for_account(ALICE).await.unwrap().into_iter().map(|t| t.id).collect();
    assert_eq!(listed, vec![admin.id, AccessTokenId(7)], "newest first");

    // Constraints of the rebuilt table: unique digest, unknown scopes refused, and the
    // foreign key to the account (with its cascade) kept.
    let duplicate = sqlx::query(
        "INSERT INTO access_tokens (account_id, name, token_digest, hint, scope, created_at) VALUES (1, 'dup', ?, 'x', 'read', 1)",
    )
    .bind([1u8; 32].as_slice())
    .execute(&db.raw)
    .await;
    assert!(duplicate.is_err(), "digests stay unique");
    let unknown = sqlx::query(
        "INSERT INTO access_tokens (account_id, name, token_digest, hint, scope, created_at) VALUES (1, 'x', ?, 'x', 'root', 1)",
    )
    .bind([8u8; 32].as_slice())
    .execute(&db.raw)
    .await;
    assert!(unknown.is_err(), "the scope CHECK is still there");
    assert!(db.exists("index", "access_tokens_by_account").await);
    assert!(!db.exists("table", "access_tokens_v2").await, "the temporary table is gone");
    let references: Vec<String> =
        sqlx::query("PRAGMA foreign_key_list(access_tokens)").fetch_all(&db.raw).await.unwrap().iter().map(|r| r.get("table")).collect();
    assert_eq!(references, vec!["accounts".to_owned()]);

    sqlx::query("DELETE FROM accounts WHERE id = 2").execute(&db.raw).await.unwrap();
    assert!(store.find_by_digest(&[2; 32]).await.unwrap().is_none(), "deleting the owner deletes the token");
}

#[tokio::test]
async fn existing_organisations_keep_the_milestone_1_member_permission() {
    let db = upgraded().await;
    let orgs = db.database.orgs();
    assert_eq!(orgs.base_permission(ACME).await.unwrap(), BasePermission::Write, "the column default fills old rows");
    assert_eq!(orgs.base_permission(BARE).await.unwrap(), BasePermission::Write, "no profile row reads as the default");
    assert_eq!(db.count("SELECT COUNT(*) FROM org_profiles WHERE member_permission = 'write'").await, 1);

    // Bob, a member since milestone 1, still writes to the organisation's repositories.
    let grants = db.database.repositories().grants(ROCKET, ACME, BOB).await.unwrap();
    assert_eq!(grants.org_role, Some(OrgRole::Member));
    assert_eq!(grants.org_base, BasePermission::Write);
    assert_eq!(grants.team, None);
    assert_eq!(grants.collaborator, None);

    orgs.set_base_permission(ACME, BasePermission::Read).await.unwrap();
    assert_eq!(orgs.base_permission(ACME).await.unwrap(), BasePermission::Read);
    let invalid = sqlx::query("UPDATE org_profiles SET member_permission = 'admin' WHERE account_id = 10").execute(&db.raw).await;
    assert!(invalid.is_err(), "the CHECK on the added column holds");
    let profile = sqlx::query("SELECT description, website FROM org_profiles WHERE account_id = 10").fetch_one(&db.raw).await.unwrap();
    assert_eq!(profile.get::<String, _>("description"), "Makers of everything");
    assert_eq!(profile.get::<String, _>("website"), "https://acme.example");
}

#[tokio::test]
async fn leaving_an_organisation_revokes_grants_for_members_from_milestone_1() {
    let db = upgraded().await;
    assert!(db.exists("trigger", "org_members_revoke_grants").await);
    db.database.repositories().grant_collaborator(ROCKET, BOB, Permission::Admin).await.unwrap();
    assert_eq!(db.database.repositories().grants(ROCKET, ACME, BOB).await.unwrap().collaborator, Some(Permission::Admin));

    db.database.orgs().remove_member(ACME, BOB).await.unwrap();
    let grants = db.database.repositories().grants(ROCKET, ACME, BOB).await.unwrap();
    assert_eq!(grants.org_role, None);
    assert_eq!(grants.collaborator, None, "the trigger removed the collaborator entry");
}

#[tokio::test]
async fn repositories_from_milestone_1_get_redirects_when_they_move() {
    let db = upgraded().await;
    for trigger in ["repo_redirects_on_create", "repo_redirects_on_move"] {
        assert!(db.exists("trigger", trigger).await, "{trigger}");
    }
    assert_eq!(db.count("SELECT COUNT(*) FROM repo_redirects").await, 0, "nothing is invented for the past");

    let repositories = db.database.repositories();
    db.database.repo_settings().rename(RepoId(5), &"config".parse().unwrap()).await.unwrap();
    let moved = repositories.find_by_old_path("alice", "dotfiles").await.unwrap().unwrap();
    assert_eq!(moved.full_name(), "alice/config");

    repositories.transfer(RepoId(5), ACME).await.unwrap();
    for (owner, name) in [("alice", "dotfiles"), ("alice", "config")] {
        let moved = repositories.find_by_old_path(owner, name).await.unwrap().unwrap();
        assert_eq!(moved.full_name(), "acme/config", "{owner}/{name}");
    }
    // The star from milestone 1 followed the repository.
    assert_eq!(db.count("SELECT COUNT(*) FROM stars WHERE repo_id = 5").await, 1);
}

#[tokio::test]
async fn new_tables_start_empty_with_their_defaults() {
    let db = upgraded().await;
    for table in [
        "collaborators",
        "teams",
        "team_members",
        "team_repositories",
        "user_preferences",
        "webhooks",
        "webhook_deliveries",
        "releases",
        "release_assets",
        "repo_forks",
        "repo_mirrors",
        "repo_redirects",
        "issue_counters",
        "issues",
        "issue_comments",
        "issue_events",
    ] {
        assert!(db.exists("table", table).await, "{table}");
        assert_eq!(db.count(&format!("SELECT COUNT(*) FROM {table}")).await, 0, "{table}");
    }

    assert_eq!(db.database.preferences().theme(ALICE).await.unwrap(), None, "existing users have no theme stored");

    sqlx::query("INSERT INTO webhooks (repo_id, url, created_at, updated_at) VALUES (6, 'https://hooks.example/x', 1, 1)")
        .execute(&db.raw)
        .await
        .unwrap();
    let hook = sqlx::query("SELECT id, events, active, secret FROM webhooks").fetch_one(&db.raw).await.unwrap();
    assert_eq!(hook.get::<String, _>("events"), "[]");
    assert_eq!(hook.get::<i64, _>("active"), 1);
    assert_eq!(hook.get::<Option<String>, _>("secret"), None);
    let hook_id: i64 = hook.get("id");
    sqlx::query("INSERT INTO webhook_deliveries (hook_id, guid, event, payload, created_at) VALUES (?, 'g-1', 'push', '{}', 1)")
        .bind(hook_id)
        .execute(&db.raw)
        .await
        .unwrap();
    let delivery = sqlx::query("SELECT status, attempts FROM webhook_deliveries").fetch_one(&db.raw).await.unwrap();
    assert_eq!(delivery.get::<String, _>("status"), "pending");
    assert_eq!(delivery.get::<i64, _>("attempts"), 0);
    let both = sqlx::query("INSERT INTO webhooks (repo_id, org_id, url, created_at, updated_at) VALUES (6, 10, 'https://x', 1, 1)")
        .execute(&db.raw)
        .await;
    assert!(both.is_err(), "a hook belongs to a repository or an organisation, not both");

    sqlx::query("INSERT INTO releases (repo_id, tag_name, name, created_at, updated_at) VALUES (6, 'v1.0.0', 'One', 1, 1)")
        .execute(&db.raw)
        .await
        .unwrap();
    let release = sqlx::query("SELECT notes, is_draft, is_prerelease, published_at FROM releases").fetch_one(&db.raw).await.unwrap();
    assert_eq!(release.get::<String, _>("notes"), "");
    assert_eq!(release.get::<i64, _>("is_draft"), 0);
    assert_eq!(release.get::<i64, _>("is_prerelease"), 0);
    assert_eq!(release.get::<Option<i64>, _>("published_at"), None);

    // Deleting a milestone 1 repository cascades into the new tables.
    sqlx::query("DELETE FROM repositories WHERE id = 6").execute(&db.raw).await.unwrap();
    assert_eq!(db.count("SELECT COUNT(*) FROM webhooks").await, 0);
    assert_eq!(db.count("SELECT COUNT(*) FROM webhook_deliveries").await, 0);
    assert_eq!(db.count("SELECT COUNT(*) FROM releases").await, 0);
}
