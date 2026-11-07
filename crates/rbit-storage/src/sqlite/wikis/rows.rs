//! Checked decoding: corrupt catalog strings cannot silently normalize to new keys.
use crate::StorageError;
use rbit_core::{RepoId, Timestamp};
use rbit_wikis::{WikiGeneration, WikiId, WikiPage, WikiPageId, WikiPath, WikiRevisionId, WikiStoreId, WikiTitle, policy::WikiHead};

pub(super) fn corrupt() -> StorageError {
    StorageError::Corrupt("wiki catalog format".into())
}

pub(super) fn identity(bytes: Vec<u8>) -> Result<WikiStoreId, StorageError> {
    WikiStoreId::try_from(<[u8; 16]>::try_from(bytes).map_err(|_| corrupt())?).map_err(|_| corrupt())
}

pub(super) fn head(generation: i64, revision: &str) -> Result<WikiHead, StorageError> {
    Ok(WikiHead {
        generation: WikiGeneration::new(generation).ok_or_else(corrupt)?,
        revision: revision.parse::<WikiRevisionId>().map_err(|_| corrupt())?,
    })
}

pub(super) fn names(path: String, title: String) -> Result<(WikiPath, WikiTitle), StorageError> {
    Ok((WikiPath::from_canonical(path).map_err(|_| corrupt())?, WikiTitle::from_canonical(title).map_err(|_| corrupt())?))
}

#[derive(sqlx::FromRow)]
pub(super) struct MetaRow {
    wiki_id: i64,
    generation: Option<i64>,
    revision: Option<String>,
    identity: Option<Vec<u8>>,
}
impl MetaRow {
    pub fn decode(self) -> Result<(WikiStoreId, WikiId, Option<WikiHead>), StorageError> {
        let head = match (self.generation, self.revision) {
            (None, None) => None,
            (Some(generation), Some(revision)) => Some(super::rows::head(generation, &revision)?),
            _ => return Err(corrupt()),
        };
        Ok((identity(self.identity.ok_or_else(corrupt)?)?, WikiId::new(self.wiki_id).ok_or_else(corrupt)?, head))
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct PageRow {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
}
impl PageRow {
    pub fn decode(self, repository: RepoId, wiki: WikiId, head: WikiHead) -> Result<WikiPage, StorageError> {
        let (path, title) = names(self.path, self.title)?;
        Ok(WikiPage {
            repository,
            wiki,
            id: WikiPageId::new(self.id).ok_or_else(corrupt)?,
            path,
            title,
            head,
            created_at: Timestamp(self.created_at),
            updated_at: Timestamp(self.updated_at),
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct RevisionPageRow {
    #[sqlx(flatten)]
    pub page: PageRow,
    pub generation: i64,
    pub revision: String,
}
