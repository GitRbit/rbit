//! Old-address resolution without weakening visibility or current-address precedence.
//!
//! Renamed and transferred repositories keep their former `owner/name` addresses
//! (migration 0016 triggers record and retire them). Every caller that follows old
//! addresses resolves them here, so a current address always wins.

use std::future::Future;

use crate::OwnedRepository;

/// Current and former repository addresses, exactly as stored.
pub trait RepositoryAddresses: Sync {
    type Error: Send;
    fn find_current(&self, owner: &str, name: &str) -> impl Future<Output = Result<Option<OwnedRepository>, Self::Error>> + Send;
    fn find_old(&self, owner: &str, name: &str) -> impl Future<Output = Result<Option<OwnedRepository>, Self::Error>> + Send;
}

/// Borrowed storage/access adapter. Opening retains transport-specific reader setup;
/// `readable` checks current permissions before revealing a destination address.
pub trait AliasPort: Sync {
    type Addresses: RepositoryAddresses;
    type Error: Send + From<<Self::Addresses as RepositoryAddresses>::Error>;
    type Opened;
    fn addresses(&self) -> &Self::Addresses;
    fn open(&self, repository: OwnedRepository) -> impl Future<Output = Result<Self::Opened, Self::Error>> + Send;
    fn readable(&self, repository: &OwnedRepository) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn not_found(&self) -> Self::Error;
}

/// No additional existence query: redirect triggers retire reclaimed aliases.
#[derive(Debug)]
pub struct RepositoryAliases;

impl RepositoryAliases {
    /// The repository at `owner/name`, or formerly there. Git-style `.git` is accepted.
    /// No permission decision is made: callers decide access on the returned repository.
    pub async fn resolve<A: RepositoryAddresses>(addresses: &A, owner: &str, name: &str) -> Result<Option<OwnedRepository>, A::Error> {
        let name = name.strip_suffix(".git").unwrap_or(name);
        match addresses.find_current(owner, name).await? {
            Some(repository) => Ok(Some(repository)),
            None => addresses.find_old(owner, name).await,
        }
    }

    /// Git accepts a trailing `.git`. A current private address must never fall back
    /// to an unrelated old address when opening it fails permission checks.
    pub async fn open<P: AliasPort>(port: &P, owner: &str, name: &str) -> Result<P::Opened, P::Error> {
        let repository = Self::resolve(port.addresses(), owner, name).await?.ok_or_else(|| port.not_found())?;
        port.open(repository).await
    }

    /// Browser redirect lookup intentionally neither strips `.git` nor queries the
    /// current address. A missing alias performs no grants or reader work.
    pub async fn moved_to<P: AliasPort>(port: &P, owner: &str, name: &str) -> Result<Option<OwnedRepository>, P::Error> {
        let Some(repository) = port.addresses().find_old(owner, name).await? else {
            return Ok(None);
        };
        Ok(port.readable(&repository).await?.then_some(repository))
    }
}
