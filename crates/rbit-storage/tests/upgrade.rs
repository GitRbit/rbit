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
async fn existing_review_threads_upgrade_as_unresolved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rbit.db");
    let raw = pool(&path).await;
    sqlx::migrate!("./migrations").run_to(40, &raw).await.unwrap();
    sqlx::raw_sql(MILESTONE_1_ROWS).execute(&raw).await.unwrap();
    sqlx::raw_sql(
        "INSERT INTO issues (id, repo_id, number, kind, title, author_id, created_at, updated_at) \
         VALUES (1, 6, 1, 'pull', 'Review me', 1, 10, 10); \
         INSERT INTO pull_requests (issue_id, repo_id, head_repo_id, head_branch, base_branch, head_sha, created_at, updated_at) \
         VALUES (1, 6, 6, 'feature', 'main', 'abc', 10, 10); \
         INSERT INTO pull_reviews (id, pull_id, reviewer_id, state, commit_sha, created_at, submitted_at) \
         VALUES (1, 1, 2, 'commented', 'abc', 11, 11); \
         INSERT INTO pull_review_comments (id, review_id, pull_id, author_id, path, side, line, commit_sha, body, created_at, updated_at) \
         VALUES (1, 1, 1, 2, 'f', 'right', 1, 'abc', 'Existing question', 11, 11);",
    )
    .execute(&raw)
    .await
    .unwrap();
    let database = Database::open(&path).await.unwrap();
    let comment = database.reviews().find_comment(rbit_core::IssueId(1), rbit_core::pull::ReviewCommentId(1)).await.unwrap().unwrap();
    assert_eq!(comment.body, "Existing question");
    assert_eq!((comment.resolved_at, comment.resolved_by), (None, None));
    database.reviews().set_thread_resolved(comment.pull_id, comment.id, BOB, true, Timestamp(12)).await.unwrap();
    // Removing the resolver preserves the resolution timestamp and conversation.
    sqlx::query("DELETE FROM accounts WHERE id = 2").execute(&raw).await.unwrap();
    let comment = database.reviews().find_comment(comment.pull_id, comment.id).await.unwrap().unwrap();
    assert_eq!((comment.resolved_at, comment.resolved_by), (Some(Timestamp(12)), None));
    assert!(sqlx::query("PRAGMA foreign_key_check").fetch_all(&raw).await.unwrap().is_empty());
    database.close().await;
    raw.close().await;
}

