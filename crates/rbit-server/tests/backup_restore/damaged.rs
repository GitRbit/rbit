//! Restores refuse what they must not put into service: interrupted, torn or newer
//! backups, and data directories that are not empty. An interrupted restore keeps every
//! other command away until a restore finishes.

use std::path::{Path, PathBuf};

use sqlx::{ConnectOptions, Connection};

use super::{
    consistency,
    instance::{self, failed_with, rbit, sha256, succeeded},
    round_trip::manifest,
};

const MARKER: &str = ".rbit-restore-incomplete";

/// A name, a way to damage a backup, and what the refusal says.
type Damage = (&'static str, Box<dyn Fn(&Path)>, &'static str);

/// A data directory and a complete backup of it.
async fn backed_up(dir: &Path) -> (PathBuf, PathBuf) {
    let (data, backup) = (dir.join("data"), dir.join("backup"));
    Box::pin(instance::populate(&data)).await;
    succeeded(&rbit(&data, &["backup", "--output", backup.to_str().unwrap()], &[]), "backup");
    (data, backup)
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &to.join(entry.file_name()));
        } else {
            std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
    }
}

fn find(dir: &Path, matches: &dyn Fn(&Path) -> bool) -> PathBuf {
    search(dir, matches).unwrap_or_else(|| panic!("nothing matches under {}", dir.display()))
}

fn search(dir: &Path, matches: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().path()).collect();
    entries.sort();
    entries.into_iter().find_map(|path| if path.is_dir() { search(&path, matches) } else { matches(&path).then_some(path) })
}

fn overwrite(path: &Path, change: impl FnOnce(&mut Vec<u8>)) {
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    #[expect(clippy::permissions_set_readonly_false, reason = "a test damages a Git pack on purpose")]
    permissions.set_readonly(false);
    std::fs::set_permissions(path, permissions).unwrap();
    let mut bytes = std::fs::read(path).unwrap();
    change(&mut bytes);
    std::fs::write(path, bytes).unwrap();
}

#[tokio::test]
async fn interrupted_and_torn_backups_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (_, backup) = Box::pin(backed_up(dir.path())).await;
    let ends = |suffix: &'static str| move |path: &Path| path.to_string_lossy().ends_with(suffix);
    let damages: Vec<Damage> = vec![
        ("no manifest", Box::new(move |b: &Path| std::fs::remove_file(b.join("manifest.json")).unwrap()), "is not a complete backup"),
        ("torn manifest", Box::new(move |b: &Path| overwrite(&b.join("manifest.json"), |bytes| bytes.truncate(10))), "is not valid"),
        (
            "truncated database",
            Box::new(move |b: &Path| overwrite(&b.join("rbit.db"), |bytes| bytes.truncate(bytes.len() / 2))),
            "does not match the digest",
        ),
        (
            "torn pack",
            Box::new(move |b: &Path| {
                overwrite(&find(&b.join("repositories"), &ends(".pack")), |bytes| {
                    let middle = bytes.len() / 2;
                    bytes[middle] ^= 0xff;
                });
            }),
            "fails git fsck",
        ),
        (
            "lost repository",
            Box::new(move |b: &Path| std::fs::remove_dir_all(find(&b.join("repositories"), &ends("HEAD")).parent().unwrap()).unwrap()),
            "is missing",
        ),
        (
            "changed LFS object",
            Box::new(move |b: &Path| overwrite(&find(&b.join("lfs"), &|_| true), |bytes| bytes[0] ^= 1)),
            "does not match the size and digest",
        ),
        (
            "lost release file",
            Box::new(move |b: &Path| std::fs::remove_file(find(&b.join("attachments"), &|_| true)).unwrap()),
            "is missing",
        ),
        (
            "truncated artifact",
            Box::new(move |b: &Path| {
                overwrite(&find(&b.join("actions").join("artifacts"), &|_| true), |bytes| {
                    bytes.pop();
                });
            }),
            "does not match",
        ),
        (
            "lost log",
            Box::new(move |b: &Path| std::fs::remove_file(find(&b.join("actions").join("logs"), &ends(".log.gz"))).unwrap()),
            "is missing",
        ),
    ];
    for (name, damage, message) in damages {
        let copy = dir.path().join(format!("backup-{}", name.replace(' ', "-")));
        copy_tree(&backup, &copy);
        damage(&copy);
        let target = dir.path().join(format!("restored-{}", name.replace(' ', "-")));
        let output = rbit(&target, &["restore", "--from", copy.to_str().unwrap()], &[]);
        failed_with(&output, message);
        if matches!(name, "no manifest" | "torn manifest") {
            assert!(!target.exists(), "{name}: nothing is written before the manifest is read");
        } else {
            assert!(target.join(MARKER).exists(), "{name}: the partial restore stays marked");
        }
    }
}

