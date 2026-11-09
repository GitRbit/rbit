//! `rbit restore --from DIR`: puts a backup into service as the configured data directory,
//! after checking that it is complete, intact and readable by this binary.
//!
//! * The backup must be complete (its `manifest.json` exists) and of a format this binary
//!   knows; a backup written by a newer rbit, or whose database records migrations this
//!   binary does not embed, is refused before anything is written.
//! * The data directory must be empty or absent: restoring never overwrites or merges with
//!   an instance. Move the old one aside first.
//! * A marker file ([`MARKER`]) is written first and removed last, after every restored
//!   file is synced. While it exists, every other command refuses the directory, and the
//!   next restore wipes what an interrupted one left and starts over.
//! * Every file is checked as it is copied: the database against the digest in the
//!   manifest and with `PRAGMA quick_check`, every repository with `git fsck`, every
//!   stored file against the size and SHA-256 its row records, every object the wiki
//!   catalog's revisions reach by re-hashing it. Anything the database
//!   refers to and the backup lacks fails the restore (a backup in format 5 or later never
//!   lacks anything; older ones list what they lacked, which is tolerated with a warning).
//! * The secret key is restored from the backup, or taken from `[storage]
//!   secret_key_file`, and must open the credentials sealed in the database. The Actions
//!   secrets key is never in a backup: a warning says so when secrets need it.
//! * The key/value store is not restored: everyone signs in again.
//! * A backup older than format 6 holds no wiki objects: its wikis are restored without
//!   their pages (a warning says how many), so no catalog row refers to a missing object.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use rbit_core::Config;
use rbit_storage::{
    error::StorageError,
    sqlite::{
        backup::{
            self as stored,
            inventory::{self, Blob, Kind},
        },
        snapshot,
    },
};

use super::{FORMAT_VERSION, Manifest, SECRET_KEY_FILE, copy, durable, wikis};

/// Present in a data directory while a restore into it is incomplete.
pub const MARKER: &str = ".rbit-restore-incomplete";

/// Whether a restore into `data_dir` was interrupted.
pub fn interrupted(data_dir: &Path) -> bool {
    data_dir.join(MARKER).exists()
}

/// Fails when a restore into the configured data directory was interrupted: what it left
/// is incomplete and must not be served, migrated or backed up.
pub fn refuse_interrupted(config: &Config) -> anyhow::Result<()> {
    if interrupted(&config.storage.data_dir) {
        bail!(
            "a restore into {} did not finish; run `rbit restore --from <backup>` again, or delete the directory",
            config.storage.data_dir.display()
        );
    }
    Ok(())
}

/// What a restore put into place.
#[derive(Debug, Default)]
pub struct Restored {
    pub repositories: u64,
    pub files: u64,
    pub wikis: u64,
    pub warnings: Vec<String>,
}

pub async fn run(config: Config, from: PathBuf) -> anyhow::Result<()> {
    let restored = restore(&config, &from).await?;
    for warning in &restored.warnings {
        tracing::warn!("{warning}");
    }
    tracing::info!(
        data_dir = %config.storage.data_dir.display(),
        repositories = restored.repositories,
        files = restored.files,
        wikis = restored.wikis,
        "restore complete; start rbit, and everyone signs in again"
    );
    Ok(())
}

/// Restores the backup at `from` into `config`'s data directory.
pub async fn restore(config: &Config, from: &Path) -> anyhow::Result<Restored> {
    let manifest = read_manifest(from).await?;
    let target = &config.storage.data_dir;
    prepare(target).await?;
    let mut restored = Restored::default();

    let database = config.storage.sqlite_path();
    tokio::fs::copy(from.join(&manifest.database), &database).await.context("cannot copy the database")?;
    check_database(&manifest, &database).await?;
    restore_key(config, from, &database).await?;

    let inventory = inventory::read(&database, manifest.format).await?;
    for &(repo, owner) in &inventory.repositories {
        let relative = Path::new("repositories").join(owner.0.to_string()).join(format!("{}.git", repo.0));
        if !from.join(&relative).join("HEAD").is_file() {
            if manifest.missing_repositories.contains(&repo.0) {
                restored.warnings.push(format!("repository {} was missing when the backup was taken and is not restored", repo.0));
                continue;
            }
            bail!("the backup is damaged: repository {} ({}) is missing", repo.0, relative.display());
        }
        copy_tree(&from.join(&relative), &target.join(&relative)).await?;
        copy::run_git(&config.repository.git_binary, |command| {
            command.arg("--git-dir").arg(target.join(&relative)).args(["fsck", "--no-progress", "--no-dangling"]);
        })
        .await
        .with_context(|| format!("the backup is damaged: repository {} ({}) fails git fsck", repo.0, relative.display()))?;
        restored.repositories += 1;
    }
    for blob in &inventory.blobs {
        if restore_blob(from, target, blob).await? {
            restored.files += 1;
        } else if listed_missing(&manifest, blob) {
            restored.warnings.push(format!("{:?} {} was missing when the backup was taken and is not restored", blob.kind, blob.id));
        } else {
            bail!("the backup is damaged: {} is missing", blob.path.display());
        }
    }
    restored.wikis = restore_wikis(&manifest, from, target, &database, &mut restored.warnings).await?;
    actions_key_warning(config, &database, &mut restored.warnings).await?;

    durable::sync_tree(target).await?;
    tokio::fs::remove_file(target.join(MARKER)).await?;
    durable::sync_dir(target).await?;
    Ok(restored)
}

