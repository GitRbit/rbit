//! `rbit backup --output DIR`: a consistent snapshot of the database, repositories,
//! release files, Git LFS objects and installed plugin archives while the server runs.
//!
//! The backup directory mirrors the data directory, so restoring is a copy:
//!
//! ```text
//! DIR/
//! ├── rbit.db                      # `SQLite`, written with `VACUUM INTO`
//! ├── repositories/<owner>/<id>.git  # bare mirrors made with `git clone --mirror`
//! ├── attachments/<repo>/<asset>     # files attached to releases
//! ├── lfs/<aa>/<bb>/<oid>            # Git LFS objects
//! ├── plugins/packages/<digest>.rbit-plugin # retained generic plugin archives
//! ├── secret.key                     # the key that seals stored credentials (see below)
//! └── manifest.json                  # written last: its presence marks a complete backup
//! ```
//!
//! 1. The database is snapshotted first with `VACUUM INTO`: one read transaction, so the
//!    copy is consistent and compact, and writers are never blocked.
//! 2. The repositories listed *in that snapshot* are then mirrored through Git's own pack
//!    protocol (`--no-local`), which reads each repository's refs and the objects they
//!    reach consistently even during a push or `git gc`. A repository created after the
//!    snapshot is not included; one deleted after it is reported as missing.
//! 3. The release files listed in the snapshot are copied. They are written once (an
//!    upload is renamed into place only when complete) and never changed, so a plain copy
//!    is consistent; uploads in progress (`attachments/tmp`) and files the snapshot does
//!    not know are skipped, and files deleted in between are reported as missing.
//! 4. The Git LFS objects listed in the snapshot are copied the same way: content-addressed
//!    and renamed into place only when complete, so a plain copy is consistent; uploads in
//!    progress (`<data_dir>/tmp`) are never included.
//! 5. The secret key file is copied, when it lives in the data directory (the default).
//!    Without it the credentials sealed in the database (TOTP seeds, webhook and push
//!    mirror secrets, saved SMTP and OIDC secrets) cannot be read after a restore. A key
//!    kept elsewhere (`[storage] secret_key_file`) is left out on purpose, so database and
//!    key can be stored apart; it must then be backed up separately.
//! 6. Retained generic package objects and installation pins come from the same
//!    database snapshot. Every referenced archive is copied once and verified by
//!    exact size and SHA256. Missing or corrupt objects fail the backup. Package
//!    storage is append-only; garbage collection must coordinate with backups
//!    before that policy changes. Private staging and pending intents are excluded.
//! 7. `manifest.json` records the format, version, time and counts.
//!
//! The backup directory is created private to its owner (0700) and the database, key and
//! manifest are written 0600 on Unix: they hold password hashes and the sealing key.
//!
//! The redb key/value store is **not** backed up: it holds only login sessions and
//! rebuildable caches. After a restore, users sign in again and caches refill on demand.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Stdio,
    str::FromStr,
    sync::Arc,
};

use anyhow::{Context, bail};
use rbit_core::{AccountId, Config, RepoId, lfs::LfsOid};
use rbit_git::RepositoryStorage;
use rbit_plugin_api::{MAX_PACKAGE_MANIFEST_BYTES, PackageManifest, Sha256Digest};
use rbit_storage::sqlite::plugin_installations::InstallationDescriptor;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{ConnectOptions, Connection, sqlite::SqliteConnectOptions};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::Semaphore,
    task::JoinSet,
};

/// Bumped whenever the layout of a backup directory changes (2: release attachments,
/// 3: Git LFS objects, 4: the secret key file, 5: generic plugin archive objects).
const FORMAT_VERSION: u32 = 5;
const MAX_PACKAGE_OBJECTS: usize = 1024;
/// The secret key file's name inside the backup, as in the data directory.
const SECRET_KEY_FILE: &str = "secret.key";
/// Repositories copied at the same time.
const COPY_CONCURRENCY: usize = 4;
/// Environment variables passed through to Git; everything else is cleared.
const INHERITED_ENV: [&str; 5] = ["PATH", "SYSTEMROOT", "TEMP", "TMP", "TMPDIR"];

