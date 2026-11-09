//! Two static adapters share one callback policy; the default still uses its original SQL.
use super::{ExternalIdentity, SessionToken, User};
use crate::{ServiceError, TwoFactorService};
use rbit_core::AccountId;
use rbit_identity::accounts::{CreatedAccountSnapshot, ProviderCreationRepository};
use rbit_identity::accounts::{ProvenAccount, ProviderAuthentication, ProviderOrigin};
use rbit_storage::{
    Database, NewUser, StorageError,
    sqlite::external_identities::{ExternalIdentityStore, GuardedIdentityLink, LinkConflict, NewExternalIdentity},
};
use std::future::Future;

/// Dormant result: the signed-in branch retains its original provider proof through
/// optional local MFA. This does not itself authorize unfenced session issuance.
#[derive(Debug)]
pub enum OriginalExternalSignIn {
    SignedIn(ProvenAccount),
    SecondFactorRequired(SessionToken),
    Linked,
    Register(SessionToken),
}

/// Private, concrete two-mode seam, not a runtime configuration or plugin interface.
pub(super) trait Mode {
    type Origin: Send + Sync;
    type Account: Send;
    fn find(
        store: &ExternalIdentityStore,
        provider: &str,
        subject: &str,
    ) -> impl Future<Output = Result<Option<(ExternalIdentity, Self::Origin)>, ServiceError>> + Send;
    fn email(
        store: &ExternalIdentityStore,
        email: &str,
    ) -> impl Future<Output = Result<Option<(AccountId, Self::Origin)>, ServiceError>> + Send;
    fn link_accepted(
        store: &ExternalIdentityStore,
        account: AccountId,
        identity: NewExternalIdentity<'_>,
        at: rbit_core::Timestamp,
        origin: &Self::Origin,
    ) -> impl Future<Output = Result<Result<ExternalIdentity, LinkConflict>, ServiceError>> + Send;
    fn finish(
        database: &Database,
        account: AccountId,
        origin: Self::Origin,
    ) -> impl Future<Output = Result<Self::Account, ServiceError>> + Send;
    fn user(account: &Self::Account) -> &User;
    fn pending(two_factor: &TwoFactorService, account: Self::Account, method: &str) -> SessionToken;
    fn register(
        store: &ExternalIdentityStore,
        new: NewUser,
        verified: bool,
        identity: NewExternalIdentity<'_>,
        open: bool,
    ) -> impl Future<Output = Result<Result<Self::Account, LinkConflict>, StorageError>> + Send;
}
pub(super) struct Legacy;
impl Mode for Legacy {
    type Origin = ();
    type Account = User;
    async fn find(store: &ExternalIdentityStore, provider: &str, subject: &str) -> Result<Option<(ExternalIdentity, ())>, ServiceError> {
        Ok(store.find(provider, subject).await?.map(|identity| (identity, ())))
    }
    async fn email(store: &ExternalIdentityStore, email: &str) -> Result<Option<(AccountId, ())>, ServiceError> {
        Ok(store.account_with_verified_email(email).await?.map(|account| (account, ())))
    }
    async fn link_accepted(
        store: &ExternalIdentityStore,
        account: AccountId,
        identity: NewExternalIdentity<'_>,
        at: rbit_core::Timestamp,
        (): &(),
    ) -> Result<Result<ExternalIdentity, LinkConflict>, ServiceError> {
        Ok(store.link(account, identity, at).await?)
    }
    async fn finish(database: &Database, account: AccountId, (): ()) -> Result<User, ServiceError> {
        database.accounts().find_active_user(account).await?.ok_or(ServiceError::NotFound)
    }
    fn user(account: &User) -> &User {
        account
    }
    fn pending(two_factor: &TwoFactorService, account: User, method: &str) -> SessionToken {
        two_factor.start_pending(account.id(), method)
    }
    async fn register(
        store: &ExternalIdentityStore,
        new: NewUser,
        verified: bool,
        identity: NewExternalIdentity<'_>,
        open: bool,
    ) -> Result<Result<User, LinkConflict>, StorageError> {
        store.register(new, verified, identity, open).await
    }
}
pub(super) struct Original;
impl Mode for Original {
    type Origin = ProviderOrigin;
    type Account = ProvenAccount;
    async fn find(
        store: &ExternalIdentityStore,
        provider: &str,
        subject: &str,
    ) -> Result<Option<(ExternalIdentity, ProviderOrigin)>, ServiceError> {
        Ok(ProviderAuthentication::capture_linked(store, provider, subject).await?)
    }
    async fn email(store: &ExternalIdentityStore, email: &str) -> Result<Option<(AccountId, ProviderOrigin)>, ServiceError> {
        Ok(ProviderAuthentication::capture_verified_email(store, email).await?.map(|origin| (origin.account(), origin)))
    }
    async fn link_accepted(
        store: &ExternalIdentityStore,
        account: AccountId,
        identity: NewExternalIdentity<'_>,
        at: rbit_core::Timestamp,
        origin: &ProviderOrigin,
    ) -> Result<Result<ExternalIdentity, LinkConflict>, ServiceError> {
        if origin.account() != account || origin.was_pending() {
            return Err(ServiceError::Unavailable { retry_after_seconds: 1 });
        }
        match store.link_at_epoch(account, identity, at, origin.original_epoch()).await? {
            GuardedIdentityLink::Linked(linked) => Ok(Ok(linked)),
            GuardedIdentityLink::Conflict(conflict) => Ok(Err(conflict)),
            GuardedIdentityLink::Unavailable => Err(ServiceError::Unavailable { retry_after_seconds: 1 }),
        }
    }
    fn finish(
        _database: &Database,
        account: AccountId,
        origin: ProviderOrigin,
    ) -> impl Future<Output = Result<ProvenAccount, ServiceError>> + Send {
        std::future::ready(if origin.account() == account {
            ProviderAuthentication::first_factor(origin).ok_or(ServiceError::Unavailable { retry_after_seconds: 1 })
        } else {
            Err(ServiceError::Forbidden)
        })
    }
    fn user(account: &ProvenAccount) -> &User {
        account.user()
    }
    fn pending(two_factor: &TwoFactorService, account: ProvenAccount, method: &str) -> SessionToken {
        two_factor.start_original_provider_pending(account, method)
    }
    async fn register(
        store: &ExternalIdentityStore,
        new: NewUser,
        verified: bool,
        identity: NewExternalIdentity<'_>,
        open: bool,
    ) -> Result<Result<ProvenAccount, LinkConflict>, StorageError> {
        match ProviderAuthentication::register(&ProviderCreation { store, identity, verified }, new, open).await {
            Ok(proven) => Ok(Ok(proven)),
            Err(CreationError::Conflict(conflict)) => Ok(Err(conflict)),
            Err(CreationError::Store(error)) => Err(error),
        }
    }
}

struct ProviderCreation<'a> {
    store: &'a ExternalIdentityStore,
    identity: NewExternalIdentity<'a>,
    verified: bool,
}
enum CreationError {
    Store(StorageError),
    Conflict(LinkConflict),
}
impl ProviderCreationRepository for ProviderCreation<'_> {
    type Error = CreationError;
    async fn create_original(&self, new: NewUser, open: bool) -> Result<CreatedAccountSnapshot, CreationError> {
        self.store
            .register_original(new, self.verified, self.identity, open)
            .await
            .map_err(CreationError::Store)?
            .map_err(CreationError::Conflict)
    }
}
