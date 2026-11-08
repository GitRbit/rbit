//! `rbit backup --output DIR`: a consistent snapshot of the database, repositories and
//! every stored file the database refers to, safe to take while the server is running.
//!
//! The backup directory mirrors the data directory, so restoring is a copy:
//!
//! ```text
//! DIR/
//! ├── rbit.db                      # `SQLite`, written with `VACUUM INTO`
//! ├── repositories/<owner>/<id>.git  # bare mirrors made with `git clone --mirror`
//! ├── attachments/<repo>/<asset>     # files attached to releases
//! ├── lfs/<aa>/<bb>/<oid>            # Git LFS objects
//! ├── actions/artifacts/<run>/<id>.tar.gz   # workflow artifacts
//! ├── actions/caches/<repo>/<id>.tar.gz     # `actions/cache` entries
//! ├── actions/logs/<repo>/<task>.{log,log.gz,idx}  # job logs
//! ├── secret.key                     # the key that seals stored credentials (see below)
//! └── manifest.json                  # written last: its presence marks a complete backup
//! ```
//!
//! 1. The database is snapshotted first with `VACUUM INTO`: one read transaction, so the
//!    copy is consistent and compact, and writers are never blocked.
//! 2. The repositories listed *in that snapshot* are then mirrored through Git's own pack
//!    protocol (`--no-local`), which reads each repository's refs and the objects they
//!    reach consistently even during a push or `git gc`. A repository created after the
//!    snapshot is not included; one deleted after it is reported as missing and left out
//!    of the snapshot too.
//! 3. The stored files listed in the snapshot are copied ([`inventory`]): release files,
//!    Git LFS objects, workflow artifacts and cache entries are written once (an upload is
//!    renamed into place only when complete) and never changed, so a plain copy is
//!    consistent. A finished job's log is compressed and never changes either; a running
//!    job's log only grows past the line count the snapshot records, and one finished
//!    since is decompressed back. Uploads in progress and files the snapshot does not know
//!    are skipped; files deleted in between are reported as missing, and their rows are
//!    removed from the snapshot ([`prune`]) so a restore never refers to them.
//! 4. The secret key file is copied, when it lives in the data directory (the default).
//!    Without it the credentials sealed in the database (TOTP seeds, webhook and push
//!    mirror secrets, saved SMTP and OIDC secrets) cannot be read after a restore. A key
//!    kept elsewhere (`[storage] secret_key_file`) is left out on purpose, so database and
//!    key can be stored apart; it must then be backed up separately. The Actions secrets
//!    key is never included, for the same reason.
//! 5. Everything written so far is synced to disk ([`durable::sync_tree`]): neither
//!    `VACUUM INTO` nor a clone nor a file copy syncs what it writes, and the manifest
//!    must never reach the disk before the files it vouches for.
//! 6. `manifest.json` records the format, version, time, counts and the database's
//!    SHA-256. It is written to a synced temporary file, renamed into place and its
//!    directory synced. [`restore`] puts a backup back into service.
//!
//! The backup directory is created private to its owner (0700) and the database, key and
//! manifest are written 0600 on Unix: they hold password hashes and the sealing key.
//!
//! The redb key/value store is **not** backed up: it holds only login sessions and
//! rebuildable caches. After a restore, users sign in again and caches refill on demand.

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{Context, bail};
use rbit_core::Config;
use rbit_git::RepositoryStorage;
use serde::{Deserialize, Serialize};
use sqlx::{ConnectOptions, Connection, sqlite::SqliteConnectOptions};

use self::durable::{create_private_dir, create_private_file, write_private};

mod copy;
mod durable;
mod inventory;
mod prune;
pub mod restore;

/// Bumped whenever the layout of a backup directory changes (2: release attachments,
/// 3: Git LFS objects, 4: the secret key file, 5: Actions artifacts, caches and logs).
pub const FORMAT_VERSION: u32 = 5;
/// The secret key file's name inside the backup, as in the data directory.
const SECRET_KEY_FILE: &str = "secret.key";

/// `manifest.json`.
#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub rbit_version: String,
    /// RFC 3339, UTC.
    pub created_at: String,
    /// File name of the `SQLite` snapshot inside the backup directory.
    pub database: String,
    pub accounts: u64,
    pub repositories: u64,
    /// Repositories recorded in the database whose directory did not exist.
    pub missing_repositories: Vec<i64>,
    /// Release files copied into `attachments/`.
    #[serde(default)]
    pub attachments: u64,
    /// Release files (asset ids) recorded in the database whose file did not exist.
    #[serde(default)]
    pub missing_attachments: Vec<i64>,
    /// Git LFS objects copied into `lfs/`.
    #[serde(default)]
    pub lfs_objects: u64,
    /// Git LFS objects (ids) recorded in the database whose file did not exist.
    #[serde(default)]
    pub missing_lfs_objects: Vec<String>,
    /// Workflow artifacts copied into `actions/artifacts/`, and those (ids) missing.
    #[serde(default)]
    pub artifacts: u64,
    #[serde(default)]
    pub missing_artifacts: Vec<i64>,
    /// Cache entries copied into `actions/caches/`, and those (ids) missing.
    #[serde(default)]
    pub caches: u64,
    #[serde(default)]
    pub missing_caches: Vec<i64>,
    /// Job log files copied into `actions/logs/`, and the tasks whose log was missing.
    #[serde(default)]
    pub logs: u64,
    #[serde(default)]
    pub missing_logs: Vec<i64>,
    /// [`SECRET_KEY_FILE`] when included, otherwise where the key must be backed up from.
    #[serde(default)]
    pub secret_key: String,
    #[serde(default)]
    pub actions_secrets_key: String,
    pub kv_store: String,
    /// SHA-256 of [`Self::database`] as written, in hex (format 5 and later).
    #[serde(default)]
    pub database_sha256: Option<String>,
}

