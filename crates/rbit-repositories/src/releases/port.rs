//! Persistence, Git and file ports for release commands.
//!
//! SQL stays authoritative for one release per tag and for the per-release file limit
//! (`insert_asset_limited` counts inside its own transaction). The file port owns the
//! bytes: temporary files, placement under numeric ids and their removal.

use std::{collections::HashMap, future::Future};

use rbit_contracts::{AccountId, RepoId};

use super::model::{NewAsset, NewRelease, Release, ReleaseAsset, ReleaseAssetId, ReleaseChanges, ReleaseId, ReleaseSource};

/// Release rows and the metadata of their attached files.
pub trait ReleaseRepository: Sync {
    type Error: Send;

    fn count(&self, repo: RepoId, include_drafts: bool) -> impl Future<Output = Result<u64, Self::Error>> + Send;
    fn list(
        &self,
        repo: RepoId,
        include_drafts: bool,
        limit: u32,
        offset: u32,
    ) -> impl Future<Output = Result<Vec<Release>, Self::Error>> + Send;
    fn assets_of(&self, releases: &[ReleaseId]) -> impl Future<Output = Result<HashMap<ReleaseId, Vec<ReleaseAsset>>, Self::Error>> + Send;
    fn find(&self, repo: RepoId, id: ReleaseId) -> impl Future<Output = Result<Option<Release>, Self::Error>> + Send;
    fn find_by_tag(&self, repo: RepoId, tag: &str) -> impl Future<Output = Result<Option<Release>, Self::Error>> + Send;
    fn latest(&self, repo: RepoId) -> impl Future<Output = Result<Option<Release>, Self::Error>> + Send;
    fn assets(&self, release: ReleaseId) -> impl Future<Output = Result<Vec<ReleaseAsset>, Self::Error>> + Send;
    /// A second release for the same tag is the store's conflict.
    fn create(&self, new: NewRelease<'_>) -> impl Future<Output = Result<ReleaseId, Self::Error>> + Send;
    fn update(&self, id: ReleaseId, changes: ReleaseChanges<'_>) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn delete(&self, id: ReleaseId) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn count_assets(&self, release: ReleaseId) -> impl Future<Output = Result<u32, Self::Error>> + Send;
    fn find_asset(&self, release: ReleaseId, name: &str) -> impl Future<Output = Result<Option<ReleaseAsset>, Self::Error>> + Send;
    fn find_asset_by_id(&self, repo: RepoId, id: ReleaseAssetId) -> impl Future<Output = Result<Option<ReleaseAsset>, Self::Error>> + Send;
    /// Inserts unless the release already has `limit` files (`None`), atomically.
    fn insert_asset_limited(&self, new: NewAsset<'_>, limit: u32)
    -> impl Future<Output = Result<Option<ReleaseAsset>, Self::Error>> + Send;
    fn delete_asset(&self, id: ReleaseAssetId) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn count_download(&self, id: ReleaseAssetId) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn asset_ids_for_repo(&self, repo: RepoId) -> impl Future<Output = Result<Vec<ReleaseAssetId>, Self::Error>> + Send;
}

/// Why a tag for a new release could not be created.
#[derive(Debug)]
pub enum TagError<E> {
    InvalidName,
    Exists,
    /// The branch or commit to create it from does not exist.
    MissingTarget,
    Failure(E),
}

/// Request-local effects of creating or editing a release.
pub trait ReleasePort: Sync {
    type Store: ReleaseRepository;
    type Error: Send + From<<Self::Store as ReleaseRepository>::Error>;

    fn store(&self) -> &Self::Store;
    fn tag_exists(&self, tag: &str) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Creates `tag` at `target`, reported like a push of the tag.
    fn create_tag(&self, tag: &str, target: &str) -> impl Future<Output = Result<(), TagError<Self::Error>>> + Send;
    /// Announces a release that stopped being a draft (or never was one).
    fn published(&self, release: ReleaseId, actor: Option<AccountId>);
    fn created(&self, tag: &str, draft: bool);
}

/// Metadata of an attached file whose bytes are complete, before it gets a row.
#[derive(Debug, Clone)]
pub struct AssetUpload {
    pub release_id: ReleaseId,
    pub repo_id: RepoId,
    pub name: String,
    /// Sanitized; never empty.
    pub content_type: String,
    pub uploader_id: Option<AccountId>,
    pub source: ReleaseSource,
}

/// The bytes of one upload.
pub trait AssetFiles: Send {
    type Received: Send;
    type Error: Send;

    /// Streams the body into a temporary file while hashing and measuring it; `None`
    /// (with the temporary file gone) as soon as it exceeds `max_bytes`.
    fn receive(&mut self, max_bytes: u64) -> impl Future<Output = Result<Option<Self::Received>, Self::Error>> + Send;
    /// Inserts the row within `limit` files and moves the bytes into place, independently
    /// of request cancellation; `None` (bytes discarded) when the release is full.
    fn place(
        &mut self,
        upload: AssetUpload,
        received: Self::Received,
        limit: u32,
    ) -> impl Future<Output = Result<Option<ReleaseAsset>, Self::Error>> + Send;
}