/// `manifest.json`.
#[derive(Debug, Serialize)]
pub struct Manifest {
    pub format: u32,
    pub rbit_version: &'static str,
    /// RFC 3339, UTC.
    pub created_at: String,
    /// File name of the `SQLite` snapshot inside the backup directory.
    pub database: &'static str,
    pub accounts: u64,
    pub repositories: u64,
    /// Repositories recorded in the database whose directory did not exist.
    pub missing_repositories: Vec<i64>,
    /// Release files copied into `attachments/`.
    pub attachments: u64,
    /// Release files (asset ids) recorded in the database whose file did not exist.
    pub missing_attachments: Vec<i64>,
    /// Git LFS objects copied into `lfs/`.
    pub lfs_objects: u64,
    /// Git LFS objects (ids) recorded in the database whose file did not exist.
    pub missing_lfs_objects: Vec<String>,
    /// Verified retained generic plugin archives copied into `plugins/packages/`.
    pub plugin_packages: u64,
    /// [`SECRET_KEY_FILE`] when included, otherwise where the key must be backed up from.
    pub secret_key: String,
    pub kv_store: &'static str,
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
        plugin_packages = manifest.plugin_packages,
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
    let Inventory { accounts, repositories, assets, lfs_objects, plugin_packages } = read_inventory(&snapshot).await?;
    tracing::info!(
        accounts,
        repositories = repositories.len(),
        attachments = assets.len(),
        lfs_objects = lfs_objects.len(),
        "database snapshot written"
    );

    let storage = RepositoryStorage::new(config.storage.repositories_dir());
    let target = RepositoryStorage::new(output.join("repositories"));
    let copied = copy_repositories(&config.repository.git_binary, &storage, &target, &repositories).await?;
    let attachments = copy_attachments(&config.storage.attachments_dir(), &output.join("attachments"), &assets).await?;
    let lfs = copy_lfs_objects(&config.storage.lfs_dir(), &output.join("lfs"), &lfs_objects).await?;
    let plugin_packages = copy_plugin_packages(&config.storage.data_dir, output, &plugin_packages).await?;
    let secret_key = copy_secret_key(config, &output.join(SECRET_KEY_FILE)).await?;

    let manifest = Manifest {
        format: FORMAT_VERSION,
        rbit_version: env!("CARGO_PKG_VERSION"),
        created_at: now_rfc3339()?,
        database: "rbit.db",
        accounts,
        repositories: copied.copied,
        missing_repositories: copied.missing,
        attachments: attachments.copied,
        missing_attachments: attachments.missing,
        lfs_objects: lfs.copied,
        missing_lfs_objects: lfs.missing,
        plugin_packages,
        secret_key,
        kv_store: "not included: it holds only sessions and caches, which are rebuilt",
    };
    let staging = output.join("manifest.json.tmp");
    let mut file = create_private_file(&staging).await?;
    file.write_all(&serde_json::to_vec_pretty(&manifest)?).await?;
    file.sync_all().await?;
    drop(file);
    tokio::fs::rename(&staging, output.join("manifest.json")).await?;
    Ok(manifest)
}

/// `VACUUM INTO`: a consistent, compacted copy of the live database. Opened without
/// running migrations, so backing up never changes the source.
async fn snapshot_database(source: &Path, snapshot: &Path) -> anyhow::Result<()> {
    let target = snapshot.to_str().context("the backup path must be valid UTF-8")?;
    let mut connection = SqliteConnectOptions::from_str("sqlite:")?
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
    let mut file = create_private_file(target).await?;
    file.write_all(&key).await?;
    file.sync_all().await?;
    Ok(SECRET_KEY_FILE.to_owned())
}

/// Creates `dir` (and missing parents), accessible only to its owner on Unix.
async fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut builder = tokio::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(dir).await
}

/// Creates the new file `path`, readable only by its owner on Unix.
async fn create_private_file(path: &Path) -> anyhow::Result<tokio::fs::File> {
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path).await.with_context(|| format!("cannot create {}", path.display()))
}

/// What the snapshot says should be copied.
struct Inventory {
    accounts: u64,
    /// `(repository, owner)`.
    repositories: Vec<(RepoId, AccountId)>,
    /// `(repository, asset)` ids of release files.
    assets: Vec<(i64, i64)>,
    /// Ids of Git LFS objects.
    lfs_objects: Vec<LfsOid>,
    plugin_packages: Vec<PackageObject>,
}

