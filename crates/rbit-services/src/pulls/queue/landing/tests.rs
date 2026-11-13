//! Exercise the real landing boundary after its earlier User snapshot goes stale.
use super::*;
use crate::{CreateRepository, Services, pulls::NewPullInput};
use rbit_core::{Config, IssueNumber, Visibility};
use rbit_storage::{Database, KvStore, NewUser, sqlite::actions_runs::NewEntry};
use std::{path::Path, process::Command, sync::Arc};

fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=Alice", "-c", "user.email=alice@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

async fn landing_after_authority_snapshot(suspend: bool) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = root.join("data");
    std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
    let user = database
        .accounts()
        .create_user(NewUser {
            name: "alice".parse().unwrap(),
            email: "alice@example.com".parse().unwrap(),
            display_name: None,
            password_hash: "unused".into(),
            is_admin: false,
        })
        .await
        .unwrap();
    services
        .repositories
        .create(&user, CreateRepository { name: "demo".into(), description: String::new(), visibility: Visibility::Public })
        .await
        .unwrap();
    let repo = services.repositories.open(Some(&user), "alice", "demo").await.unwrap();
    let work = root.join("work");
    std::fs::create_dir(&work).unwrap();
    git(&work, &["init", "-q"]);
    std::fs::write(work.join("README.md"), "base\n").unwrap();
    git(&work, &["add", "."]);
    git(&work, &["commit", "-qm", "base"]);
    let base = git(&work, &["rev-parse", "HEAD"]);
    git(&work, &["push", "-q", repo.disk_path.to_str().unwrap(), "main"]);
    git(&work, &["checkout", "-qb", "feature"]);
    std::fs::write(work.join("README.md"), "feature\n").unwrap();
    git(&work, &["commit", "-qam", "feature"]);
    let candidate = git(&work, &["rev-parse", "HEAD"]);
    git(&work, &["push", "-q", repo.disk_path.to_str().unwrap(), "feature"]);
    let pull = services
        .pulls
        .create(&user, &repo, &repo, NewPullInput { base_branch: "main".into(), head_branch: "feature".into(), ..NewPullInput::default() })
        .await
        .unwrap()
        .0;
    let ctx = services.pulls.open(Some(&user), repo, IssueNumber::new(pull.number.get()).unwrap()).await.unwrap();
    let new = NewEntry {
        repo: ctx.repo().repo.repo.id,
        issue: pull.id,
        number: pull.number.get(),
        base_branch: "main",
        head_sha: &candidate,
        strategy: "merge",
        queued_by: user.id(),
    };
    let entry = database.actions_runs().queue_pull(&new, Timestamp::now()).await.unwrap().unwrap();
    database.actions_runs().queue_testing(entry.id, &candidate, &base, Timestamp::now()).await.unwrap();
    // User and repository authority were captured while the account was active.
    // Suspend only after the landing method has acquired its repository lease.
    services
        .pulls
        .land_entry_after(&user, &ctx, &entry, (&candidate, &base), async {
            if suspend {
                database.admin().set_suspended(user.id(), true).await.unwrap();
            }
        })
        .await
        .unwrap();
    let entry = database.actions_runs().queue_entry(entry.id).await.unwrap().unwrap();
    let tip = ctx.repo().reader.commit_id("refs/heads/main".into()).await.unwrap();
    let pull = database.pulls().find(pull.id).await.unwrap().unwrap();
    if suspend {
        assert_eq!(entry.state, "failed", "a stale active-user snapshot must not authorize landing");
        assert!(entry.reason.unwrap().contains("suspended"));
        assert_eq!(tip, base);
        assert!(!pull.is_merged());
    } else {
        assert_eq!(entry.state, "landed", "an active queuer still lands the tested candidate");
        assert_eq!(tip, candidate);
        assert!(pull.is_merged());
    }
    services.pulls.shutdown().await;
}

#[tokio::test]
async fn suspension_after_the_earlier_lookup_is_rechecked_at_landing() {
    landing_after_authority_snapshot(true).await;
}

#[tokio::test]
async fn an_active_queuer_still_lands_the_exact_candidate() {
    landing_after_authority_snapshot(false).await;
}