#[tokio::test]
async fn a_backup_from_a_newer_rbit_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (_, backup) = Box::pin(backed_up(dir.path())).await;

    let newer_format = dir.path().join("newer-format");
    copy_tree(&backup, &newer_format);
    let mut document = manifest(&newer_format);
    document["format"] = 6.into();
    std::fs::write(newer_format.join("manifest.json"), serde_json::to_vec(&document).unwrap()).unwrap();
    let target = dir.path().join("restored-format");
    failed_with(&rbit(&target, &["restore", "--from", newer_format.to_str().unwrap()], &[]), "newer than this rbit reads");
    assert!(!target.exists());

    // A database with a migration this binary does not embed, under a manifest that vouches for it.
    let newer_schema = dir.path().join("newer-schema");
    copy_tree(&backup, &newer_schema);
    let mut connection = sqlx::sqlite::SqliteConnectOptions::new().filename(newer_schema.join("rbit.db")).connect().await.unwrap();
    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (99999, 'future', 1, x'00', 0)",
    )
    .execute(&mut connection)
    .await
    .unwrap();
    connection.close().await.unwrap();
    let mut document = manifest(&newer_schema);
    document["database_sha256"] = hex(&sha256(&std::fs::read(newer_schema.join("rbit.db")).unwrap())).into();
    std::fs::write(newer_schema.join("manifest.json"), serde_json::to_vec(&document).unwrap()).unwrap();
    let target = dir.path().join("restored-schema");
    failed_with(&rbit(&target, &["restore", "--from", newer_schema.to_str().unwrap()], &[]), "written by a newer rbit");

    // Copied into place by hand, the same database is refused at start-up too.
    let by_hand = dir.path().join("by-hand");
    std::fs::create_dir_all(&by_hand).unwrap();
    for file in ["rbit.db", "secret.key"] {
        std::fs::copy(newer_schema.join(file), by_hand.join(file)).unwrap();
    }
    failed_with(&rbit(&by_hand, &["migrate"], &[]), "99999");
}

fn hex(digest: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    digest.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

fn listing(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_owned()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
    }
    files.sort();
    files
}

#[tokio::test]
async fn a_restore_never_writes_into_a_directory_that_is_not_empty() {
    let dir = tempfile::tempdir().unwrap();
    let (data, backup) = Box::pin(backed_up(dir.path())).await;
    let before = listing(&data);
    failed_with(&rbit(&data, &["restore", "--from", backup.to_str().unwrap()], &[]), "is not empty");
    assert_eq!(listing(&data), before, "the instance is untouched");

    let stray = dir.path().join("stray");
    std::fs::create_dir_all(&stray).unwrap();
    std::fs::write(stray.join("notes.txt"), "keep me").unwrap();
    failed_with(&rbit(&stray, &["restore", "--from", backup.to_str().unwrap()], &[]), "is not empty");
    assert_eq!(listing(&stray).len(), 1);

    let empty = dir.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    succeeded(&rbit(&empty, &["restore", "--from", backup.to_str().unwrap()], &[]), "restore into an empty directory");
}

#[tokio::test]
async fn an_interrupted_restore_blocks_the_directory_until_a_restore_finishes() {
    let dir = tempfile::tempdir().unwrap();
    let (_, backup) = Box::pin(backed_up(dir.path())).await;
    let torn = dir.path().join("torn");
    copy_tree(&backup, &torn);
    // The LFS objects are copied after the database, the key and the repositories.
    overwrite(&find(&torn.join("lfs"), &|_| true), |bytes| bytes[0] ^= 1);
    let target = dir.path().join("restored");
    failed_with(&rbit(&target, &["restore", "--from", torn.to_str().unwrap()], &[]), "is damaged");
    assert!(target.join(MARKER).exists() && target.join("rbit.db").exists() && target.join("repositories").exists());

    for command in [&["migrate"][..], &["serve"], &["backup", "--output", dir.path().join("b").to_str().unwrap()]] {
        failed_with(&rbit(&target, command, &[]), "did not finish");
    }
    succeeded(&rbit(&target, &["restore", "--from", backup.to_str().unwrap()], &[]), "the second restore");
    assert!(!target.join(MARKER).exists());
    consistency::check(&target).await;
    succeeded(&rbit(&target, &["migrate"], &[]), "migrate");
}