async fn read_inventory(snapshot: &Path) -> anyhow::Result<Inventory> {
    let mut connection = SqliteConnectOptions::from_str("sqlite:")?.filename(snapshot).read_only(true).connect().await?;
    let accounts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts").fetch_one(&mut connection).await?;
    let rows: Vec<(i64, i64)> = sqlx::query_as("SELECT id, owner_id FROM repositories ORDER BY id").fetch_all(&mut connection).await?;
    let assets: Vec<(i64, i64)> = sqlx::query_as("SELECT repo_id, id FROM release_assets ORDER BY id").fetch_all(&mut connection).await?;
    let oids: Vec<String> = sqlx::query_scalar("SELECT oid FROM lfs_objects ORDER BY oid").fetch_all(&mut connection).await?;
    let plugin_packages = read_package_inventory(&mut connection).await?;
    connection.close().await?;
    let lfs_objects =
        oids.iter().map(|oid| LfsOid::parse(oid).with_context(|| format!("invalid LFS object id {oid:?}"))).collect::<Result<_, _>>()?;
    Ok(Inventory {
        accounts: u64::try_from(accounts).unwrap_or_default(),
        repositories: rows.into_iter().map(|(id, owner)| (RepoId(id), AccountId(owner))).collect(),
        assets,
        lfs_objects,
        plugin_packages,
    })
}

struct PackageObject {
    digest: Sha256Digest,
    wire_bytes: u64,
}

/// Read only the immutable backup database. Pending operations assert no durable
/// object, whereas every retained object and installation pin must survive restore.
async fn read_package_inventory(connection: &mut sqlx::SqliteConnection) -> anyhow::Result<Vec<PackageObject>> {
    validate_package_policy_snapshot(connection).await?;
    let tables: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name IN ('plugin_objects', 'plugin_installations', 'plugin_package_operations')")
        .fetch_one(&mut *connection).await?;
    if tables != 3 {
        let migrations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name = '_sqlx_migrations'")
            .fetch_one(&mut *connection)
            .await?;
        let current: bool = if migrations == 0 {
            false
        } else {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version >= 63)").fetch_one(&mut *connection).await?
        };
        anyhow::ensure!(tables == 0 && !current, "incomplete generic plugin package ledger in backup snapshot");
        return Ok(Vec::new());
    }
    // Fetch only bounded identities/sizes first; read at most one 1 MiB manifest
    // at a time rather than allocating every retained descriptor together.
    let rows: Vec<(String, i64, i64)> =
        sqlx::query_as("SELECT digest, wire_bytes, length(CAST(manifest AS BLOB)) FROM plugin_objects ORDER BY digest LIMIT 1025")
            .fetch_all(&mut *connection)
            .await?;
    anyhow::ensure!(rows.len() <= MAX_PACKAGE_OBJECTS, "generic plugin package inventory exceeds its bound");
    let mut objects = Vec::with_capacity(rows.len());
    let mut identities = BTreeMap::new();
    let mut total = 0_u64;
    let max_manifest_bytes = i64::try_from(MAX_PACKAGE_MANIFEST_BYTES)?;
    for (digest, wire_bytes, manifest_bytes) in rows {
        anyhow::ensure!((1..=max_manifest_bytes).contains(&manifest_bytes), "invalid package manifest length in backup snapshot");
        let digest: Sha256Digest = digest.parse().context("invalid package archive digest in backup snapshot")?;
        let wire_bytes = u64::try_from(wire_bytes).context("invalid package archive size in backup snapshot")?;
        let json: String = sqlx::query_scalar("SELECT manifest FROM plugin_objects WHERE digest = ?")
            .bind(digest.as_str())
            .fetch_one(&mut *connection)
            .await?;
        let manifest: PackageManifest =
            serde_json::from_str(&json).map_err(|_| anyhow::anyhow!("invalid package manifest in backup snapshot"))?;
        let descriptor =
            InstallationDescriptor::new(manifest, digest.clone(), wire_bytes).context("invalid package object in backup snapshot")?;
        let previous = identities.insert(
            digest.as_str().to_owned(),
            (descriptor.manifest().plugin.id.to_string(), descriptor.manifest().plugin.version.to_string()),
        );
        anyhow::ensure!(previous.is_none(), "duplicate package archive identity in backup snapshot");
        total = total.checked_add(wire_bytes).context("package backup inventory size overflow")?;
        objects.push(PackageObject { digest, wire_bytes });
    }
    let pins: Vec<(String, String, String, Option<String>)> = sqlx::query_as("SELECT i.plugin_id, i.version, i.digest, o.digest FROM plugin_installations i LEFT JOIN plugin_objects o ON o.digest = i.digest ORDER BY i.plugin_id, i.version LIMIT 1025")
        .fetch_all(&mut *connection).await?;
    anyhow::ensure!(pins.len() <= MAX_PACKAGE_OBJECTS, "generic plugin installation inventory exceeds its bound");
    for (id, version, digest, present) in pins {
        anyhow::ensure!(present.is_some(), "installed plugin references a missing archive object");
        anyhow::ensure!(identities.get(&digest) == Some(&(id, version)), "installed plugin identity differs from its archive manifest");
    }
    Ok(objects)
}

