//! Release persistence over the existing release store and SQL.
//!
//! Every method is the store method of the same name; the store keeps one release per
//! tag and the per-release file limit inside its transactions.
use std::collections::HashMap;

use crate::{StorageError, sqlite::releases::ReleaseStore};
use rbit_core::RepoId;
use rbit_repositories::releases::{
    NewAsset, NewRelease, Release, ReleaseAsset, ReleaseAssetId, ReleaseChanges, ReleaseId, ReleaseRepository,
};

impl ReleaseRepository for ReleaseStore {
    type Error = StorageError;

    async fn count(&self, repo: RepoId, include_drafts: bool) -> Result<u64, StorageError> {
        Self::count(self, repo, include_drafts).await
    }
    async fn list(&self, repo: RepoId, include_drafts: bool, limit: u32, offset: u32) -> Result<Vec<Release>, StorageError> {
        Self::list(self, repo, include_drafts, limit, offset).await
    }
    async fn assets_of(&self, releases: &[ReleaseId]) -> Result<HashMap<ReleaseId, Vec<ReleaseAsset>>, StorageError> {
        Self::assets_of(self, releases).await
    }
    async fn find(&self, repo: RepoId, id: ReleaseId) -> Result<Option<Release>, StorageError> {
        Self::find(self, repo, id).await
    }
    async fn find_by_tag(&self, repo: RepoId, tag: &str) -> Result<Option<Release>, StorageError> {
        Self::find_by_tag(self, repo, tag).await
    }
    async fn latest(&self, repo: RepoId) -> Result<Option<Release>, StorageError> {
        Self::latest(self, repo).await
    }
    async fn assets(&self, release: ReleaseId) -> Result<Vec<ReleaseAsset>, StorageError> {
        Self::assets(self, release).await
    }
    async fn create(&self, new: NewRelease<'_>) -> Result<ReleaseId, StorageError> {
        Self::create(self, new).await
    }
    async fn update(&self, id: ReleaseId, changes: ReleaseChanges<'_>) -> Result<(), StorageError> {
        Self::update(self, id, changes).await
    }
    async fn delete(&self, id: ReleaseId) -> Result<(), StorageError> {
        Self::delete(self, id).await
    }
    async fn count_assets(&self, release: ReleaseId) -> Result<u32, StorageError> {
        Self::count_assets(self, release).await
    }
    async fn find_asset(&self, release: ReleaseId, name: &str) -> Result<Option<ReleaseAsset>, StorageError> {
        Self::find_asset(self, release, name).await
    }
    async fn find_asset_by_id(&self, repo: RepoId, id: ReleaseAssetId) -> Result<Option<ReleaseAsset>, StorageError> {
        Self::find_asset_by_id(self, repo, id).await
    }
    async fn insert_asset_limited(&self, new: NewAsset<'_>, limit: u32) -> Result<Option<ReleaseAsset>, StorageError> {
        Self::insert_asset_limited(self, new, limit).await
    }
    async fn delete_asset(&self, id: ReleaseAssetId) -> Result<(), StorageError> {
        Self::delete_asset(self, id).await
    }
    async fn count_download(&self, id: ReleaseAssetId) -> Result<(), StorageError> {
        Self::count_download(self, id).await
    }
    async fn asset_ids_for_repo(&self, repo: RepoId) -> Result<Vec<ReleaseAssetId>, StorageError> {
        Self::asset_ids_for_repo(self, repo).await
    }
}
