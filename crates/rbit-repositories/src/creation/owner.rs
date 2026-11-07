//! Self-owner fast path and organization creation grant shared with forks/transfers.
use super::RepositoryOwnerCatalog;
use rbit_identity_models::{Account, User};
#[derive(Debug)]
pub enum RepositoryOwnerError<E> {
    Port(E),
    Invalid(String),
}
#[derive(Debug)]
pub struct RepositoryOwnership;
impl RepositoryOwnership {
    /// The caller retains operation-specific trimming; do not normalize shared owner inputs here.
    pub async fn resolve<S: RepositoryOwnerCatalog>(
        store: &S,
        actor: &User,
        owner: &str,
    ) -> Result<Account, RepositoryOwnerError<S::Error>> {
        if owner.is_empty() || owner.eq_ignore_ascii_case(actor.name().as_str()) {
            return Ok(actor.account.clone());
        }
        let not_allowed = || RepositoryOwnerError::Invalid(format!("you cannot create repositories for {owner}"));
        let account = store.owner_named(owner).await.map_err(RepositoryOwnerError::Port)?.ok_or_else(not_allowed)?;
        if !account.is_organization() {
            return Err(not_allowed());
        }
        if actor.is_admin || store.membership(account.id, actor.id()).await.map_err(RepositoryOwnerError::Port)?.is_some() {
            Ok(account)
        } else {
            Err(not_allowed())
        }
    }
}