async fn read_manifest(from: &Path) -> anyhow::Result<Manifest> {
    let path = from.join("manifest.json");
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            bail!("{} is not a complete backup: it has no manifest.json (the backup was interrupted or failed)", from.display())
        }
        Err(error) => return Err(error).with_context(|| format!("cannot read {}", path.display())),
    };
    let manifest: Manifest =
        serde_json::from_slice(&bytes).with_context(|| format!("the backup is damaged: {} is not valid", path.display()))?;
    if manifest.format > FORMAT_VERSION {
        bail!(
            "the backup was written by rbit {} in format {}, newer than this rbit reads ({FORMAT_VERSION}); restore it with that version or newer",
            manifest.rbit_version,
            manifest.format
        );
    }
    anyhow::ensure!(manifest.format >= 1, "the backup is damaged: format {} does not exist", manifest.format);
    let plain = Path::new(&manifest.database).file_name().is_some_and(|name| name == manifest.database.as_str());
    anyhow::ensure!(plain, "the backup is damaged: its database {:?} is not a file name", manifest.database);
    Ok(manifest)
}

/// Creates the data directory, or checks that it is empty, and writes the marker. What an
/// interrupted restore left (the marker says so) is removed first.
async fn prepare(target: &Path) -> anyhow::Result<()> {
    if !tokio::fs::try_exists(target).await? {
        durable::create_private_dir(target).await.with_context(|| format!("cannot create {}", target.display()))?;
    }
    let mut entries = Vec::new();
    let mut listing = tokio::fs::read_dir(target).await.with_context(|| format!("cannot list {}", target.display()))?;
    while let Some(entry) = listing.next_entry().await? {
        entries.push(entry);
    }
    if !entries.is_empty() && !interrupted(target) {
        bail!("{} is not empty; move it aside (or choose another [storage] data_dir) and restore again", target.display());
    }
    for entry in entries {
        if entry.file_name() == MARKER {
            continue;
        }
        let path = entry.path();
        if entry.file_type().await?.is_dir() { remove_tree(&path).await? } else { tokio::fs::remove_file(&path).await? }
    }
    durable::publish(&target.join(MARKER), b"rbit restore in progress; run it again to finish\n").await
}

async fn check_database(manifest: &Manifest, database: &Path) -> anyhow::Result<()> {
    if let Some(expected) = &manifest.database_sha256 {
        let (_, digest) = rbit_services::file_digest::sha256(database).await?;
        if rbit_services::file_digest::hex(&digest) != *expected {
            bail!("the backup is damaged: {} does not match the digest in its manifest", manifest.database);
        }
    } else if manifest.format >= 5 {
        bail!("the backup is damaged: its manifest records no database digest");
    }
    let report = snapshot::inspect(database).await.context("the backup is damaged: its database cannot be read")?;
    if !report.unknown_migrations.is_empty() {
        bail!(
            "the backup's database has migrations this rbit does not know ({:?}): it was written by a newer rbit; restore it with that version or newer",
            report.unknown_migrations
        );
    }
    if !report.is_clean() {
        bail!("the backup's database is damaged: failed migrations {:?}, {}", report.failed_migrations, report.integrity.join("; "));
    }
    Ok(())
}