/// Review and candidate metadata belong to this same immutable database snapshot.
/// A missing pre-policy schema is supported; a partial/current missing ledger must
/// never be silently restored as unreviewed or open state.
async fn validate_package_policy_snapshot(connection: &mut sqlx::SqliteConnection) -> anyhow::Result<()> {
    let legacy: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name IN \
         ('plugin_policy_gate', 'plugin_review_history', 'plugin_review_heads', \
          'plugin_selection_heads', 'plugin_policy_receipts')",
    )
    .fetch_one(&mut *connection)
    .await?;
    let epochs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name IN \
         ('plugin_policy_epochs', 'plugin_policy_renewals')",
    )
    .fetch_one(&mut *connection)
    .await?;
    let migrations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name = '_sqlx_migrations'")
        .fetch_one(&mut *connection)
        .await?;
    let (policy_marker, epoch_marker): (bool, bool) = if migrations == 0 {
        (false, false)
    } else {
        sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version >= 64), EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version >= 65)",
        )
        .fetch_one(&mut *connection)
        .await?
    };
    if legacy == 0 && epochs == 0 && !policy_marker {
        return Ok(());
    }
    let complete_legacy = legacy == 5 && epochs == 0 && !epoch_marker;
    let complete_epochs = legacy == 5 && epochs == 2;
    anyhow::ensure!(complete_legacy || complete_epochs, "incomplete generic plugin policy ledger in backup snapshot");
    // Storage owns old0064/new0065 decoding and all bounded graph validation.
    rbit_storage::sqlite::plugin_policy::validate_snapshot(connection)
        .await
        .map_err(|_| anyhow::anyhow!("invalid generic plugin policy ledger in backup snapshot"))
}

async fn nonsymlink_directory(path: &Path) -> anyhow::Result<()> {
    let metadata = tokio::fs::symlink_metadata(path).await.context("package backup directory is unavailable")?;
    anyhow::ensure!(metadata.is_dir() && !metadata.file_type().is_symlink(), "package backup directory must be a nonsymlink directory");
    Ok(())
}

async fn create_package_directory(path: &Path) -> anyhow::Result<()> {
    let mut builder = tokio::fs::DirBuilder::new();
    // Parents are checked separately; never follow an existing parent link
    // while recursively creating a package subtree.
    builder.recursive(false);
    #[cfg(unix)]
    builder.mode(0o700);
    match builder.create(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    nonsymlink_directory(path).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if tokio::fs::symlink_metadata(path).await?.permissions().mode() & 0o077 != 0 {
            bail!("package backup directory must be private");
        }
    }
    Ok(())
}

