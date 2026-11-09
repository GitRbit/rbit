//! What a database snapshot says the backup must hold besides itself: every repository and
//! every stored file a row refers to, with the size and digest the row records for it.

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::Context;
use rbit_core::{AccountId, RepoId, lfs::LfsOid};
use sqlx::{ConnectOptions, Connection, SqliteConnection, sqlite::SqliteConnectOptions};

/// The kinds of stored file, each with its own rows and directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `attachments/<repo>/<asset>`: release files.
    Attachment,
    /// `lfs/<aa>/<bb>/<oid>`: Git LFS objects.
    Lfs,
    /// `actions/artifacts/<run>/<artifact>.tar.gz`: workflow artifacts.
    Artifact,
    /// `actions/caches/<repo>/<entry>.tar.gz`: `actions/cache` entries.
    Cache,
    /// `actions/logs/<repo>/<task>.{log,log.gz,idx}`: job logs and their line indexes.
    Log,
}

impl Kind {
    /// The first backup format that holds this kind.
    pub const fn since_format(self) -> u32 {
        match self {
            Self::Attachment => 2,
            Self::Lfs => 3,
            Self::Artifact | Self::Cache | Self::Log => 5,
        }
    }
}

/// One stored file a row refers to.
#[derive(Debug, Clone)]
pub struct Blob {
    pub kind: Kind,
    /// The row's id (an LFS object's oid), as the manifest reports it.
    pub id: String,
    /// Relative to the data directory and the backup alike.
    pub path: PathBuf,
    /// What the row records, when it records it.
    pub size: Option<u64>,
    pub sha256: Option<[u8; 32]>,
    /// A live log finalised since the snapshot: its plain file is gone and this
    /// compressed one (relative, as `path`) holds the same lines.
    pub compressed: Option<PathBuf>,
}

impl Blob {
    fn new(kind: Kind, id: &dyn std::fmt::Display, path: PathBuf) -> Self {
        Self { kind, id: id.to_string(), path, size: None, sha256: None, compressed: None }
    }

    fn recorded(mut self, size: i64, sha256: &[u8]) -> Self {
        self.size = u64::try_from(size).ok();
        self.sha256 = sha256.try_into().ok();
        self
    }
}

/// What the snapshot says should be copied.
#[derive(Debug)]
pub struct Inventory {
    pub accounts: u64,
    /// `(repository, owner)`.
    pub repositories: Vec<(RepoId, AccountId)>,
    pub blobs: Vec<Blob>,
}

pub async fn read_only(snapshot: &Path) -> anyhow::Result<SqliteConnection> {
    let options = rbit_storage::sqlite::writer_compatibility::register(SqliteConnectOptions::from_str("sqlite:")?);
    options.filename(snapshot).read_only(true).connect().await.with_context(|| format!("cannot open {}", snapshot.display()))
}

/// Reads the inventory of the snapshot at `snapshot`, of the kinds backup format `format` holds.
pub async fn read(snapshot: &Path, format: u32) -> anyhow::Result<Inventory> {
    let mut connection = read_only(snapshot).await?;
    let accounts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts").fetch_one(&mut connection).await?;
    let rows: Vec<(i64, i64)> = sqlx::query_as("SELECT id, owner_id FROM repositories ORDER BY id").fetch_all(&mut connection).await?;
    let mut blobs = Vec::new();
    if format >= Kind::Attachment.since_format() {
        let assets: Vec<(i64, i64, i64, Vec<u8>)> =
            sqlx::query_as("SELECT repo_id, id, size, sha256 FROM release_assets ORDER BY id").fetch_all(&mut connection).await?;
        for (repo, id, size, sha256) in assets {
            let path = Path::new("attachments").join(repo.to_string()).join(id.to_string());
            blobs.push(Blob::new(Kind::Attachment, &id, path).recorded(size, &sha256));
        }
    }
    if format >= Kind::Lfs.since_format() {
        let objects: Vec<(String, i64)> =
            sqlx::query_as("SELECT oid, size FROM lfs_objects ORDER BY oid").fetch_all(&mut connection).await?;
        for (oid, size) in objects {
            let parsed = LfsOid::parse(&oid).with_context(|| format!("invalid LFS object id {oid:?}"))?;
            let (first, second) = parsed.shards();
            let digest = (0..32).map(|i| u8::from_str_radix(&oid[i * 2..i * 2 + 2], 16)).collect::<Result<Vec<u8>, _>>()?;
            let path = Path::new("lfs").join(first).join(second).join(parsed.as_str());
            blobs.push(Blob::new(Kind::Lfs, &oid, path).recorded(size, &digest));
        }
    }
    // A database not yet migrated (backed up before an upgrade) may predate Actions.
    if format >= Kind::Artifact.since_format() && has_table(&mut connection, "actions_artifacts").await? {
        read_actions(&mut connection, &mut blobs).await?;
    }
    connection.close().await?;
    Ok(Inventory {
        accounts: u64::try_from(accounts).unwrap_or_default(),
        repositories: rows.into_iter().map(|(id, owner)| (RepoId(id), AccountId(owner))).collect(),
        blobs,
    })
}

async fn has_table(connection: &mut SqliteConnection, name: &str) -> anyhow::Result<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?)")
        .bind(name)
        .fetch_one(connection)
        .await?)
}

async fn read_actions(connection: &mut SqliteConnection, blobs: &mut Vec<Blob>) -> anyhow::Result<()> {
    let actions = Path::new("actions");
    let artifacts: Vec<(i64, i64, i64, Vec<u8>)> =
        sqlx::query_as("SELECT run_id, id, size, sha256 FROM actions_artifacts ORDER BY id").fetch_all(&mut *connection).await?;
    for (run, id, size, sha256) in artifacts {
        let path = actions.join("artifacts").join(run.to_string()).join(format!("{id}.tar.gz"));
        blobs.push(Blob::new(Kind::Artifact, &id, path).recorded(size, &sha256));
    }
    // A database not yet migrated (backed up before an upgrade) may predate the cache.
    let caches: Vec<(i64, i64, i64, Vec<u8>)> = if has_table(connection, "actions_caches").await? {
        sqlx::query_as("SELECT repo_id, id, size, sha256 FROM actions_caches ORDER BY id").fetch_all(&mut *connection).await?
    } else {
        Vec::new()
    };
    for (repo, id, size, sha256) in caches {
        let path = actions.join("caches").join(repo.to_string()).join(format!("{id}.tar.gz"));
        blobs.push(Blob::new(Kind::Cache, &id, path).recorded(size, &sha256));
    }
    // A log is written only once a line arrives; until then neither file exists.
    let logs: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT j.repo_id, t.id, t.log_state FROM actions_tasks t JOIN actions_jobs j ON j.id = t.job_id \
         WHERE t.log_state IN ('live', 'final') AND t.log_bytes > 0 ORDER BY t.id",
    )
    .fetch_all(&mut *connection)
    .await?;
    for (repo, task, state) in logs {
        let dir = actions.join("logs").join(repo.to_string());
        let compressed = dir.join(format!("{task}.log.gz"));
        blobs.push(Blob::new(Kind::Log, &task, dir.join(format!("{task}.idx"))));
        if state == "final" {
            blobs.push(Blob::new(Kind::Log, &task, compressed));
        } else {
            blobs.push(Blob { compressed: Some(compressed), ..Blob::new(Kind::Log, &task, dir.join(format!("{task}.log"))) });
        }
    }
    Ok(())
}
