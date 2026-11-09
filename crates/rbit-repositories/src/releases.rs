//! Releases: notes on a tag, and attached files, with their permissions and limits.
//!
//! # Rules
//!
//! * Anyone who can read the repository sees its published releases; drafts are visible
//!   to writers only (to everyone else a draft does not exist). Writers of a repository
//!   that is not archived create, edit, publish and delete releases.
//! * A release names a tag of the repository, at most one release per tag. A new
//!   release may create its tag from a branch or commit (`ReleaseInput::target`).
//! * Publishing — creating a release as non-draft, or turning a draft into a release —
//!   is announced once ([`ReleasePort::published`]).
//! * A workflow task ([`ReleaseWriter::Task`]) writes only to its own repository and only
//!   with `contents: write`; it has no author and its source is `actions`.
//! * Attached files: valid names, unique per release, at most `max_assets` per release
//!   and `max_bytes` each. File bytes are the [`AssetFiles`] port's; names never become
//!   paths.
//!
//! The caller passes facts it already loaded (access, archived state, its limits) and
//! keeps locks, cancellation and tracing; every decision and message here is final.

pub mod model;
mod port;

use rbit_contracts::{AccountId, RepoId, Timestamp};

pub use model::*;
pub use port::{AssetFiles, AssetUpload, ReleasePort, ReleaseRepository, TagError};

/// Longest declared content type kept for an attached file.
pub const MAX_CONTENT_TYPE_LEN: usize = 255;
/// What a file without a declared content type is stored as.
const DEFAULT_CONTENT_TYPE: &str = "application/octet-stream";

/// What the caller already knows about the repository and the viewer.
#[derive(Debug, Clone, Copy)]
pub struct ReleaseAccess {
    pub repo: RepoId,
    pub can_write: bool,
    pub archived: bool,
}

/// Who makes or changes a release: a person, or a workflow task with its token's grant.
#[derive(Debug, Clone, Copy)]
pub enum ReleaseWriter {
    Person(AccountId),
    Task { repo: RepoId, contents_write: bool },
}

impl ReleaseWriter {
    /// The person, recorded as author or uploader; `None` for a task.
    pub const fn account(self) -> Option<AccountId> {
        match self {
            Self::Person(id) => Some(id),
            Self::Task { .. } => None,
        }
    }

    pub const fn source(self) -> ReleaseSource {
        match self {
            Self::Person(_) => ReleaseSource::User,
            Self::Task { .. } => ReleaseSource::Actions,
        }
    }

    /// A task changes only its own repository, and only with `contents: write`.
    fn check<E>(self, access: ReleaseAccess) -> Result<(), ReleaseError<E>> {
        match self {
            Self::Task { repo, contents_write } if repo != access.repo || !contents_write => Err(ReleaseError::Forbidden),
            _ => Ok(()),
        }
    }
}

/// Size limits of attached files, from the instance configuration.
#[derive(Debug, Clone, Copy)]
pub struct AssetLimits {
    pub max_assets: u32,
    pub max_bytes: u64,
}

/// Refusals carry final messages; adapter failures keep their identity.
#[derive(Debug)]
pub enum ReleaseError<E> {
    NotFound,
    Forbidden,
    /// The repository is archived and read-only.
    Archived,
    Invalid(String),
    /// What is already taken.
    Conflict(&'static str),
    Failure(E),
}

/// Release commands and queries over the ports.
#[derive(Debug)]
pub struct Releases;

impl Releases {
    /// Writers of a repository that is not archived may change its releases.
    pub fn require_writer<E>(access: ReleaseAccess) -> Result<(), ReleaseError<E>> {
        if !access.can_write {
            return Err(ReleaseError::Forbidden);
        }
        if access.archived { Err(ReleaseError::Archived) } else { Ok(()) }
    }