/// CAS objects are append-only: a snapshot pin cannot race object garbage
/// collection. Validate/copy one opened file through bounded buffers; no payload
/// names or plugin/version strings become filesystem paths.
async fn copy_plugin_packages(source: &Path, output: &Path, objects: &[PackageObject]) -> anyhow::Result<u64> {
    nonsymlink_directory(output).await?;
    let target_plugins = output.join("plugins");
    create_package_directory(&target_plugins).await?;
    let target = target_plugins.join("packages");
    create_package_directory(&target).await?;
    if objects.is_empty() {
        return Ok(0);
    }
    nonsymlink_directory(source).await?;
    let source_plugins = source.join("plugins");
    nonsymlink_directory(&source_plugins).await?;
    let source = source_plugins.join("packages");
    nonsymlink_directory(&source).await?;
    for object in objects {
        let filename = format!("{}.rbit-plugin", object.digest.as_str());
        let from = source.join(&filename);
        let metadata = tokio::fs::symlink_metadata(&from).await.context("referenced plugin archive is missing")?;
        anyhow::ensure!(metadata.is_file() && !metadata.file_type().is_symlink(), "plugin archive must be a nonsymlink regular file");
        anyhow::ensure!(metadata.len() == object.wire_bytes, "plugin archive size differs from backup inventory");
        let mut options = tokio::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        #[cfg(windows)]
        // Open the reparse point itself instead of following its target.
        options.custom_flags(0x0020_0000);
        let mut input = options.open(&from).await.context("cannot open referenced plugin archive")?;
        let metadata = input.metadata().await?;
        anyhow::ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() == object.wire_bytes,
            "opened plugin archive differs from backup inventory"
        );
        let mut file = create_private_file(&target.join(filename)).await?;
        let mut digest = Sha256::new();
        let mut remaining = object.wire_bytes;
        let mut buffer = vec![0; 64 * 1024];
        while remaining != 0 {
            let count = usize::try_from(remaining.min(buffer.len() as u64))?;
            let read = input.read(&mut buffer[..count]).await?;
            anyhow::ensure!(read != 0, "plugin archive is truncated");
            file.write_all(&buffer[..read]).await?;
            digest.update(&buffer[..read]);
            remaining -= read as u64;
        }
        anyhow::ensure!(input.read(&mut [0; 1]).await? == 0, "plugin archive has trailing bytes");
        let actual = LfsOid::from_digest(digest.finalize().into());
        anyhow::ensure!(actual.as_str() == object.digest.as_str(), "plugin archive digest differs from backup inventory");
        file.sync_all().await?;
    }
    #[cfg(unix)]
    for directory in [target.as_path(), target_plugins.as_path(), output] {
        tokio::fs::File::open(directory).await?.sync_all().await?;
    }
    Ok(u64::try_from(objects.len())?)
}

#[derive(Debug)]
struct Copied<Id = i64> {
    copied: u64,
    missing: Vec<Id>,
}

impl<Id> Default for Copied<Id> {
    fn default() -> Self {
        Self { copied: 0, missing: Vec::new() }
    }
}

async fn copy_repositories(
    git: &Path,
    source: &RepositoryStorage,
    target: &RepositoryStorage,
    repositories: &[(RepoId, AccountId)],
) -> anyhow::Result<Copied> {
    let permits = Arc::new(Semaphore::new(COPY_CONCURRENCY));
    let mut copies = JoinSet::new();
    let mut result = Copied::default();
    for &(repo, owner) in repositories {
        let from = source.path_for(owner, repo);
        if !from.join("HEAD").is_file() {
            tracing::warn!(repo = repo.0, path = %from.display(), "repository is in the database but not on disk; skipped");
            result.missing.push(repo.0);
            continue;
        }
        let to = target.path_for(owner, repo);
        let git = git.to_owned();
        let permit = Arc::clone(&permits).acquire_owned().await?;
        copies.spawn(async move {
            let _permit = permit;
            mirror(&git, &from, &to).await.with_context(|| format!("cannot copy repository {} ({})", repo.0, from.display()))
        });
    }
    let mut failures = Vec::new();
    while let Some(copy) = copies.join_next().await {
        match copy? {
            Ok(()) => result.copied += 1,
            Err(error) => {
                tracing::error!("{error:#}");
                failures.push(error);
            }
        }
    }
    if let Some(first) = failures.into_iter().next() {
        return Err(first.context("backup incomplete: at least one repository could not be copied"));
    }
    Ok(result)
}

/// Copies the release files `assets` (`(repository, asset)` ids) from `<source>/<repo>/<asset>`
/// to the same place under `target`. Files are immutable once in place, so a copy is
/// consistent; one that disappeared since the snapshot (its release was deleted) is
/// reported as missing rather than failing the backup.
async fn copy_attachments(source: &Path, target: &Path, assets: &[(i64, i64)]) -> anyhow::Result<Copied> {
    // Always present, so restoring can copy it unconditionally.
    tokio::fs::create_dir_all(target).await?;
    let mut result = Copied::default();
    for &(repo, asset) in assets {
        let relative = Path::new(&repo.to_string()).join(asset.to_string());
        let from = source.join(&relative);
        if !tokio::fs::try_exists(&from).await? {
            tracing::warn!(asset, path = %from.display(), "release file is in the database but not on disk; skipped");
            result.missing.push(asset);
            continue;
        }
        let to = target.join(&relative);
        if let Some(parent) = to.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(&from, &to).await.with_context(|| format!("cannot copy release file {asset} ({})", from.display()))?;
        result.copied += 1;
    }
    Ok(result)
}