/// Puts the backup's secret key in place (unless `[storage] secret_key_file` keeps it
/// elsewhere) and checks that it opens what the database seals.
async fn restore_key(config: &Config, from: &Path, database: &Path) -> anyhow::Result<()> {
    let key = config.storage.secret_key_path();
    let included = from.join(SECRET_KEY_FILE);
    if config.storage.secret_key_file.is_none() && included.is_file() {
        durable::write_private(&key, &tokio::fs::read(&included).await?).await?;
    }
    match snapshot::check_key(database, &key).await {
        Ok(()) => Ok(()),
        Err(error @ (StorageError::MissingKey(_) | StorageError::WrongKey(_))) => Err(anyhow::Error::new(error)),
        Err(error) => Err(anyhow::Error::new(error).context("cannot check the secret key")),
    }
}

/// Copies one stored file and checks it against its row; `false` when the backup lacks it.
async fn restore_blob(from: &Path, target: &Path, blob: &Blob) -> anyhow::Result<bool> {
    let to = target.join(&blob.path);
    if let Some(parent) = to.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    match tokio::fs::copy(from.join(&blob.path), &to).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error).with_context(|| format!("cannot copy {}", blob.path.display())),
    }
    if blob.size.is_some() || blob.sha256.is_some() {
        let (size, digest) = rbit_services::file_digest::sha256(&to).await?;
        if blob.size.is_some_and(|expected| expected != size) || blob.sha256.is_some_and(|expected| expected != digest) {
            bail!("the backup is damaged: {} does not match the size and digest its database records", blob.path.display());
        }
    }
    Ok(true)
}

/// Whether the manifest of an older backup lists `blob` as missing when it was taken.
fn listed_missing(manifest: &Manifest, blob: &Blob) -> bool {
    let id = blob.id.parse::<i64>().unwrap_or_default();
    match blob.kind {
        Kind::Attachment => manifest.missing_attachments.contains(&id),
        Kind::Lfs => manifest.missing_lfs_objects.contains(&blob.id),
        Kind::Artifact => manifest.missing_artifacts.contains(&id),
        Kind::Cache => manifest.missing_caches.contains(&id),
        Kind::Log => manifest.missing_logs.contains(&id),
    }
}

async fn restore_wikis(
    manifest: &Manifest,
    from: &Path,
    target: &Path,
    database: &Path,
    warnings: &mut Vec<String>,
) -> anyhow::Result<u64> {
    if manifest.format >= stored::wikis::SINCE_FORMAT {
        let listed = stored::wikis::read(database).await?;
        return wikis::restore(from, target, listed).await;
    }
    let forgotten = stored::wikis::forget_pages(database).await?;
    if forgotten > 0 {
        warnings.push(format!(
            "{forgotten} wikis lost their pages: backups older than format {} hold no wiki objects",
            stored::wikis::SINCE_FORMAT
        ));
    }
    Ok(0)
}

async fn actions_key_warning(config: &Config, database: &Path, warnings: &mut Vec<String>) -> anyhow::Result<()> {
    let secrets = stored::sealed_actions_secrets(database).await?;
    let key = config.actions.secrets_key_path(&config.storage);
    if secrets > 0 && !key.exists() {
        warnings.push(format!(
            "{secrets} Actions secrets are sealed with a key no backup holds; put it at {} before starting rbit, or set the secrets again",
            key.display()
        ));
    }
    Ok(())
}

pub(super) async fn copy_tree(from: &Path, to: &Path) -> anyhow::Result<()> {
    let (from, to) = (from.to_owned(), to.to_owned());
    tokio::task::spawn_blocking(move || {
        let mut pending = vec![(from, to)];
        while let Some((from, to)) = pending.pop() {
            std::fs::create_dir_all(&to)?;
            for entry in std::fs::read_dir(&from)? {
                let entry = entry?;
                let target = to.join(entry.file_name());
                if entry.file_type()?.is_dir() {
                    pending.push((entry.path(), target));
                } else {
                    std::fs::copy(entry.path(), &target).with_context(|| format!("cannot copy {}", entry.path().display()))?;
                }
            }
        }
        Ok(())
    })
    .await?
}

/// Removes a tree that may hold read-only files (Git objects on Windows).
async fn remove_tree(path: &Path) -> anyhow::Result<()> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || {
        if std::fs::remove_dir_all(&path).is_ok() {
            return Ok(());
        }
        let mut pending = vec![path.clone()];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(&dir)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    pending.push(entry.path());
                } else {
                    let mut permissions = entry.metadata()?.permissions();
                    #[expect(clippy::permissions_set_readonly_false, reason = "the file is deleted next")]
                    permissions.set_readonly(false);
                    std::fs::set_permissions(entry.path(), permissions)?;
                }
            }
        }
        std::fs::remove_dir_all(&path).with_context(|| format!("cannot remove {}", path.display()))
    })
    .await?
}
