//! `rustit backup --output DIR`: a consistent snapshot of the database, repositories and
//! release files, safe to take while the server is running.
//!
//! The backup directory mirrors the data directory, so restoring is a copy:
//!
//! ```text
//! DIR/
//! ├── rustit.db                      # SQLite, written with `VACUUM INTO`
//! ├── repositories/<owner>/<id>.git  # bare mirrors made with `git clone --mirror`
//! ├── attachments/<repo>/<asset>     # files attached to releases
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
//! 4. `manifest.json` records the format, version, time and counts.
//!
//! The redb key/value store is **not** backed up: it holds only login sessions and
//! rebuildable caches. After a restore, users sign in again and caches refill on demand.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    str::FromStr,
    sync::Arc,
};

use anyhow::{Context, bail};
use rustit_core::{AccountId, Config, RepoId};
use rustit_git::RepositoryStorage;
use serde::Serialize;
use sqlx::{ConnectOptions, Connection, sqlite::SqliteConnectOptions};
use tokio::{process::Command, sync::Semaphore, task::JoinSet};

/// Bumped whenever the layout of a backup directory changes (2: release attachments).
const FORMAT_VERSION: u32 = 2;
/// Repositories copied at the same time.
const COPY_CONCURRENCY: usize = 4;
/// Environment variables passed through to Git; everything else is cleared.
const INHERITED_ENV: [&str; 5] = ["PATH", "SYSTEMROOT", "TEMP", "TMP", "TMPDIR"];

/// `manifest.json`.
#[derive(Debug, Serialize)]
pub struct Manifest {
    pub format: u32,
    pub rustit_version: &'static str,
    /// RFC 3339, UTC.
    pub created_at: String,
    /// File name of the SQLite snapshot inside the backup directory.
    pub database: &'static str,
    pub accounts: u64,
    pub repositories: u64,
    /// Repositories recorded in the database whose directory did not exist.
    pub missing_repositories: Vec<i64>,
    /// Release files copied into `attachments/`.
    pub attachments: u64,
    /// Release files (asset ids) recorded in the database whose file did not exist.
    pub missing_attachments: Vec<i64>,
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
    tokio::fs::create_dir_all(output).await.with_context(|| format!("cannot create {}", output.display()))?;

    let snapshot = output.join("rustit.db");
    snapshot_database(&source, &snapshot).await?;
    let Inventory { accounts, repositories, assets } = read_inventory(&snapshot).await?;
    tracing::info!(accounts, repositories = repositories.len(), attachments = assets.len(), "database snapshot written");

    let storage = RepositoryStorage::new(config.storage.repositories_dir());
    let target = RepositoryStorage::new(output.join("repositories"));
    let copied = copy_repositories(&config.repository.git_binary, &storage, &target, &repositories).await?;
    let attachments = copy_attachments(&config.storage.attachments_dir(), &output.join("attachments"), &assets).await?;

    let manifest = Manifest {
        format: FORMAT_VERSION,
        rustit_version: env!("CARGO_PKG_VERSION"),
        created_at: now_rfc3339()?,
        database: "rustit.db",
        accounts,
        repositories: copied.copied,
        missing_repositories: copied.missing,
        attachments: attachments.copied,
        missing_attachments: attachments.missing,
        kv_store: "not included: it holds only sessions and caches, which are rebuilt",
    };
    let staging = output.join("manifest.json.tmp");
    tokio::fs::write(&staging, serde_json::to_vec_pretty(&manifest)?).await?;
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

/// What the snapshot says should be copied.
struct Inventory {
    accounts: u64,
    /// `(repository, owner)`.
    repositories: Vec<(RepoId, AccountId)>,
    /// `(repository, asset)` ids of release files.
    assets: Vec<(i64, i64)>,
}

async fn read_inventory(snapshot: &Path) -> anyhow::Result<Inventory> {
    let mut connection = SqliteConnectOptions::from_str("sqlite:")?.filename(snapshot).read_only(true).connect().await?;
    let accounts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts").fetch_one(&mut connection).await?;
    let rows: Vec<(i64, i64)> = sqlx::query_as("SELECT id, owner_id FROM repositories ORDER BY id").fetch_all(&mut connection).await?;
    let assets: Vec<(i64, i64)> = sqlx::query_as("SELECT repo_id, id FROM release_assets ORDER BY id").fetch_all(&mut connection).await?;
    connection.close().await?;
    Ok(Inventory {
        accounts: u64::try_from(accounts).unwrap_or_default(),
        repositories: rows.into_iter().map(|(id, owner)| (RepoId(id), AccountId(owner))).collect(),
        assets,
    })
}

#[derive(Debug, Default)]
struct Copied {
    copied: u64,
    missing: Vec<i64>,
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