/// Copies the Git LFS objects `oids` from `<source>/<aa>/<bb>/<oid>` to the same place
/// under `target`. Objects never change once in place; one removed since the snapshot (no
/// repository used it any more) is reported as missing.
async fn copy_lfs_objects(source: &Path, target: &Path, oids: &[LfsOid]) -> anyhow::Result<Copied<String>> {
    tokio::fs::create_dir_all(target).await?;
    let mut result = Copied::default();
    for oid in oids {
        let (first, second) = oid.shards();
        let relative = Path::new(first).join(second).join(oid.as_str());
        let from = source.join(&relative);
        if !tokio::fs::try_exists(&from).await? {
            tracing::warn!(%oid, path = %from.display(), "LFS object is in the database but not on disk; skipped");
            result.missing.push(oid.to_string());
            continue;
        }
        let to = target.join(&relative);
        if let Some(parent) = to.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(&from, &to).await.with_context(|| format!("cannot copy LFS object {oid} ({})", from.display()))?;
        result.copied += 1;
    }
    Ok(result)
}

/// Mirrors one bare repository through the pack protocol, then makes the copy
/// indistinguishable from the original: same `HEAD`, no `origin` remote.
async fn mirror(git: &Path, from: &Path, to: &Path) -> anyhow::Result<()> {
    if let Some(parent) = to.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    run_git(git, |command| {
        command.args(["clone", "--mirror", "--no-local", "--quiet"]).arg(from).arg(to);
    })
    .await?;
    // `clone` points HEAD at the source's HEAD branch only if it exists; copy the
    // symbolic ref itself so empty repositories keep their default branch too.
    tokio::fs::copy(from.join("HEAD"), to.join("HEAD")).await?;
    // Not `git remote remove`: with a mirror refspec it would delete every ref.
    run_git(git, |command| {
        command.arg("--git-dir").arg(to).args(["config", "--remove-section", "remote.origin"]);
    })
    .await
}

async fn run_git(git: &Path, configure: impl FnOnce(&mut Command)) -> anyhow::Result<()> {
    let mut command = Command::new(git);
    command.env_clear();
    for key in INHERITED_ENV {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    configure(&mut command);
    let output = command.output().await.with_context(|| format!("cannot run {}", git.display()))?;
    if !output.status.success() {
        bail!("git exited with {}: {}", output.status, String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(())
}

fn now_rfc3339() -> anyhow::Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}

#[cfg(test)]
mod package_backup_tests {
    use super::*;

    #[tokio::test]
    async fn package_copy_never_overwrites_an_existing_target() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        let output = directory.path().join("output");
        create_private_dir(&source.join("plugins").join("packages")).await.unwrap();
        create_private_dir(&output).await.unwrap();
        let bytes = b"verified package archive";
        let digest = LfsOid::from_digest(Sha256::digest(bytes).into()).as_str().parse::<Sha256Digest>().unwrap();
        let filename = format!("{}.rbit-plugin", digest.as_str());
        tokio::fs::write(source.join("plugins").join("packages").join(&filename), bytes).await.unwrap();
        create_package_directory(&output.join("plugins")).await.unwrap();
        let target = output.join("plugins").join("packages");
        create_package_directory(&target).await.unwrap();
        tokio::fs::write(target.join(&filename), b"existing target").await.unwrap();
        let object = PackageObject { digest, wire_bytes: bytes.len() as u64 };
        assert!(copy_plugin_packages(&source, &output, &[object]).await.is_err());
        assert_eq!(tokio::fs::read(target.join(filename)).await.unwrap(), b"existing target");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn package_copy_rejects_target_directory_links_before_creating_children() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("output");
        let outside = directory.path().join("outside");
        create_private_dir(&output).await.unwrap();
        create_private_dir(&outside).await.unwrap();
        std::os::unix::fs::symlink(&outside, output.join("plugins")).unwrap();
        assert!(copy_plugin_packages(directory.path(), &output, &[]).await.is_err());
        assert!(!outside.join("packages").exists());
    }
}
