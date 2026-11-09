//! Per-command authorization for Git over SSH, independent of russh.
//!
//! A connection may run several commands long after it authenticated, so every command
//! is authorized on its own, in a fixed order: the key must still belong to the same
//! holder ([`same_key_holder`]), a user must not be suspended, the repository must open
//! for the key (user keys exactly as over HTTP, old names included; deploy keys only
//! their own repository), a push needs write access, and a push to a pull mirror is
//! refused. The [`SshRepositories`] port does the lookups; this module decides the order
//! and the text the client sees. The repository on disk always comes from the opened
//! repository, never from the command line.
//!
//! Which identity a key signs in and which repository a deploy key reaches are decided
//! by `rbit_repositories::ssh_keys` ([`key_holder`], the deploy-key adapter below) over
//! the key service's storage adapters.

use std::future::Future;

use rbit_core::{AccountId, PushSource, User};
use rbit_repositories::ssh_keys;
use rbit_services::{RepoContext, ServiceError, Services, ssh_keys::SshIdentity};

use crate::command::GitCommand;

/// Shown when a connection's key was deleted or reassigned since it authenticated.
pub(crate) const KEY_NO_LONGER_AUTHORIZED: &str = "this SSH key is no longer authorized; reconnect with a valid key";

/// Shown for any unexpected failure; the cause is logged.
const INTERNAL: &str = "internal error; please try again later";

/// The account and repository lookups behind SSH authorization.
pub(crate) trait SshRepositories {
    /// An opened repository, at the access the key grants.
    type Repository: Send;

    /// Whether the account is suspended now (it may have been after the connection
    /// authenticated).
    fn is_suspended(&self, user: AccountId) -> impl Future<Output = Result<bool, ServiceError>> + Send;

    /// Opens `owner/name` with a user's own access, following renames and transfers.
    fn open_for_user(&self, user: &User, owner: &str, name: &str) -> impl Future<Output = Result<Self::Repository, ServiceError>> + Send;

    /// Opens `owner/name` for a deploy key: only its own repository.
    fn open_for_deploy_key(
        &self,
        identity: &SshIdentity,
        owner: &str,
        name: &str,
    ) -> impl Future<Output = Result<Self::Repository, ServiceError>> + Send;

    /// Whether the access to `repository` allows pushing.
    fn can_write(&self, repository: &Self::Repository) -> bool;

    /// Whether `repository` is a pull mirror.
    fn is_mirror(&self, repository: &Self::Repository) -> impl Future<Output = Result<bool, ServiceError>> + Send;
}

impl SshRepositories for Services {
    type Repository = RepoContext;

    async fn is_suspended(&self, user: AccountId) -> Result<bool, ServiceError> {
        self.accounts.is_suspended(user).await
    }

    async fn open_for_user(&self, user: &User, owner: &str, name: &str) -> Result<RepoContext, ServiceError> {
        self.repositories.open_following_renames(Some(user), owner, name).await
    }

    async fn open_for_deploy_key(&self, identity: &SshIdentity, owner: &str, name: &str) -> Result<RepoContext, ServiceError> {
        let keys = &self.ssh_keys;
        let reach = ssh_keys::deploy_key_reach(&keys.addresses(), identity, owner, name).await.map_err(ServiceError::from)?;
        Ok(keys.deploy_key_context(reach.ok_or(ServiceError::NotFound)?))
    }

    fn can_write(&self, ctx: &RepoContext) -> bool {
        ctx.access.can_write()
    }

    async fn is_mirror(&self, ctx: &RepoContext) -> Result<bool, ServiceError> {
        self.mirrors.is_mirror(ctx.repo.repo.id).await
    }
}

/// Who holds the key with `fingerprint` now ([`ssh_keys::sign_in`]): `None` for unknown
/// keys, suspended users and users who must change their password.
pub(crate) async fn key_holder(services: &Services, fingerprint: &str) -> Result<Option<SshIdentity>, ServiceError> {
    let signed_in = ssh_keys::sign_in(&services.ssh_keys.directory(), fingerprint).await.map_err(ServiceError::from)?;
    rbit_services::ssh_keys::log_refusal(&signed_in);
    Ok(signed_in.into_identity())
}

/// Where a push with this key comes from: the user, or the repository's deploy key.
pub(crate) fn push_source(identity: &SshIdentity) -> PushSource {
    match identity {
        SshIdentity::User { user, .. } => PushSource::User(user.id()),
        SshIdentity::Deploy { .. } => PushSource::DeployKey,
    }
}

/// Opens the repository named by `command` with exactly the access HTTP would grant.
/// The error is the message shown to the client. With `check_write_access` off, a push
/// without write access is let through to the push gate, which refuses it after reading
/// the command list (some Git clients wait forever when an SSH server closes before
/// advertising refs).
pub(crate) async fn authorize<P>(
    port: &P,
    identity: &SshIdentity,
    command: &GitCommand,
    check_write_access: bool,
) -> Result<P::Repository, String>
where
    P: SshRepositories + Sync,
{
    let full_name = command.full_name();
    let opened = match identity {
        SshIdentity::User { user, .. } => {
            // The connection may have been authenticated before a suspension.
            match port.is_suspended(user.id()).await {
                Ok(false) => {}
                Ok(true) => return Err("this account has been suspended".to_owned()),
                Err(error) => return Err(internal(&error)),
            }
            port.open_for_user(user, &command.owner, &command.repo).await
        }
        SshIdentity::Deploy { .. } => port.open_for_deploy_key(identity, &command.owner, &command.repo).await,
    };
    let repository = match opened {
        Ok(repository) => repository,
        Err(ServiceError::NotFound) => return Err(format!("repository {full_name} not found, or you do not have access to it")),
        Err(error) => return Err(internal(&error)),
    };
    if check_write_access && command.service.is_write() && !port.can_write(&repository) {
        return Err(match identity {
            SshIdentity::Deploy { .. } => read_only_deploy_key(&full_name),
            SshIdentity::User { .. } => format!("you do not have permission to push to {full_name}"),
        });
    }
    if command.service.is_write() {
        match port.is_mirror(&repository).await {
            Ok(false) => {}
            Ok(true) => return Err(rbit_services::mirrors::PUSH_REFUSED.to_owned()),
            Err(error) => return Err(internal(&error)),
        }
    }
    Ok(repository)
}

/// Why a read-only deploy key cannot push to `repository` (`owner/name`).
pub(crate) fn read_only_deploy_key(repository: &str) -> String {
    format!("this deploy key is read-only; it cannot push to {repository}")
}

/// Logs an unexpected error and returns the generic message shown to the client.
pub(crate) fn internal(error: &ServiceError) -> String {
    tracing::error!(%error, "SSH request failed");
    INTERNAL.to_owned()
}

/// A deleted key re-registered for another holder cannot turn an old authenticated
/// connection into that holder's session.
pub(crate) fn same_key_holder(previous: &SshIdentity, current: &SshIdentity) -> bool {
    match (previous, current) {
        (SshIdentity::User { key: old_key, user: old }, SshIdentity::User { key, user }) => old_key == key && old.id() == user.id(),
        (SshIdentity::Deploy { key: old_key, repo: old, .. }, SshIdentity::Deploy { key, repo, .. }) => {
            old_key == key && old.repo.id == repo.repo.id
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests;