#[tokio::test]
async fn every_migration_applies_and_the_database_stays_consistent() {
    let db = upgraded().await;
    let embedded: Vec<i64> = sqlx::migrate!("./migrations").iter().map(|m| m.version).collect();
    let applied: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version").fetch_all(&db.raw).await.unwrap();
    assert_eq!(applied, embedded);
    assert!(applied.contains(&16), "the redirects migration is among them");
    assert!(applied.contains(&42), "the account redirects migration is among them");

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
async fn accounts_from_milestone_1_get_identity_based_redirects() {
    let db = upgraded().await;
    for trigger in ["account_redirects_on_create", "account_redirects_on_rename"] {
        assert!(db.exists("trigger", trigger).await);
    }
    assert_eq!(db.count("SELECT COUNT(*) FROM account_redirects").await, 0);
    let accounts = db.database.accounts();
    for (id, old, new, repo) in [(ALICE, "alice", "renamed", "dotfiles"), (ACME, "acme", "neworg", "Rocket")] {
        accounts.rename(id, &new.parse().unwrap()).await.unwrap();
        assert_eq!(accounts.find_by_old_name(old).await.unwrap().unwrap().id, id);
        let moved = db.database.repositories().find_by_old_path(old, repo).await.unwrap().unwrap();
        assert_eq!(moved.owner.id, id);
        assert_eq!(moved.owner.name.as_str(), new);
    }
    // A transferred repository remains reachable after the former owner also renames.
    db.database.repositories().transfer(RepoId(5), ACME).await.unwrap();
    assert_eq!(db.database.repositories().find_by_old_path("alice", "dotfiles").await.unwrap().unwrap().full_name(), "neworg/dotfiles");
    sqlx::query("DELETE FROM accounts WHERE id = ?").bind(ALICE.0).execute(&db.raw).await.unwrap();
    assert!(accounts.find_by_old_name("alice").await.unwrap().is_none());
    assert_eq!(db.count("SELECT COUNT(*) FROM account_redirects WHERE account_id = 1").await, 0);
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
        "account_redirects",
        "issue_counters",
        "issues",
        "issue_comments",
        "issue_events",
        "labels",
        "issue_labels",
        "milestones",
        "issue_milestones",
        "issue_assignees",
        "issue_commit_refs",
        "issue_cross_refs",
        "issue_subscriptions",
        "notifications",
        "issues_fts",
        "pull_requests",
        "pull_reviews",
        "pull_review_comments",
        "pull_review_requests",
        "branch_protections",
        "branch_protection_users",
        "branch_protection_teams",
        // Milestone 3: Git LFS (L).
        "lfs_objects",
        "lfs_repo_objects",
        "lfs_locks",
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

    // Milestone 3: Git LFS (L). An object linked to a milestone 1 repository, and a lock
    // held by a milestone 1 user.
    let oid = "a".repeat(64);
    sqlx::query("INSERT INTO lfs_objects (oid, size, created_at) VALUES (?, 5, 1)").bind(&oid).execute(&db.raw).await.unwrap();
    sqlx::query("INSERT INTO lfs_repo_objects (repo_id, oid, created_at) VALUES (6, ?, 1)").bind(&oid).execute(&db.raw).await.unwrap();
    sqlx::query("INSERT INTO lfs_locks (repo_id, path, owner_id, created_at) VALUES (6, 'a.psd', 1, 1)").execute(&db.raw).await.unwrap();
    let unlinked: Option<i64> = sqlx::query_scalar("SELECT unlinked_at FROM lfs_objects").fetch_one(&db.raw).await.unwrap();
    assert_eq!(unlinked, None);

    // Deleting a milestone 1 repository cascades into the new tables.
    sqlx::query("DELETE FROM repositories WHERE id = 6").execute(&db.raw).await.unwrap();
    assert_eq!(db.count("SELECT COUNT(*) FROM webhooks").await, 0);
    assert_eq!(db.count("SELECT COUNT(*) FROM webhook_deliveries").await, 0);
    assert_eq!(db.count("SELECT COUNT(*) FROM releases").await, 0);
    assert_eq!(db.count("SELECT COUNT(*) FROM lfs_repo_objects").await, 0);
    assert_eq!(db.count("SELECT COUNT(*) FROM lfs_locks").await, 0);
    assert_eq!(db.count("SELECT COUNT(*) FROM lfs_objects WHERE unlinked_at IS NOT NULL").await, 1, "kept, now an orphan");
}

// Milestone 3: commit statuses (S).
#[tokio::test]
async fn commit_status_tables_start_empty_and_existing_rules_require_no_checks() {
    let db = upgraded().await;
    for table in ["commit_statuses", "branch_protection_required_checks"] {
        assert!(db.exists("table", table).await, "{table}");
        assert_eq!(db.count(&format!("SELECT COUNT(*) FROM {table}")).await, 0, "{table}");
    }
    sqlx::query("INSERT INTO branch_protections (repo_id, pattern, created_at, updated_at) VALUES (6, 'trunk', 1, 1)")
        .execute(&db.raw)
        .await
        .unwrap();
    assert_eq!(db.count("SELECT require_status_checks FROM branch_protections").await, 0, "off by default");
    let rules = db.database.branch_protection().list(ROCKET).await.unwrap();
    assert!(!rules[0].rule.settings.require_status_checks);
    assert_eq!(rules[0].rule.required_contexts.len(), 0);

    let insert = "INSERT INTO commit_statuses (repo_id, sha, state, context, created_at, updated_at) VALUES (6, ?, ?, 'ci', 1, 1)";
    let sha = "a".repeat(40);
    for (sha, state, valid) in [(sha.as_str(), "success", true), ("ABC", "success", false), (sha.as_str(), "passed", false)] {
        let result = sqlx::query(insert).bind(sha).bind(state).execute(&db.raw).await;
        assert_eq!(result.is_ok(), valid, "{sha} {state}");
    }
    sqlx::query("DELETE FROM repositories WHERE id = 6").execute(&db.raw).await.unwrap();
    assert_eq!(db.count("SELECT COUNT(*) FROM commit_statuses").await, 0, "statuses go with their repository");
    assert_eq!(db.count("SELECT COUNT(*) FROM branch_protections").await, 0);
}

// Milestone 3: email notifications (N).
#[tokio::test]
async fn notification_email_tables_start_empty() {
    let db = upgraded().await;
    for table in ["notification_email_settings", "notification_emails", "server_secrets"] {
        assert!(db.exists("table", table).await, "{table}");
        assert_eq!(db.count(&format!("SELECT COUNT(*) FROM {table}")).await, 0, "{table}");
    }
    let store = db.database.notification_email();
    assert_eq!(store.preference(ALICE).await.unwrap(), None, "existing users keep the configured default");

    store.set_preference(BOB, rbit_core::config::EmailPreference::Off, Timestamp(1)).await.unwrap();
    let invalid = sqlx::query("INSERT INTO notification_email_settings (account_id, email, updated_at) VALUES (1, 'sometimes', 1)")
        .execute(&db.raw)
        .await;
    assert!(invalid.is_err(), "only known preferences are stored");
    sqlx::query("DELETE FROM accounts WHERE id = 2").execute(&db.raw).await.unwrap();
    assert_eq!(db.count("SELECT COUNT(*) FROM notification_email_settings").await, 0, "deleting the account removes its preference");
}

// Milestone 3: external sign-in (O).
#[tokio::test]
async fn external_identity_table_starts_empty() {
    let db = upgraded().await;
    assert!(db.exists("table", "external_identities").await);
    assert_eq!(db.count("SELECT COUNT(*) FROM external_identities").await, 0);
    let store = db.database.external_identities();
    assert_eq!(store.list_for_account(ALICE).await.unwrap().len(), 0);
    assert!(store.has_password(ALICE).await.unwrap(), "existing users keep their passwords");

    let identity = rbit_storage::sqlite::external_identities::NewExternalIdentity {
        provider_id: "corp",
        subject: "alice",
        email: None,
        username: None,
    };
    store.link(ALICE, identity, Timestamp(1)).await.unwrap().unwrap();
    sqlx::query("DELETE FROM accounts WHERE id = 1").execute(&db.raw).await.unwrap();
    assert_eq!(db.count("SELECT COUNT(*) FROM external_identities").await, 0, "deleting the account removes its identities");
}

// Milestone 3: importing and push mirrors (M).
#[tokio::test]
async fn import_and_push_mirror_tables_start_empty() {
    let db = upgraded().await;
    for table in ["repo_imports", "repo_push_mirrors"] {
        assert!(db.exists("table", table).await, "{table}");
        assert_eq!(db.count(&format!("SELECT COUNT(*) FROM {table}")).await, 0, "{table}");
    }
    sqlx::query(
        "INSERT INTO repo_imports (repo_id, importer_id, source_kind, source_url, created_at) \
         VALUES (6, 2, 'github', 'https://github.com/a/b', 1)",
    )
    .execute(&db.raw)
    .await
    .unwrap();
    let import = db.database.imports().find(ROCKET).await.unwrap().unwrap();
    assert_eq!(import.status, rbit_core::import::ImportStatus::Queued);
    assert_eq!(import.phase, rbit_core::import::ImportPhase::Repository);
    assert_eq!(import.options, rbit_core::import::ImportOptions::default());
    assert!(import.warnings.is_empty() && import.counts == rbit_core::import::ImportCounts::default());
    let failed_without_error = sqlx::query("UPDATE repo_imports SET status = 'failed'").execute(&db.raw).await;
    assert!(failed_without_error.is_err(), "a failed import says why");

    sqlx::query(
        "INSERT INTO repo_push_mirrors (repo_id, url, next_sync_at, created_by, created_at, updated_at) \
         VALUES (6, 'https://example.com/a.git', 1, 2, 1, 1)",
    )
    .execute(&db.raw)
    .await
    .unwrap();
    let mirrors = db.database.push_mirrors().list(ROCKET).await.unwrap();
    assert_eq!(mirrors.len(), 1);
    assert!(mirrors[0].enabled && mirrors[0].sync_on_push && !mirrors[0].has_secret && mirrors[0].username.is_empty());

    sqlx::query("DELETE FROM accounts WHERE id = 2").execute(&db.raw).await.unwrap();
    assert_eq!(db.database.imports().find(ROCKET).await.unwrap().unwrap().importer, None, "the importer may be deleted");
    assert_eq!(db.database.push_mirrors().list(ROCKET).await.unwrap()[0].created_by, None);
    sqlx::query("DELETE FROM repositories WHERE id = 6").execute(&db.raw).await.unwrap();
    assert_eq!(db.count("SELECT COUNT(*) FROM repo_imports").await, 0, "imports go with their repository");
    assert_eq!(db.count("SELECT COUNT(*) FROM repo_push_mirrors").await, 0, "push mirrors go with their repository");
}

#[tokio::test]
async fn dashboard_mentions_start_empty_and_index_completion_persists() {
    let db = upgraded().await;
    assert!(db.exists("table", "issue_mentions").await);
    assert_eq!(db.count("SELECT COUNT(*) FROM issue_mentions").await, 0);
    assert!(!db.database.issue_search().mentions_indexed().await.unwrap());
    db.database.issue_search().finish_mentions_index().await.unwrap();
    assert!(db.database.issue_search().mentions_indexed().await.unwrap());
}

/// A full commit id, quoted for SQL.
const ACTIONS_SHA: &str = "'0123456789abcdef0123456789abcdef01234567'";

/// The CHECKs of the Actions tables refuse rows that contradict themselves.
async fn actions_checks_refuse_inconsistent_rows(db: &Upgraded) {
    let run = |sha: &str, event: &str, status: i64| {
        format!(
            "INSERT INTO actions_runs (repo_id, run_number, workflow_path, workflow_name, title, event, ref, sha, trusted, status, \
             event_payload, created_at) VALUES (6, 1, 'a.yml', '', '', '{event}', 'refs/heads/main', {sha}, 1, {status}, '{{}}', 1)"
        )
    };
    let refused = [
        // A runner's scope columns must agree with its kind.
        "INSERT INTO actions_runners (uuid, token_digest, name, scope_kind, owner_id, created_at) \
         VALUES ('00000000-0000-0000-0000-000000000001', zeroblob(32), 'r', 'instance', 1, 1)"
            .to_owned(),
        "INSERT INTO actions_runners (uuid, token_digest, name, scope_kind, created_at) \
         VALUES ('00000000-0000-0000-0000-000000000001', zeroblob(32), 'r', 'repository', 1)"
            .to_owned(),
        "INSERT INTO actions_runners (uuid, token_digest, name, scope_kind, created_at) VALUES ('short', zeroblob(32), 'r', 'instance', 1)"
            .to_owned(),
        "INSERT INTO actions_runners (uuid, token_digest, name, scope_kind, created_at) \
         VALUES ('00000000-0000-0000-0000-000000000001', zeroblob(31), 'r', 'instance', 1)"
            .to_owned(),
        "INSERT INTO actions_runners (uuid, token_digest, name, scope_kind, labels, created_at) \
         VALUES ('00000000-0000-0000-0000-000000000001', zeroblob(32), 'r', 'instance', '{}', 1)"
            .to_owned(),
        // Registration tokens always expire.
        "INSERT INTO actions_registration_tokens (digest, scope_kind, created_at, expires_at) VALUES (zeroblob(32), 'instance', 5, 5)"
            .to_owned(),
        // Statuses are the known codes, events the supported ones, shas full commit ids.
        run("'abc'", "push", 1),
        run(ACTIONS_SHA, "push", 9),
        run(ACTIONS_SHA, "issues", 1),
        // A repository secret is visible to its repository only; nonces have 12 bytes.
        "INSERT INTO actions_secrets (scope_kind, repo_id, name, visibility, key_id, nonce, ciphertext, created_at, updated_at) \
         VALUES ('repository', 6, 'X', 'selected', zeroblob(8), zeroblob(12), zeroblob(16), 1, 1)"
            .to_owned(),
        "INSERT INTO actions_secrets (scope_kind, owner_id, name, key_id, nonce, ciphertext, created_at, updated_at) \
         VALUES ('owner', 10, 'X', zeroblob(8), zeroblob(11), zeroblob(16), 1, 1)"
            .to_owned(),
        "INSERT INTO actions_repositories (repo_id, token_permission, updated_at) VALUES (6, 'admin', 1)".to_owned(),
    ];
    for sql in refused {
        assert!(sqlx::query(sqlx::AssertSqlSafe(sql.clone())).execute(&db.raw).await.is_err(), "{sql}");
    }
    assert!(sqlx::query(sqlx::AssertSqlSafe(run(ACTIONS_SHA, "push", 1))).execute(&db.raw).await.is_ok(), "the template itself is valid");
    sqlx::query("DELETE FROM actions_runs").execute(&db.raw).await.unwrap();
}

// Actions: foundation (F).
#[tokio::test]
async fn actions_tables_start_empty_with_their_checks_and_cascades() {
    let db = upgraded().await;
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' AND name GLOB 'actions_*' ORDER BY name")
            .fetch_all(&db.raw)
            .await
            .unwrap();
    assert_eq!(
        tables,
        [
            "actions_artifacts",
            "actions_job_labels",
            "actions_job_outputs",
            "actions_jobs",
            "actions_owner_policies",
            "actions_registration_tokens",
            "actions_repositories",
            "actions_run_numbers",
            "actions_runners",
            "actions_runs",
            "actions_schedules",
            "actions_secret_repositories",
            "actions_secrets",
            "actions_steps",
            "actions_tasks",
            "actions_trusted_users",
            "actions_variables",
        ]
    );
    for table in &tables {
        assert_eq!(db.count(&format!("SELECT COUNT(*) FROM {table}")).await, 0, "{table}");
    }
    assert!(!db.database.actions_settings().is_enabled(ROCKET).await.unwrap(), "no row: Actions is off");

    actions_checks_refuse_inconsistent_rows(&db).await;

    sqlx::raw_sql(
        "INSERT INTO actions_repositories (repo_id, enabled, updated_at) VALUES (6, 1, 1); \
         INSERT INTO actions_runners (id, uuid, token_digest, name, scope_kind, repo_id, created_at) \
             VALUES (1, '00000000-0000-0000-0000-000000000001', zeroblob(32), 'r', 'repository', 6, 1); \
         INSERT INTO actions_runs (id, repo_id, run_number, workflow_path, workflow_name, title, event, ref, sha, actor_id, trusted, \
             status, event_payload, created_at) VALUES (1, 6, 1, 'a.yml', 'CI', 'x', 'push', 'refs/heads/trunk', \
             '0123456789abcdef0123456789abcdef01234567', 2, 1, 6, '{}', 1); \
         INSERT INTO actions_run_numbers (repo_id, last) VALUES (6, 1); \
         INSERT INTO actions_jobs (id, run_id, repo_id, job_key, name, payload, status, runner_id, created_at) \
             VALUES (1, 1, 6, 'build', 'build', x'00', 6, 1, 1); \
         INSERT INTO actions_job_labels (job_id, label) VALUES (1, 'linux'); \
         INSERT INTO actions_job_outputs (job_id, key, value) VALUES (1, 'version', '1'); \
         INSERT INTO actions_tasks (id, job_id, attempt, runner_id, token_digest, status, started_at, heartbeat_at) \
             VALUES (1, 1, 1, 1, zeroblob(32), 6, 1, 1); \
         UPDATE actions_jobs SET task_id = 1 WHERE id = 1; \
         INSERT INTO actions_steps (task_id, number, name, status) VALUES (1, 0, 'Checkout', 1); \
         INSERT INTO actions_secrets (id, scope_kind, owner_id, name, visibility, key_id, nonce, ciphertext, created_at, updated_at, \
             updated_by) VALUES (1, 'owner', 10, 'TOKEN', 'selected', zeroblob(8), zeroblob(12), zeroblob(16), 1, 1, 2); \
         INSERT INTO actions_secret_repositories (secret_id, repo_id) VALUES (1, 6); \
         INSERT INTO actions_variables (scope_kind, repo_id, name, value, created_at, updated_at) VALUES ('repository', 6, 'A', 'b', 1, 1); \
         INSERT INTO actions_trusted_users (repo_id, user_id, approved_by, created_at) VALUES (6, 2, 1, 1); \
         INSERT INTO actions_owner_policies (owner_id, instance_runners, updated_by, updated_at) VALUES (10, 1, 2, 1); \
         INSERT INTO actions_registration_tokens (digest, scope_kind, repo_id, created_by, created_at, expires_at) \
             VALUES (zeroblob(32), 'repository', 6, 2, 1, 2); \
         INSERT INTO actions_schedules (repo_id, workflow_path, cron, next_at) VALUES (6, 'a.yml', '@daily', 1);          INSERT INTO actions_artifacts (run_id, repo_id, attempt, job_id, name, size, sha256, created_at, expires_at)              VALUES (1, 6, 1, 1, 'dist', 3, zeroblob(32), 1, 2);",
    )
    .execute(&db.raw)
    .await
    .unwrap();

    // Removing the runner keeps the job and task; removing a person keeps what they did.
    sqlx::query("DELETE FROM actions_runners WHERE id = 1").execute(&db.raw).await.unwrap();
    assert_eq!(db.count("SELECT COUNT(*) FROM actions_tasks WHERE runner_id IS NULL").await, 1);
    assert_eq!(db.count("SELECT COUNT(*) FROM actions_jobs WHERE runner_id IS NULL").await, 1);
    sqlx::query("DELETE FROM accounts WHERE id = 2").execute(&db.raw).await.unwrap();
    assert_eq!(db.count("SELECT COUNT(*) FROM actions_runs WHERE actor_id IS NULL").await, 1);
    assert_eq!(db.count("SELECT COUNT(*) FROM actions_secrets WHERE updated_by IS NULL").await, 1);
    assert_eq!(db.count("SELECT COUNT(*) FROM actions_registration_tokens WHERE created_by IS NULL").await, 1);
    assert_eq!(db.count("SELECT COUNT(*) FROM actions_owner_policies WHERE updated_by IS NULL").await, 1);
    assert_eq!(db.count("SELECT COUNT(*) FROM actions_trusted_users").await, 0, "a deleted user is no longer trusted");

    // Deleting the repository removes everything Actions kept for it but its owner's.
    sqlx::query("DELETE FROM repositories WHERE id = 6").execute(&db.raw).await.unwrap();
    for table in &tables {
        let expected = i64::from(table == "actions_secrets" || table == "actions_owner_policies");
        assert_eq!(db.count(&format!("SELECT COUNT(*) FROM {table}")).await, expected, "{table}");
    }
    assert!(sqlx::query("PRAGMA foreign_key_check").fetch_all(&db.raw).await.unwrap().is_empty());
    sqlx::query("DELETE FROM accounts WHERE id = 10").execute(&db.raw).await.unwrap();
    assert_eq!(db.count("SELECT COUNT(*) FROM actions_secrets").await, 0, "owner secrets go with their owner");
    assert_eq!(db.count("SELECT COUNT(*) FROM actions_owner_policies").await, 0);
}
