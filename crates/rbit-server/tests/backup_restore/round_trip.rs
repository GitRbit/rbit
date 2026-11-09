//! A backup of a realistic instance restores into a fresh data directory with every
//! repository, stored file, row and sealed credential, and nothing the policy leaves out.

use std::path::Path;

use rbit_storage::{error::StorageError, sqlite::Database};

use super::{
    consistency,
    instance::{self, ACTIONS_SECRET, SERVER_SECRET, TOTP_SEED, failed_with, git, git_dir, rbit, succeeded},
};

pub fn manifest(backup: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(backup.join("manifest.json")).unwrap()).unwrap()
}

#[tokio::test]
async fn a_backup_restores_repositories_files_rows_and_sealed_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let (data, backup, restored) = (dir.path().join("data"), dir.path().join("backup"), dir.path().join("restored"));
    let seeded = Box::pin(instance::populate(&data)).await;
    let open = instance::open(&data).await;
    let session = open.services.accounts.start_session(&open.ada().await).await.unwrap();
    assert!(open.services.accounts.resume_session(&session).await.unwrap().is_some());
    open.close().await;
    let refs = git(dir.path(), &[&git_dir(&seeded.project), "show-ref"]);

    succeeded(&rbit(&data, &["backup", "--output", backup.to_str().unwrap()], &[]), "backup");
    let manifest = manifest(&backup);
    assert_eq!(manifest["format"], 6);
    assert_eq!((manifest["wikis"].as_u64(), manifest["missing_wikis"].clone()), (Some(1), serde_json::json!([])));
    assert!(manifest["wiki_objects"].as_u64().unwrap() >= 8, "commits, trees and page blobs");
    let counts = ["repositories", "attachments", "lfs_objects", "artifacts", "caches", "logs"].map(|key| manifest[key].as_u64().unwrap());
    assert_eq!(counts, [2, 1, 2, 1, 1, 4], "two log indexes, a compressed and a live log");
    for key in ["missing_repositories", "missing_attachments", "missing_lfs_objects", "missing_artifacts", "missing_caches", "missing_logs"]
    {
        assert_eq!(manifest[key], serde_json::json!([]), "{key}");
    }
    assert_eq!(manifest["database_sha256"].as_str().unwrap().len(), 64);
    assert!(manifest["actions_secrets_key"].as_str().unwrap().starts_with("not included"));

    let output = rbit(&restored, &["restore", "--from", backup.to_str().unwrap()], &[]);
    succeeded(&output, "restore");
    assert!(String::from_utf8_lossy(&output.stdout).contains("Actions secrets are sealed with a key no backup holds"));
    assert!(!restored.join(".rbit-restore-incomplete").exists());
    assert!(!restored.join("rbit.redb").exists(), "sessions are not restored");

    let found = consistency::check(&restored).await;
    assert_eq!((found.repositories, found.releases, found.assets, found.lfs), (2, 1, 1, 2));
    assert_eq!((found.artifacts, found.caches, found.logs, found.wikis), (1, 1, 2, 1));
    let relative = |path: &Path| restored.join(path.strip_prefix(&data).unwrap());
    assert_eq!(git(dir.path(), &[&git_dir(&relative(&seeded.project)), "show-ref"]), refs, "every branch and tag");
    assert_eq!(std::fs::read_to_string(relative(&seeded.empty).join("HEAD")).unwrap(), "ref: refs/heads/trunk\n");
    for (path, bytes) in &seeded.files {
        assert_eq!(&std::fs::read(restored.join(path)).unwrap(), bytes, "{}", path.display());
    }
    for (oid, bytes) in &seeded.lfs {
        let (first, second) = oid.shards();
        assert_eq!(&std::fs::read(restored.join("lfs").join(first).join(second).join(oid.as_str())).unwrap(), bytes);
    }

    // The restored instance starts; sealed credentials open with the restored key; the
    // session from before the backup is gone; Actions secrets need their separate key.
    succeeded(&rbit(&restored, &["migrate"], &[]), "migrate");
    let open = instance::open(&restored).await;
    let ada = open.ada().await;
    assert_eq!(open.database.server_secrets().get_or_insert("probe", b"other").await.unwrap(), SERVER_SECRET);
    assert_eq!(open.database.two_factor().find(ada.id()).await.unwrap().unwrap().secret, TOTP_SEED);
    assert!(open.services.accounts.resume_session(&session).await.unwrap().is_none(), "everyone signs in again");
    wiki_restored(&open, &ada).await;
    let actions = open.services.actions.as_ref().unwrap();
    let run = open.database.actions_runs().run(rbit_core::actions::RunId(seeded.run)).await.unwrap().unwrap();
    assert!(actions.task_secrets(&run).await.is_err(), "no Actions key, no secrets");
    open.close().await;
    let key = Path::new("actions").join("secrets.key");
    std::fs::copy(data.join(&key), restored.join(&key)).unwrap();
    let open = instance::open(&restored).await;
    let actions = open.services.actions.as_ref().unwrap();
    assert_eq!(actions.task_secrets(&run).await.unwrap().get("DEPLOY").map(String::as_str), Some(ACTIONS_SECRET));
    open.close().await;
}