pub async fn run(config: Config, output: PathBuf) -> anyhow::Result<()> {
    let manifest = create(&config, &output).await?;
    tracing::info!(
        output = %output.display(),
        accounts = manifest.accounts,
        repositories = manifest.repositories,
        missing = manifest.missing_repositories.len(),
        attachments = manifest.attachments,
        missing_attachments = manifest.missing_attachments.len(),
        lfs_objects = manifest.lfs_objects,
        missing_lfs_objects = manifest.missing_lfs_objects.len(),
        artifacts = manifest.artifacts,
        caches = manifest.caches,
        logs = manifest.logs,
        "backup complete"
    );
    Ok(())
}

/// Writes a complete backup of `config`'s data directory into the new directory `output`.
pub async fn create(config: &Config, output: &Path) -> anyhow::Result<Manifest> {
    let source = config.storage.sqlite_path();
    anyhow::ensure!(source.is_file(), "no database at {}; check [storage] data_dir", source.display());
    if tokio::fs::try_exists(output).await? {
        bail!("{} already exists; choose a new directory", output.display());
    }
    create_private_dir(output).await.with_context(|| format!("cannot create {}", output.display()))?;

    let snapshot = output.join("rbit.db");
    // `VACUUM INTO` accepts an empty file, so the snapshot is private from the start.
    drop(create_private_file(&snapshot).await?);
    snapshot_database(&source, &snapshot).await?;
    let inventory = inventory::read(&snapshot, FORMAT_VERSION).await?;
    tracing::info!(
        accounts = inventory.accounts,
        repositories = inventory.repositories.len(),
        files = inventory.blobs.len(),
        "database snapshot written"
    );

    let storage = RepositoryStorage::new(config.storage.repositories_dir());
    let target = RepositoryStorage::new(output.join("repositories"));
    let repositories = copy::copy_repositories(&config.repository.git_binary, &storage, &target, &inventory.repositories).await?;
    let blobs = copy::copy_blobs(&config.storage.data_dir, output, &inventory.blobs).await?;
    let secret_key = copy_secret_key(config, &output.join(SECRET_KEY_FILE)).await?;
    prune::prune(&snapshot, &repositories, &blobs).await?;
    let (_, database_digest) = rbit_services::file_digest::sha256(&snapshot).await?;
    let actions_key = config.actions.secrets_key_path(&config.storage);

    let manifest = Manifest {
        format: FORMAT_VERSION,
        rbit_version: env!("CARGO_PKG_VERSION").to_owned(),
        created_at: now_rfc3339()?,
        database: "rbit.db".to_owned(),
        accounts: inventory.accounts,
        repositories: repositories.copied,
        missing_repositories: repositories.missing,
        attachments: blobs.attachments.copied,
        missing_attachments: blobs.attachments.missing,
        lfs_objects: blobs.lfs.copied,
        missing_lfs_objects: blobs.lfs.missing,
        artifacts: blobs.artifacts.copied,
        missing_artifacts: blobs.artifacts.missing,
        caches: blobs.caches.copied,
        missing_caches: blobs.caches.missing,
        logs: blobs.logs.copied,
        missing_logs: blobs.logs.missing,
        secret_key,
        actions_secrets_key: format!("not included: kept at {}; back it up separately", actions_key.display()),
        kv_store: "not included: it holds only sessions and caches, which are rebuilt".to_owned(),
        database_sha256: Some(rbit_services::file_digest::hex(&database_digest)),
    };
    durable::sync_tree(output).await?;
    durable::publish(&output.join("manifest.json"), &serde_json::to_vec_pretty(&manifest)?).await?;
    Ok(manifest)
}

/// `VACUUM INTO`: a consistent, compacted copy of the live database. Opened without
/// running migrations, so backing up never changes the source.
async fn snapshot_database(source: &Path, snapshot: &Path) -> anyhow::Result<()> {
    let target = snapshot.to_str().context("the backup path must be valid UTF-8")?;
    let mut connection = rbit_storage::sqlite::writer_compatibility::register(SqliteConnectOptions::from_str("sqlite:")?)
        .filename(source)
        .create_if_missing(false)
        .busy_timeout(std::time::Duration::from_secs(30))
        .connect()
        .await
        .with_context(|| format!("cannot open {}", source.display()))?;
    sqlx::query("VACUUM INTO ?").bind(target).execute(&mut connection).await.context("VACUUM INTO failed")?;
    connection.close().await?;
    Ok(())
}

/// Copies the secret key file into the backup if it lives in the data directory; returns
/// what the manifest records about it.
async fn copy_secret_key(config: &Config, target: &Path) -> anyhow::Result<String> {
    let source = config.storage.secret_key_path();
    if config.storage.secret_key_file.is_some() {
        tracing::warn!(
            path = %source.display(),
            "the secret key file is kept outside the data directory and not included; back it up separately"
        );
        return Ok(format!("not included: kept at {}; back it up separately", source.display()));
    }
    let key = match tokio::fs::read(&source).await {
        Ok(key) => key,
        // Created on the first start of a version that seals secrets; nothing is sealed yet.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            tracing::warn!(path = %source.display(), "no secret key file found; nothing in the database is sealed with one yet");
            return Ok("not included: none existed yet".to_owned());
        }
        Err(error) => return Err(error).with_context(|| format!("cannot read {}", source.display())),
    };
    write_private(target, &key).await?;
    Ok(SECRET_KEY_FILE.to_owned())
}

fn now_rfc3339() -> anyhow::Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}
