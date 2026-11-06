//! Old-address resolution without weakening visibility or current-address precedence.

use std::future::Future;

use crate::OwnedRepository;

/// Borrowed storage/access adapter. Opening retains transport-specific reader setup;
/// `readable` checks current permissions before revealing a destination address.
pub trait AliasPort: Sync {
    type Error: Send;
    type Opened;
    fn find_current(&self, owner: &str, name: &str) -> impl Future<Output = Result<Option<OwnedRepository>, Self::Error>> + Send;
    fn find_old(&self, owner: &str, name: &str) -> impl Future<Output = Result<Option<OwnedRepository>, Self::Error>> + Send;
    fn open(&self, repository: OwnedRepository) -> impl Future<Output = Result<Self::Opened, Self::Error>> + Send;
    fn readable(&self, repository: &OwnedRepository) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn not_found(&self) -> Self::Error;
}

/// No additional existence query: redirect triggers retire reclaimed aliases.
#[derive(Debug)]
pub struct RepositoryAliases;

impl RepositoryAliases {
    /// Git accepts a trailing `.git`. A current private address must never fall back
    /// to an unrelated old address when opening it fails permission checks.
    pub async fn open<P: AliasPort>(port: &P, owner: &str, name: &str) -> Result<P::Opened, P::Error> {
        let name = name.strip_suffix(".git").unwrap_or(name);
        let repository = match port.find_current(owner, name).await? {
            Some(repository) => repository,
            None => port.find_old(owner, name).await?.ok_or_else(|| port.not_found())?,
        };
        port.open(repository).await
    }

    /// Browser redirect lookup intentionally neither strips `.git` nor queries the
    /// current address. A missing alias performs no grants or reader work.
    pub async fn moved_to<P: AliasPort>(port: &P, owner: &str, name: &str) -> Result<Option<OwnedRepository>, P::Error> {
        let Some(repository) = port.find_old(owner, name).await? else {
            return Ok(None);
        };
        Ok(port.readable(&repository).await?.then_some(repository))
    }
}