/// The wiki's pages, rename alias, deleted page and its history came back.
async fn wiki_restored(open: &instance::Open, ada: &rbit_core::User) {
    use rbit_services::wikis::pages::WikiLookup;
    let repo = open.services.repositories.open(Some(ada), "ada", "project").await.unwrap();
    let wikis = &open.services.wikis;
    let home = wikis.page(&repo, Some(ada), &"Home".parse().unwrap()).await.unwrap();
    assert!(matches!(&home, WikiLookup::Page(view) if view.content.as_str() == "# Project wiki\n"), "{home:?}");
    let old = wikis.page(&repo, Some(ada), &"Notes".parse().unwrap()).await.unwrap();
    assert!(matches!(&old, WikiLookup::Moved(path) if path.as_str() == "Guide/Notes"), "{old:?}");
    let deleted = wikis.deleted(&repo, Some(ada), None).await.unwrap();
    assert_eq!(deleted.entries.iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>(), ["Draft"]);
    let draft = &deleted.entries[0];
    let restored = wikis.undelete(&repo, ada, draft.id, draft.deleted).await.unwrap();
    let WikiLookup::Page(view) = wikis.page(&repo, Some(ada), &restored).await.unwrap() else { panic!("not restored") };
    assert_eq!(view.content.as_str(), "draft\n");
}

#[tokio::test]
async fn sealed_credentials_need_the_key_that_sealed_them() {
    let dir = tempfile::tempdir().unwrap();
    let (data, backup) = (dir.path().join("data"), dir.path().join("backup"));
    Box::pin(instance::populate(&data)).await;
    let kept = dir.path().join("kept.key");
    std::fs::copy(data.join("secret.key"), &kept).unwrap();
    let other = dir.path().join("other.key");
    rbit_storage::sealing::SealingKey::create(&other).unwrap();
    succeeded(&rbit(&data, &["backup", "--output", backup.to_str().unwrap()], &[("RBIT_STORAGE__SECRET_KEY_FILE", &kept)]), "backup");
    assert!(!backup.join("secret.key").exists(), "a separately kept key stays separate");

    let missing = dir.path().join("missing");
    let output = rbit(&missing, &["restore", "--from", backup.to_str().unwrap()], &[]);
    failed_with(&output, "is missing, but the database holds values sealed with it");
    let wrong = dir.path().join("wrong");
    let output = rbit(&wrong, &["restore", "--from", backup.to_str().unwrap()], &[("RBIT_STORAGE__SECRET_KEY_FILE", &other)]);
    failed_with(&output, "does not match the values sealed in the database");

    let restored = dir.path().join("restored");
    let output = rbit(&restored, &["restore", "--from", backup.to_str().unwrap()], &[("RBIT_STORAGE__SECRET_KEY_FILE", &kept)]);
    succeeded(&output, "restore with the kept key");
    let database = Database::open_with_key_file(&restored.join("rbit.db"), &kept).await.unwrap();
    assert_eq!(database.server_secrets().get_or_insert("probe", b"other").await.unwrap(), SERVER_SECRET);
    database.close().await;
    // Starting without the key never makes a new one that would orphan the sealed values.
    assert!(matches!(Database::open(&restored.join("rbit.db")).await, Err(StorageError::MissingKey(_))));
    assert!(!restored.join("secret.key").exists());
}
