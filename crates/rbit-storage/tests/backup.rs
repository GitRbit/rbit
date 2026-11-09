//! The database side of `rbit backup` and `rbit restore` (`rbit_storage::sqlite::backup`)
//! against a real, migrated database. The end-to-end backup and restore suites in
//! `rbit-server` drive the same functions through the executable.

use std::path::{Path, PathBuf};

use rbit_core::{AccountId, RepoId, Visibility, lfs::LfsOid};
use rbit_storage::{
    Database, NewRepository, NewUser,
    sqlite::backup::{
        self, BackupError, FORMAT_VERSION,
        inventory::{self, Kind},
        prune::{Missing, prune},
        wikis,
    },
};

use super::writer_connection::pool;

/// A live database with one account, two repositories and one LFS object per repository.
async fn live(dir: &Path) -> (PathBuf, AccountId, [RepoId; 2], [String; 2]) {
    let path = dir.join("live.db");
    let database = Database::open(&path).await.unwrap();
    let new = NewUser {
        name: "alice".parse().unwrap(),
        email: "alice@example.com".parse().unwrap(),
        display_name: None,
        password_hash: "not-a-real-hash".to_owned(),
        is_admin: false,
    };
    let alice = database.accounts().create_user(new).await.unwrap().id();
    let mut repositories = Vec::new();
    for name in ["kept", "gone"] {
        let new = NewRepository {
            owner_id: alice,
            name: name.parse().unwrap(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".to_owned(),
        };
        repositories.push(database.repositories().create(new).await.unwrap().id);
    }
    let oids = ["a".repeat(64), "b".repeat(64)];
    for (oid, repo) in oids.iter().zip(&repositories) {
        database.lfs().record_object(&LfsOid::parse(oid).unwrap(), 5, *repo).await.unwrap();
    }
    database.close().await;
    (path, alice, [repositories[0], repositories[1]], oids)
}

async fn snapshot(dir: &Path, live: &Path) -> PathBuf {
    let snapshot = dir.join("rbit.db");
    std::fs::File::create(&snapshot).unwrap();
    backup::snapshot_into(live, &snapshot).await.unwrap();
    snapshot
}

async fn count(path: &Path, sql: &'static str) -> i64 {
    let pool = pool(path).await;
    let count = sqlx::query_scalar(sql).fetch_one(&pool).await.unwrap();
    pool.close().await;
    count
}

#[tokio::test]
async fn the_snapshot_lists_what_its_format_holds() {
    let dir = tempfile::tempdir().unwrap();
    let (live, alice, [kept, gone], oids) = live(dir.path()).await;
    let snapshot = snapshot(dir.path(), &live).await;
    assert_eq!(FORMAT_VERSION, 6, "the backup layout is unchanged");

    let current = inventory::read(&snapshot, FORMAT_VERSION).await.unwrap();
    assert_eq!(current.accounts, 1);
    assert_eq!(current.repositories, vec![(kept, alice), (gone, alice)]);
    let lfs: Vec<_> = current.blobs.iter().filter(|blob| blob.kind == Kind::Lfs).collect();
    assert_eq!(lfs.len(), 2);
    assert_eq!(lfs[0].id, oids[0]);
    assert_eq!(lfs[0].path, Path::new("lfs").join("aa").join("aa").join(&oids[0]));
    assert_eq!((lfs[0].size, lfs[0].sha256), (Some(5), Some([0xaa; 32])));

    // Format 2 predates LFS objects in backups: restoring it lists none.
    let old = inventory::read(&snapshot, 2).await.unwrap();
    assert!(old.blobs.iter().all(|blob| blob.kind == Kind::Attachment));
    assert_eq!((Kind::Attachment.since_format(), Kind::Lfs.since_format(), Kind::Log.since_format()), (2, 3, 5));

    assert_eq!(backup::sealed_actions_secrets(&snapshot).await.unwrap(), 0);
    let catalog = wikis::read(&snapshot).await.unwrap();
    assert!(catalog.wikis.is_empty());
    assert_eq!(wikis::forget_pages(&snapshot).await.unwrap(), 0);
    assert_eq!(wikis::SINCE_FORMAT, FORMAT_VERSION);
}

#[tokio::test]
async fn pruning_removes_exactly_what_the_backup_lacks() {
    let dir = tempfile::tempdir().unwrap();
    let (live, _, [kept, gone], oids) = live(dir.path()).await;
    let snapshot = snapshot(dir.path(), &live).await;

    // Nothing missing opens nothing, not even a missing file.
    prune(&dir.path().join("absent.db"), &Missing::default()).await.unwrap();

    let repositories = [gone.0];
    let lfs_objects = [oids[0].clone()];
    prune(&snapshot, &Missing { repositories: &repositories, lfs_objects: &lfs_objects, ..Missing::default() }).await.unwrap();
    let left = inventory::read(&snapshot, FORMAT_VERSION).await.unwrap();
    assert_eq!(left.repositories.iter().map(|(repo, _)| *repo).collect::<Vec<_>>(), vec![kept]);
    assert_eq!(count(&snapshot, "SELECT COUNT(*) FROM lfs_repo_objects").await, 0, "both cascades removed their links");
    assert_eq!(left.blobs.iter().map(|blob| blob.id.as_str()).collect::<Vec<_>>(), vec![oids[1].as_str()]);
    // The live database is untouched.
    assert_eq!(count(&live, "SELECT COUNT(*) FROM repositories").await, 2);
}

#[tokio::test]
async fn failures_keep_their_messages() {
    let dir = tempfile::tempdir().unwrap();
    let absent = dir.path().join("absent.db");
    let error = backup::snapshot_into(&absent, &dir.path().join("rbit.db")).await.unwrap_err();
    assert!(matches!(error, BackupError::Open { .. }));
    assert_eq!(error.to_string(), format!("cannot open {}", absent.display()));
    assert_eq!(
        BackupError::PruneRepository(7, sqlx::Error::RowNotFound).to_string(),
        "cannot leave repository 7, deleted during the backup, out of it; take the backup again"
    );
}
