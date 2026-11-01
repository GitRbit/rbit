//! Host-owned installation journal. Durable rows assert host decisions; they do
//! not verify archive bytes, construct publication receipts, approve trust or
//! grants, select a runtime version, or change enabled intent.

use rbit_core::{AccountId, Timestamp};
use rbit_plugin_api::{MAX_PACKAGE_MANIFEST_BYTES, PackageManifest, PluginId, Sha256Digest};
use semver::Version;
use sqlx::{Row, sqlite::SqliteRow};

use super::pool::Pool;
use crate::StorageError;

const MAX_WIRE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const PAGE_SIZE: i64 = 128;

/// Host-generated idempotency token, never a filesystem name or package input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationId(String);
impl OperationId {
    pub fn new(value: &str) -> Result<Self, StorageError> {
        if value.is_empty() || value.len() > 64 || !value.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
            return Err(invalid());
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Bounded declarations supplied by the trusted host coordinator. This value is
/// not evidence of byte integrity, namespace durability, trust or authorization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationDescriptor {
    manifest: PackageManifest,
    digest: Sha256Digest,
    wire_bytes: u64,
}
impl InstallationDescriptor {
    pub fn new(manifest: PackageManifest, digest: Sha256Digest, wire_bytes: u64) -> Result<Self, StorageError> {
        manifest.validate().map_err(|_| invalid())?;
        if wire_bytes == 0 || wire_bytes > MAX_WIRE_BYTES {
            return Err(invalid());
        }
        let json = serde_json::to_vec(&manifest).map_err(|_| invalid())?;
        if json.len() > MAX_PACKAGE_MANIFEST_BYTES {
            return Err(invalid());
        }
        Ok(Self { manifest, digest, wire_bytes })
    }
    pub fn manifest(&self) -> &PackageManifest {
        &self.manifest
    }
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }
    pub const fn wire_bytes(&self) -> u64 {
        self.wire_bytes
    }
    fn json(&self) -> Result<String, StorageError> {
        serde_json::to_string(&self.manifest).map_err(|_| invalid())
    }
}

