//! Releases and the metadata of their attached files (the bytes live on disk).
//!
//! Milestone 2 workstream R (repository essentials) owns this file. Keep row types here rather than in `rows.rs`.

use std::collections::HashMap;

use rbit_core::{
    AccountId, RepoId, Timestamp,
    release::{Release, ReleaseAsset, ReleaseAssetId, ReleaseAuthor, ReleaseId},
};
use sqlx::SqlitePool;

use crate::StorageError;

/// A release joined with its author's name.
macro_rules! select_release {
    ($($clause:expr),+) => {
        concat!(
            "SELECT r.id, r.repo_id, r.tag_name, r.name, r.notes, r.is_draft, r.is_prerelease, r.author_id, \
             a.name AS author_name, r.created_at, r.updated_at, r.published_at \
             FROM releases r LEFT JOIN accounts a ON a.id = r.author_id ",
            $($clause),+
        )
    };
}

/// Asset columns in [`AssetRow`] order.
macro_rules! select_asset {
    ($clause:expr) => {
        concat!(
            "SELECT id, release_id, repo_id, name, size, content_type, sha256, download_count, uploader_id, created_at \
             FROM release_assets ",
            $clause
        )
    };
}

#[derive(Debug, sqlx::FromRow)]
struct ReleaseRow {
    id: i64,
    repo_id: i64,
    tag_name: String,
    name: String,
    notes: String,
    is_draft: bool,
    is_prerelease: bool,
    author_id: Option<i64>,
    author_name: Option<String>,
    created_at: i64,
    updated_at: i64,
    published_at: Option<i64>,
}

impl From<ReleaseRow> for Release {
    fn from(row: ReleaseRow) -> Self {
        Self {
            id: ReleaseId(row.id),
            repo_id: RepoId(row.repo_id),
            tag_name: row.tag_name,
            name: row.name,
            notes: row.notes,
            is_draft: row.is_draft,
            is_prerelease: row.is_prerelease,
            author: row.author_id.zip(row.author_name).map(|(id, name)| ReleaseAuthor { id: AccountId(id), name }),
            created_at: Timestamp(row.created_at),
            updated_at: Timestamp(row.updated_at),
            published_at: row.published_at.map(Timestamp),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct AssetRow {
    id: i64,
    release_id: i64,
    repo_id: i64,
    name: String,
    size: i64,
    content_type: String,
    sha256: Vec<u8>,
    download_count: i64,
    uploader_id: Option<i64>,
    created_at: i64,
}

impl TryFrom<AssetRow> for ReleaseAsset {
    type Error = StorageError;

    fn try_from(row: AssetRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: ReleaseAssetId(row.id),
            release_id: ReleaseId(row.release_id),
            repo_id: RepoId(row.repo_id),
            name: row.name,
            size: u64::try_from(row.size).map_err(|_| StorageError::Corrupt("negative asset size".to_owned()))?,
            content_type: row.content_type,
            sha256: row.sha256.try_into().map_err(|_| StorageError::Corrupt("asset digest is not 32 bytes".to_owned()))?,
            download_count: u64::try_from(row.download_count).unwrap_or_default(),
            uploader_id: row.uploader_id.map(AccountId),
            created_at: Timestamp(row.created_at),
        })
    }
}

/// A release to insert.
#[derive(Debug, Clone)]
pub struct NewRelease<'a> {
    pub repo_id: RepoId,
    pub tag_name: &'a str,
    pub name: &'a str,
    pub notes: &'a str,
    pub is_draft: bool,
    pub is_prerelease: bool,
    pub author_id: AccountId,
}

/// The editable fields of a release. `published_at` is only ever set once: pass the
/// publication time when a draft is published, `None` to keep the stored value.
#[derive(Debug, Clone)]
pub struct ReleaseChanges<'a> {
    pub tag_name: &'a str,
    pub name: &'a str,
    pub notes: &'a str,
    pub is_draft: bool,
    pub is_prerelease: bool,
    pub published_at: Option<Timestamp>,
}

/// Metadata of an uploaded file, inserted once its bytes are safely on disk.
#[derive(Debug, Clone)]
pub struct NewAsset<'a> {
    pub release_id: ReleaseId,
    pub repo_id: RepoId,
    pub name: &'a str,
    pub size: u64,
    pub content_type: &'a str,
    pub sha256: [u8; 32],
    pub uploader_id: AccountId,
}

#[derive(Debug, Clone)]
pub struct ReleaseStore {
    pool: SqlitePool,
}