    /// One page (1-based, clamped) of the repository's releases, newest first; drafts
    /// lead the list for writers and are absent for everyone else.
    pub async fn list<S: ReleaseRepository>(store: &S, access: ReleaseAccess, page: u32) -> Result<ReleasePage, ReleaseError<S::Error>> {
        let total = store.count(access.repo, access.can_write).await.map_err(ReleaseError::Failure)?;
        let mut result = ReleasePage { releases: Vec::new(), total, page: page.max(1) };
        result.page = result.page.min(result.page_count());
        let offset = (result.page - 1).saturating_mul(RELEASES_PER_PAGE);
        result.releases = Self::details(store, access, RELEASES_PER_PAGE, offset).await?;
        Ok(result)
    }

    /// `limit` releases from `offset`, ordered like [`Self::list`], and how many in all.
    pub async fn page<S: ReleaseRepository>(
        store: &S,
        access: ReleaseAccess,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<ReleaseDetail>, u64), ReleaseError<S::Error>> {
        let total = store.count(access.repo, access.can_write).await.map_err(ReleaseError::Failure)?;
        Ok((Self::details(store, access, limit, offset).await?, total))
    }

    async fn details<S: ReleaseRepository>(
        store: &S,
        access: ReleaseAccess,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ReleaseDetail>, ReleaseError<S::Error>> {
        let releases = store.list(access.repo, access.can_write, limit, offset).await.map_err(ReleaseError::Failure)?;
        let ids: Vec<_> = releases.iter().map(|r| r.id).collect();
        let mut assets = store.assets_of(&ids).await.map_err(ReleaseError::Failure)?;
        Ok(releases.into_iter().map(|release| ReleaseDetail { assets: assets.remove(&release.id).unwrap_or_default(), release }).collect())
    }

    /// The accounts that wrote `releases` or uploaded their files, sorted and unique.
    pub fn people(releases: &[ReleaseDetail]) -> Vec<AccountId> {
        let mut ids: Vec<_> = releases
            .iter()
            .flat_map(|detail| {
                detail.release.author.iter().map(|author| author.id).chain(detail.assets.iter().filter_map(|a| a.uploader_id))
            })
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// The release of `tag`, if the viewer may see it.
    pub async fn by_tag<S: ReleaseRepository>(
        store: &S,
        access: ReleaseAccess,
        tag: &str,
    ) -> Result<ReleaseDetail, ReleaseError<S::Error>> {
        let release = store.find_by_tag(access.repo, tag).await.map_err(ReleaseError::Failure)?;
        Self::visible(store, access, release).await
    }

    /// Release `id`, if the viewer may see it.
    pub async fn by_id<S: ReleaseRepository>(
        store: &S,
        access: ReleaseAccess,
        id: ReleaseId,
    ) -> Result<ReleaseDetail, ReleaseError<S::Error>> {
        let release = store.find(access.repo, id).await.map_err(ReleaseError::Failure)?;
        Self::visible(store, access, release).await
    }

    async fn visible<S: ReleaseRepository>(
        store: &S,
        access: ReleaseAccess,
        release: Option<Release>,
    ) -> Result<ReleaseDetail, ReleaseError<S::Error>> {
        let release = release.filter(|r| !r.is_draft || access.can_write).ok_or(ReleaseError::<S::Error>::NotFound)?;
        let assets = store.assets(release.id).await.map_err(ReleaseError::Failure)?;
        Ok(ReleaseDetail { release, assets })
    }

    /// File `name` of the release of `tag`, if the viewer may see the release.
    pub async fn asset_named<S: ReleaseRepository>(
        store: &S,
        access: ReleaseAccess,
        tag: &str,
        name: &str,
    ) -> Result<ReleaseAsset, ReleaseError<S::Error>> {
        let release = Self::by_tag(store, access, tag).await?.release;
        store.find_asset(release.id, name).await.map_err(ReleaseError::Failure)?.ok_or(ReleaseError::NotFound)
    }

    /// File `id` of any release of the repository and that release, if the viewer may see it.
    pub async fn asset<S: ReleaseRepository>(
        store: &S,
        access: ReleaseAccess,
        id: ReleaseAssetId,
    ) -> Result<(ReleaseDetail, ReleaseAsset), ReleaseError<S::Error>> {
        let asset =
            store.find_asset_by_id(access.repo, id).await.map_err(ReleaseError::Failure)?.ok_or(ReleaseError::<S::Error>::NotFound)?;
        let release = Self::by_id(store, access, asset.release_id).await?;
        Ok((release, asset))
    }

    /// Checks the text fields of `input`. Returns the trimmed tag and the title (the tag
    /// when left empty).
    fn validated_text<E>(input: &ReleaseInput) -> Result<(String, String), ReleaseError<E>> {
        let tag = input.tag_name.trim();
        if tag.is_empty() {
            return Err(ReleaseError::Invalid("choose the tag this release is for".to_owned()));
        }
        let name = match input.name.trim() {
            "" => tag,
            name => name,
        };
        validate_release_name(name).map_err(ReleaseError::<E>::Invalid)?;
        validate_release_notes(&input.notes).map_err(ReleaseError::<E>::Invalid)?;
        Ok((tag.to_owned(), name.to_owned()))
    }

    /// Creates a release, creating its tag from `input.target` when it does not exist.
    pub async fn create<P: ReleasePort>(
        port: &P,
        access: ReleaseAccess,
        writer: ReleaseWriter,
        input: ReleaseInput,
    ) -> Result<Release, ReleaseError<P::Error>> {
        writer.check::<P::Error>(access)?;
        Self::require_writer::<P::Error>(access)?;
        let (tag, name) = Self::validated_text::<P::Error>(&input)?;
        let store = port.store();
        if !port.tag_exists(&tag).await.map_err(ReleaseError::Failure)? {
            let target = input.target.trim();
            if target.is_empty() {
                return Err(ReleaseError::Invalid(format!("there is no tag called {tag}; push it or choose a branch to create it from")));
            }
            if store.find_by_tag(access.repo, &tag).await.map_err(fail::<P>)?.is_some() {
                return Err(ReleaseError::Conflict("a release for this tag"));
            }
            port.create_tag(&tag, target).await.map_err(|error| match error {
                TagError::InvalidName => ReleaseError::Invalid(format!("{tag} is not a valid tag name")),
                TagError::Exists => ReleaseError::Conflict("the tag"),
                TagError::MissingTarget => ReleaseError::Invalid(format!("there is no branch or commit called {target}")),
                TagError::Failure(error) => ReleaseError::Failure(error),
            })?;
        }
        let new = NewRelease {
            repo_id: access.repo,
            tag_name: &tag,
            name: &name,
            notes: &input.notes,
            is_draft: input.is_draft,
            is_prerelease: input.is_prerelease,
            author_id: writer.account(),
            source: writer.source(),
        };
        let id = store.create(new).await.map_err(fail::<P>)?;
        if !input.is_draft {
            port.published(id, writer.account());
        }
        port.created(&tag, input.is_draft);
        store.find(access.repo, id).await.map_err(fail::<P>)?.ok_or(ReleaseError::NotFound)
    }

    /// Edits release `id`; its tag must exist. Turning a draft into a release publishes it
    /// (once: the first publication date is kept if it is made a draft and re-published).
    pub async fn update<P: ReleasePort>(
        port: &P,
        access: ReleaseAccess,
        writer: ReleaseWriter,
        id: ReleaseId,
        input: ReleaseInput,
    ) -> Result<Release, ReleaseError<P::Error>> {
        writer.check::<P::Error>(access)?;
        Self::require_writer::<P::Error>(access)?;
        let store = port.store();
        let current = store.find(access.repo, id).await.map_err(fail::<P>)?.ok_or(ReleaseError::<P::Error>::NotFound)?;
        let (tag, name) = Self::validated_text::<P::Error>(&input)?;
        if !port.tag_exists(&tag).await.map_err(ReleaseError::Failure)? {
            return Err(ReleaseError::Invalid(format!("there is no tag called {tag}; push it first")));
        }
        let publishing = current.published_at.is_none() && !input.is_draft;
        let changes = ReleaseChanges {
            tag_name: &tag,
            name: &name,
            notes: &input.notes,
            is_draft: input.is_draft,
            is_prerelease: input.is_prerelease,
            published_at: (!input.is_draft).then(Timestamp::now),
        };
        store.update(id, changes).await.map_err(fail::<P>)?;
        if publishing {
            port.published(id, writer.account());
        }
        store.find(access.repo, id).await.map_err(fail::<P>)?.ok_or(ReleaseError::NotFound)
    }

    /// Deletes the row of release `id` (the tag stays) and returns it with the files whose
    /// bytes the caller removes next. Call after [`Self::require_writer`].
    pub async fn delete<S: ReleaseRepository>(
        store: &S,
        repo: RepoId,
        id: ReleaseId,
    ) -> Result<(Release, Vec<ReleaseAsset>), ReleaseError<S::Error>> {
        let release = store.find(repo, id).await.map_err(ReleaseError::Failure)?.ok_or(ReleaseError::<S::Error>::NotFound)?;
        let assets = store.assets(release.id).await.map_err(ReleaseError::Failure)?;
        store.delete(release.id).await.map_err(ReleaseError::Failure)?;
        Ok((release, assets))
    }

    /// Deletes the row of file `asset` of release `id`, before its bytes. Call after
    /// [`Self::require_writer`].
    pub async fn delete_asset<S: ReleaseRepository>(
        store: &S,
        repo: RepoId,
        id: ReleaseId,
        asset: ReleaseAssetId,
    ) -> Result<ReleaseAsset, ReleaseError<S::Error>> {
        let release = store.find(repo, id).await.map_err(ReleaseError::Failure)?.ok_or(ReleaseError::<S::Error>::NotFound)?;
        let assets = store.assets(release.id).await.map_err(ReleaseError::Failure)?;
        let asset = assets.into_iter().find(|a| a.id == asset).ok_or(ReleaseError::<S::Error>::NotFound)?;
        store.delete_asset(asset.id).await.map_err(ReleaseError::Failure)?;
        Ok(asset)
    }

    /// Stores the body read by `files` as the file `name` of release `id`.
    #[allow(clippy::too_many_arguments)]
    pub async fn upload<P: ReleasePort, F: AssetFiles<Error = P::Error>>(
        port: &P,
        files: &mut F,
        access: ReleaseAccess,
        writer: ReleaseWriter,
        id: ReleaseId,
        name: &str,
        content_type: &str,
        limits: AssetLimits,
    ) -> Result<ReleaseAsset, ReleaseError<P::Error>> {
        writer.check::<P::Error>(access)?;
        Self::require_writer::<P::Error>(access)?;
        let store = port.store();
        let release = store.find(access.repo, id).await.map_err(fail::<P>)?.ok_or(ReleaseError::<P::Error>::NotFound)?;
        validate_asset_name(name).map_err(ReleaseError::<P::Error>::Invalid)?;
        let full = || ReleaseError::Invalid(format!("a release can have at most {} files", limits.max_assets));
        if store.count_assets(release.id).await.map_err(fail::<P>)? >= limits.max_assets {
            return Err(full());
        }
        if store.find_asset(release.id, name).await.map_err(fail::<P>)?.is_some() {
            return Err(ReleaseError::Conflict("a file with this name"));
        }
        let Some(received) = files.receive(limits.max_bytes).await.map_err(ReleaseError::Failure)? else {
            return Err(ReleaseError::Invalid(format!("files can be at most {}", human_bytes(limits.max_bytes))));
        };
        let content_type: String = content_type.trim().chars().filter(|c| !c.is_control()).take(MAX_CONTENT_TYPE_LEN).collect();
        let upload = AssetUpload {
            release_id: release.id,
            repo_id: access.repo,
            name: name.to_owned(),
            content_type: if content_type.is_empty() { DEFAULT_CONTENT_TYPE.to_owned() } else { content_type },
            uploader_id: writer.account(),
            source: writer.source(),
        };
        files.place(upload, received, limits.max_assets).await.map_err(ReleaseError::Failure)?.ok_or_else(full)
    }
}

/// A store failure as the port's own error.
fn fail<P: ReleasePort>(error: <P::Store as ReleaseRepository>::Error) -> ReleaseError<P::Error> {
    ReleaseError::Failure(error.into())
}