/// Static failure classifications; never retain source errors or secret inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationError {
    Source,
    Validation,
    Publication,
    Durability,
    Conflict,
    Interrupted,
}
impl OperationError {
    const fn code(self) -> i64 {
        match self {
            Self::Source => 0,
            Self::Validation => 1,
            Self::Publication => 2,
            Self::Durability => 3,
            Self::Conflict => 4,
            Self::Interrupted => 5,
        }
    }
    fn decode(value: i64) -> Result<Self, StorageError> {
        match value {
            0 => Ok(Self::Source),
            1 => Ok(Self::Validation),
            2 => Ok(Self::Publication),
            3 => Ok(Self::Durability),
            4 => Ok(Self::Conflict),
            5 => Ok(Self::Interrupted),
            _ => Err(invalid()),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationStage {
    Pending,
    AwaitingDurability,
    Complete,
    Error(OperationError),
}
impl OperationStage {
    const fn codes(self) -> (i64, Option<i64>) {
        match self {
            Self::Pending => (0, None),
            Self::AwaitingDurability => (1, None),
            Self::Complete => (2, None),
            Self::Error(error) => (3, Some(error.code())),
        }
    }
    fn decode(stage: i64, error: Option<i64>) -> Result<Self, StorageError> {
        match (stage, error) {
            (0, None) => Ok(Self::Pending),
            (1, None) => Ok(Self::AwaitingDurability),
            (2, None) => Ok(Self::Complete),
            (3, Some(error)) => Ok(Self::Error(OperationError::decode(error)?)),
            _ => Err(invalid()),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationOperation {
    pub id: OperationId,
    pub descriptor: InstallationDescriptor,
    pub actor: Option<AccountId>,
    pub stage: OperationStage,
    pub revision: i64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    pub descriptor: InstallationDescriptor,
    pub installed_by: Option<AccountId>,
    pub installed_at: Timestamp,
}

#[derive(Debug, Clone)]
pub struct PluginInstallationStore {
    pool: Pool,
}
impl PluginInstallationStore {
    pub(crate) fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Durably records bounded intent before host filesystem publication. Reusing
    /// a token requires the exact descriptor and actor; it never resets progress.
    pub async fn begin(
        &self,
        id: &OperationId,
        descriptor: &InstallationDescriptor,
        actor: Option<AccountId>,
        now: Timestamp,
    ) -> Result<InstallationOperation, StorageError> {
        let mut tx = self.pool.begin_immediate().await?;
        if let Some(row) =
            sqlx::query("SELECT * FROM plugin_package_operations WHERE id = ?").bind(id.as_str()).fetch_optional(&mut *tx).await?
        {
            let existing = operation(&row)?;
            if existing.descriptor != *descriptor || existing.actor != actor {
                return Err(StorageError::Conflict("package operation token"));
            }
            tx.commit().await?;
            return Ok(existing);
        }
        if let Some(digest) = sqlx::query_scalar::<_, String>("SELECT digest FROM plugin_installations WHERE plugin_id = ? AND version = ?")
            .bind(descriptor.manifest.plugin.id.as_str())
            .bind(descriptor.manifest.plugin.version.to_string())
            .fetch_optional(&mut *tx)
            .await?
            && digest != descriptor.digest.as_str()
        {
            return Err(StorageError::Conflict("package version digest"));
        }
        sqlx::query(
            "INSERT INTO plugin_package_operations (id, plugin_id, version, digest, manifest, wire_bytes, actor, stage, revision, error, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, 0, 0, NULL, ?, ?)",
        )
        .bind(id.as_str())
        .bind(descriptor.manifest.plugin.id.as_str())
        .bind(descriptor.manifest.plugin.version.to_string())
        .bind(descriptor.digest.as_str())
        .bind(descriptor.json()?)
        .bind(i64::try_from(descriptor.wire_bytes).map_err(|_| invalid())?)
        .bind(actor.map(|id| id.0))
        .bind(now.0)
        .bind(now.0)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(InstallationOperation {
            id: id.clone(),
            descriptor: descriptor.clone(),
            actor,
            stage: OperationStage::Pending,
            revision: 0,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn operation(&self, id: &OperationId) -> Result<Option<InstallationOperation>, StorageError> {
        sqlx::query("SELECT * FROM plugin_package_operations WHERE id = ?")
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await?
            .as_ref()
            .map(operation)
            .transpose()
    }

    /// At most128 incomplete operations in token order. Pass the last token to
    /// continue; completed rows never enter recovery enumeration.
    pub async fn recovery(&self, after: Option<&OperationId>) -> Result<Vec<InstallationOperation>, StorageError> {
        sqlx::query("SELECT * FROM plugin_package_operations WHERE stage <> 2 AND id > ? ORDER BY id LIMIT ?")
            .bind(after.map_or("", OperationId::as_str))
            .bind(PAGE_SIZE)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(operation)
            .collect()
    }

    /// Revision-fenced progress; use `complete()` for the terminal atomic pin.
    /// Error can return to Pending for explicit retry. Complete never transitions.
    pub async fn transition(
        &self,
        id: &OperationId,
        expected_revision: i64,
        stage: OperationStage,
        now: Timestamp,
    ) -> Result<InstallationOperation, StorageError> {
        if stage == OperationStage::Complete || !(0..i64::MAX - 1).contains(&expected_revision) {
            return Err(invalid());
        }
        let (stage, error) = stage.codes();
        let mut tx = self.pool.begin_immediate().await?;
        let updated = sqlx::query(
            "UPDATE plugin_package_operations SET stage = ?, error = ?, revision = revision + 1, updated_at = ? \
             WHERE id = ? AND revision = ? AND stage <> 2",
        )
        .bind(stage)
        .bind(error)
        .bind(now.0)
        .bind(id.as_str())
        .bind(expected_revision)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(StorageError::Conflict("package operation revision"));
        }
        let row = sqlx::query("SELECT * FROM plugin_package_operations WHERE id = ?").bind(id.as_str()).fetch_one(&mut *tx).await?;
        let result = operation(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    /// Trusted host assertion: the pinned CAS object is durably published. This
    /// method cannot establish filesystem durability or publisher trust itself.
    /// An Unconfirmed publication receipt must not be passed off as this assertion.
    /// Object metadata, version pin and completion commit in one FULL transaction.
    pub async fn complete(&self, id: &OperationId, expected_revision: i64, now: Timestamp) -> Result<Installation, StorageError> {
        if !(0..i64::MAX - 1).contains(&expected_revision) {
            return Err(invalid());
        }
        let mut tx = self.pool.begin_immediate().await?;
        let row = sqlx::query("SELECT * FROM plugin_package_operations WHERE id = ?")
            .bind(id.as_str())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StorageError::Conflict("package operation"))?;
        let op = operation(&row)?;
        if op.revision != expected_revision || matches!(op.stage, OperationStage::Error(_)) {
            return Err(StorageError::Conflict("package operation revision"));
        }
        let descriptor = &op.descriptor;
        if let Some(row) =
            sqlx::query("SELECT * FROM plugin_objects WHERE digest = ?").bind(descriptor.digest.as_str()).fetch_optional(&mut *tx).await?
        {
            if object(&row)? != *descriptor {
                return Err(StorageError::Conflict("package object metadata"));
            }
        } else {
            sqlx::query("INSERT INTO plugin_objects (digest, manifest, wire_bytes, recorded_at) VALUES (?, ?, ?, ?)")
                .bind(descriptor.digest.as_str())
                .bind(descriptor.json()?)
                .bind(i64::try_from(descriptor.wire_bytes).map_err(|_| invalid())?)
                .bind(now.0)
                .execute(&mut *tx)
                .await?;
        }
        let existing = sqlx::query("SELECT i.*, o.manifest, o.wire_bytes FROM plugin_installations i JOIN plugin_objects o USING(digest) WHERE i.plugin_id = ? AND i.version = ?")
            .bind(descriptor.manifest.plugin.id.as_str()).bind(descriptor.manifest.plugin.version.to_string()).fetch_optional(&mut *tx).await?;
        let installed = if let Some(row) = existing {
            let installed = installation(&row)?;
            if installed.descriptor != *descriptor {
                return Err(StorageError::Conflict("package version digest"));
            }
            installed
        } else {
            if op.stage == OperationStage::Complete {
                return Err(invalid());
            }
            sqlx::query("INSERT INTO plugin_installations (plugin_id, version, digest, installed_by, installed_at) VALUES (?, ?, ?, ?, ?)")
                .bind(descriptor.manifest.plugin.id.as_str())
                .bind(descriptor.manifest.plugin.version.to_string())
                .bind(descriptor.digest.as_str())
                .bind(op.actor.map(|id| id.0))
                .bind(now.0)
                .execute(&mut *tx)
                .await?;
            Installation { descriptor: descriptor.clone(), installed_by: op.actor, installed_at: now }
        };
        if op.stage != OperationStage::Complete {
            sqlx::query(
                "UPDATE plugin_package_operations SET stage = 2, error = NULL, revision = revision + 1, updated_at = ? WHERE id = ?",
            )
            .bind(now.0)
            .bind(id.as_str())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(installed)
    }

    pub async fn installation(&self, id: &PluginId, version: &Version) -> Result<Option<Installation>, StorageError> {
        sqlx::query("SELECT i.*, o.manifest, o.wire_bytes FROM plugin_installations i JOIN plugin_objects o USING(digest) WHERE i.plugin_id = ? AND i.version = ?")
            .bind(id.as_str()).bind(version.to_string()).fetch_optional(&self.pool).await?.as_ref().map(installation).transpose()
    }

    /// Bounded durable object inventory, including retained objects without a
    /// selected runtime. Backup must read its own `SQLite` snapshot for consistency.
    pub async fn objects(&self, after: Option<&Sha256Digest>) -> Result<Vec<InstallationDescriptor>, StorageError> {
        sqlx::query("SELECT * FROM plugin_objects WHERE digest > ? ORDER BY digest LIMIT ?")
            .bind(after.map_or("", Sha256Digest::as_str))
            .bind(PAGE_SIZE)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(object)
            .collect()
    }
}

fn invalid() -> StorageError {
    StorageError::Corrupt("invalid package ledger descriptor or state".to_owned())
}
fn object(row: &SqliteRow) -> Result<InstallationDescriptor, StorageError> {
    let json: String = row.try_get("manifest")?;
    if json.len() > MAX_PACKAGE_MANIFEST_BYTES {
        return Err(invalid());
    }
    InstallationDescriptor::new(
        serde_json::from_str(&json).map_err(|_| invalid())?,
        row.try_get::<String, _>("digest")?.parse().map_err(|_| invalid())?,
        u64::try_from(row.try_get::<i64, _>("wire_bytes")?).map_err(|_| invalid())?,
    )
}
fn check_identity(row: &SqliteRow, descriptor: &InstallationDescriptor) -> Result<(), StorageError> {
    if row.try_get::<String, _>("plugin_id")? != descriptor.manifest.plugin.id.as_str()
        || row.try_get::<String, _>("version")? != descriptor.manifest.plugin.version.to_string()
    {
        return Err(invalid());
    }
    Ok(())
}
fn operation(row: &SqliteRow) -> Result<InstallationOperation, StorageError> {
    let descriptor = object(row)?;
    check_identity(row, &descriptor)?;
    Ok(InstallationOperation {
        id: OperationId::new(&row.try_get::<String, _>("id")?)?,
        descriptor,
        actor: row.try_get::<Option<i64>, _>("actor")?.map(AccountId),
        stage: OperationStage::decode(row.try_get("stage")?, row.try_get("error")?)?,
        revision: row.try_get("revision")?,
        created_at: Timestamp(row.try_get("created_at")?),
        updated_at: Timestamp(row.try_get("updated_at")?),
    })
}
fn installation(row: &SqliteRow) -> Result<Installation, StorageError> {
    let descriptor = object(row)?;
    check_identity(row, &descriptor)?;
    Ok(Installation {
        descriptor,
        installed_by: row.try_get::<Option<i64>, _>("installed_by")?.map(AccountId),
        installed_at: Timestamp(row.try_get("installed_at")?),
    })
}

#[cfg(test)]
mod tests {
    use super::super::Database;

    #[tokio::test]
    async fn package_writer_is_lazy_full_and_ordinary_pool_remains_normal() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(&directory.path().join("rbit.db")).await.unwrap();
        assert_eq!(database.package_pool.size(), 0);
        assert_eq!(database.package_pool.options().get_min_connections(), 0);
        assert_eq!(database.package_pool.options().get_idle_timeout(), None);
        assert_eq!(database.package_pool.options().get_max_lifetime(), None);
        let _store = database.plugin_installations();
        assert_eq!(database.package_pool.size(), 0);
        let normal: i64 = sqlx::query_scalar("PRAGMA synchronous").fetch_one(&database.pool).await.unwrap();
        let full: i64 = sqlx::query_scalar("PRAGMA synchronous").fetch_one(&database.package_pool).await.unwrap();
        assert_eq!(normal, 1);
        assert_eq!(full, 2);
        assert_eq!(database.package_pool.size(), 1);
        let normal: i64 = sqlx::query_scalar("PRAGMA synchronous").fetch_one(&database.pool).await.unwrap();
        assert_eq!(normal, 1);
        database.close().await;
        assert!(database.pool.is_closed());
        assert!(database.package_pool.is_closed());
        assert!(database.plugin_installations().objects(None).await.is_err());
    }
}