impl ReleaseStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Inserts a release; [`StorageError::Conflict`] when the tag already has one.
    pub async fn create(&self, new: NewRelease<'_>) -> Result<ReleaseId, StorageError> {
        let now = Timestamp::now().seconds();
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO releases (repo_id, tag_name, name, notes, is_draft, is_prerelease, author_id, created_at, updated_at, published_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, CASE WHEN ?5 THEN NULL ELSE ?8 END) RETURNING id",
        )
        .bind(new.repo_id.0)
        .bind(new.tag_name)
        .bind(new.name)
        .bind(new.notes)
        .bind(new.is_draft)
        .bind(new.is_prerelease)
        .bind(new.author_id.0)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::from_insert(e, "a release for this tag"))?;
        Ok(ReleaseId(id))
    }

    /// Applies `changes`; [`StorageError::Conflict`] when the new tag already has a release.
    pub async fn update(&self, id: ReleaseId, changes: ReleaseChanges<'_>) -> Result<(), StorageError> {
        sqlx::query(
            "UPDATE releases SET tag_name = ?, name = ?, notes = ?, is_draft = ?, is_prerelease = ?, updated_at = ?, \
             published_at = COALESCE(published_at, ?) WHERE id = ?",
        )
        .bind(changes.tag_name)
        .bind(changes.name)
        .bind(changes.notes)
        .bind(changes.is_draft)
        .bind(changes.is_prerelease)
        .bind(Timestamp::now().seconds())
        .bind(changes.published_at.map(Timestamp::seconds))
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::from_insert(e, "a release for this tag"))?;
        Ok(())
    }

    /// Removes a release and (by cascade) its asset rows; the caller removes the files.
    pub async fn delete(&self, id: ReleaseId) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM releases WHERE id = ?").bind(id.0).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn find(&self, repo: RepoId, id: ReleaseId) -> Result<Option<Release>, StorageError> {
        let row: Option<ReleaseRow> =
            sqlx::query_as(select_release!("WHERE r.repo_id = ? AND r.id = ?")).bind(repo.0).bind(id.0).fetch_optional(&self.pool).await?;
        Ok(row.map(Release::from))
    }

    pub async fn find_by_tag(&self, repo: RepoId, tag: &str) -> Result<Option<Release>, StorageError> {
        let row: Option<ReleaseRow> = sqlx::query_as(select_release!("WHERE r.repo_id = ? AND r.tag_name = ?"))
            .bind(repo.0)
            .bind(tag)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(Release::from))
    }

    /// One page of releases: drafts first (when included), then newest published first.
    pub async fn list(&self, repo: RepoId, include_drafts: bool, limit: u32, offset: u32) -> Result<Vec<Release>, StorageError> {
        let rows: Vec<ReleaseRow> = sqlx::query_as(select_release!(
            "WHERE r.repo_id = ? AND (? OR r.is_draft = 0) \
             ORDER BY r.is_draft DESC, COALESCE(r.published_at, r.created_at) DESC, r.id DESC LIMIT ? OFFSET ?"
        ))
        .bind(repo.0)
        .bind(include_drafts)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Release::from).collect())
    }

    pub async fn count(&self, repo: RepoId, include_drafts: bool) -> Result<u64, StorageError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM releases WHERE repo_id = ? AND (? OR is_draft = 0)")
            .bind(repo.0)
            .bind(include_drafts)
            .fetch_one(&self.pool)
            .await?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    /// The most recently published release, preferring full releases over pre-releases.
    pub async fn latest(&self, repo: RepoId) -> Result<Option<Release>, StorageError> {
        let row: Option<ReleaseRow> = sqlx::query_as(select_release!(
            "WHERE r.repo_id = ? AND r.is_draft = 0 ORDER BY r.is_prerelease, r.published_at DESC, r.id DESC LIMIT 1"
        ))
        .bind(repo.0)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Release::from))
    }

    // ------------------------------------------------------------------ assets

    /// Records an uploaded file; [`StorageError::Conflict`] when the name is taken.
    pub async fn insert_asset(&self, new: NewAsset<'_>) -> Result<ReleaseAsset, StorageError> {
        self.insert_asset_limited(new, u32::MAX).await?.ok_or_else(|| StorageError::Corrupt("too many release assets".to_owned()))
    }

    /// Records an uploaded file only while the release remains below its limit. The
    /// check and insert share a write statement, so simultaneous uploads cannot race it.
    pub async fn insert_asset_limited(&self, new: NewAsset<'_>, limit: u32) -> Result<Option<ReleaseAsset>, StorageError> {
        let size = i64::try_from(new.size).map_err(|_| StorageError::Corrupt("asset too large".to_owned()))?;
        let row: Option<AssetRow> = sqlx::query_as(
            "INSERT INTO release_assets (release_id, repo_id, name, size, content_type, sha256, uploader_id, created_at) \
             SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8 \
             WHERE (SELECT COUNT(*) FROM release_assets WHERE release_id = ?1) < ?9 \
             RETURNING id, release_id, repo_id, name, size, content_type, sha256, download_count, uploader_id, created_at",
        )
        .bind(new.release_id.0)
        .bind(new.repo_id.0)
        .bind(new.name)
        .bind(size)
        .bind(new.content_type)
        .bind(new.sha256.as_slice())
        .bind(new.uploader_id.0)
        .bind(Timestamp::now().seconds())
        .bind(limit)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::from_insert(e, "a file with this name"))?;
        row.map(ReleaseAsset::try_from).transpose()
    }

    pub async fn delete_asset(&self, id: ReleaseAssetId) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM release_assets WHERE id = ?").bind(id.0).execute(&self.pool).await?;
        Ok(())
    }

    /// The files of one release, by name.
    pub async fn assets(&self, release: ReleaseId) -> Result<Vec<ReleaseAsset>, StorageError> {
        let rows: Vec<AssetRow> =
            sqlx::query_as(select_asset!("WHERE release_id = ? ORDER BY name")).bind(release.0).fetch_all(&self.pool).await?;
        rows.into_iter().map(ReleaseAsset::try_from).collect()
    }

    /// The files of many releases in one query, grouped by release.
    pub async fn assets_of(&self, releases: &[ReleaseId]) -> Result<HashMap<ReleaseId, Vec<ReleaseAsset>>, StorageError> {
        if releases.is_empty() {
            return Ok(HashMap::new());
        }
        let ids = format!("[{}]", releases.iter().map(|id| id.0.to_string()).collect::<Vec<_>>().join(","));
        let rows: Vec<AssetRow> = sqlx::query_as(select_asset!("WHERE release_id IN (SELECT value FROM json_each(?)) ORDER BY name"))
            .bind(ids)
            .fetch_all(&self.pool)
            .await?;
        let mut grouped: HashMap<ReleaseId, Vec<ReleaseAsset>> = HashMap::new();
        for row in rows {
            let asset = ReleaseAsset::try_from(row)?;
            grouped.entry(asset.release_id).or_default().push(asset);
        }
        Ok(grouped)
    }

    pub async fn count_assets(&self, release: ReleaseId) -> Result<u32, StorageError> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM release_assets WHERE release_id = ?").bind(release.0).fetch_one(&self.pool).await?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }

    /// File ids still referenced by a repository, for cleaning up delayed deletion
    /// events without removing assets of a newly created repository with a reused id.
    pub async fn asset_ids_for_repo(&self, repo: RepoId) -> Result<Vec<ReleaseAssetId>, StorageError> {
        let ids: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM release_assets WHERE repo_id = ?").bind(repo.0).fetch_all(&self.pool).await?;
        Ok(ids.into_iter().map(ReleaseAssetId).collect())
    }

    pub async fn find_asset(&self, release: ReleaseId, name: &str) -> Result<Option<ReleaseAsset>, StorageError> {
        let row: Option<AssetRow> = sqlx::query_as(select_asset!("WHERE release_id = ? AND name = ?"))
            .bind(release.0)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;
        row.map(ReleaseAsset::try_from).transpose()
    }

    /// File `id` of any release of `repo`.
    pub async fn find_asset_by_id(&self, repo: RepoId, id: ReleaseAssetId) -> Result<Option<ReleaseAsset>, StorageError> {
        let row: Option<AssetRow> =
            sqlx::query_as(select_asset!("WHERE repo_id = ? AND id = ?")).bind(repo.0).bind(id.0).fetch_optional(&self.pool).await?;
        row.map(ReleaseAsset::try_from).transpose()
    }

    /// Best-effort download counter.
    pub async fn count_download(&self, id: ReleaseAssetId) -> Result<(), StorageError> {
        sqlx::query("UPDATE release_assets SET download_count = download_count + 1 WHERE id = ?").bind(id.0).execute(&self.pool).await?;
        Ok(())
    }
}
