//! Who an SSH key authenticates, and which repository a deploy key reaches.
//!
//! Keys are Git access credentials: a user's personal key carries the user's own
//! repository permissions, a deploy key one repository's. Sign-in resolves a verified
//! key's `SHA256:…` fingerprint through the [`SshKeyDirectory`] port and decides here:
//!
//! * a suspended user's key authenticates no one (the password-change check is not
//!   even made);
//! * a user who must change their password signs in with no key until they have;
//! * a deploy key always authenticates its repository.
//!
//! A deploy key reaches only its own repository, also under a former `owner/name`
//! ([`RepositoryAliases::resolve`]), read-only unless it was added with write
//! permission; any other repository is indistinguishable from a missing one.
//!
//! Persistence owns fingerprint uniqueness across user and deploy keys; nothing here
//! stores or caches an identity. Each sign-in costs one key lookup, plus one account
//! read for an active user's key.

use std::future::Future;

use rbit_contracts::AccountId;
use rbit_identity_models::User;

use crate::{
    OwnedRepository,
    aliases::{RepositoryAddresses, RepositoryAliases},
};

/// A user's SSH key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SshKeyId(pub i64);

/// A repository's deploy key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeployKeyId(pub i64);

/// A key's holder exactly as stored, before any sign-in decision.
#[derive(Debug, Clone)]
pub enum StoredKeyHolder {
    /// A user's personal key; `suspended` is the account's current state.
    User { key: SshKeyId, user: User, suspended: bool },
    /// A deploy key and its repository.
    Deploy { key: DeployKeyId, repo: OwnedRepository, read_only: bool },
}

/// The key lookups behind SSH sign-in.
pub trait SshKeyDirectory: Sync {
    type Error: Send;

    /// The holder of the key with this fingerprint, if any.
    fn holder(&self, fingerprint: &str) -> impl Future<Output = Result<Option<StoredKeyHolder>, Self::Error>> + Send;

    /// Whether the account must change its password before signing in again.
    fn password_change_required(&self, user: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}

/// Who an SSH key authenticates.
#[derive(Debug, Clone)]
pub enum SshIdentity {
    /// A user's personal key: the user's own permissions apply.
    User { key: SshKeyId, user: User },
    /// A deploy key: one repository, read-only unless `read_only` is false.
    Deploy { key: DeployKeyId, repo: OwnedRepository, read_only: bool },
}

/// What a fingerprint resolved to at sign-in.
#[derive(Debug, Clone)]
pub enum KeySignIn {
    /// The key authenticates this identity.
    Identified(SshIdentity),
    /// No key has this fingerprint.
    Unknown,
    /// A suspended user's key; callers log the refusal with the user's name.
    Suspended(User),
    /// The user must change their password first.
    PasswordChangeRequired,
}

impl KeySignIn {
    /// The authenticated identity, or `None` for every refusal.
    pub fn into_identity(self) -> Option<SshIdentity> {
        match self {
            Self::Identified(identity) => Some(identity),
            Self::Unknown | Self::Suspended(_) | Self::PasswordChangeRequired => None,
        }
    }
}

/// Resolves a verified key's fingerprint and decides whether it signs anyone in.
pub async fn sign_in<D: SshKeyDirectory>(directory: &D, fingerprint: &str) -> Result<KeySignIn, D::Error> {
    Ok(match directory.holder(fingerprint).await? {
        None => KeySignIn::Unknown,
        Some(StoredKeyHolder::User { user, suspended: true, .. }) => KeySignIn::Suspended(user),
        Some(StoredKeyHolder::User { key, user, suspended: false }) => {
            if directory.password_change_required(user.id()).await? {
                KeySignIn::PasswordChangeRequired
            } else {
                KeySignIn::Identified(SshIdentity::User { key, user })
            }
        }
        Some(StoredKeyHolder::Deploy { key, repo, read_only }) => KeySignIn::Identified(SshIdentity::Deploy { key, repo, read_only }),
    })
}

/// A deploy key's own repository, as addressed by the client.
#[derive(Debug, Clone)]
pub struct DeployKeyReach {
    pub repo: OwnedRepository,
    /// Whether the key was added with write permission.
    pub writable: bool,
}

/// The repository at `owner/name` (or formerly there; `.git` accepted) when it is the
/// deploy key's own. `None` for user keys, missing repositories and every other
/// repository, so a deploy key learns nothing about them.
pub async fn deploy_key_reach<A: RepositoryAddresses>(
    addresses: &A,
    identity: &SshIdentity,
    owner: &str,
    name: &str,
) -> Result<Option<DeployKeyReach>, A::Error> {
    let SshIdentity::Deploy { repo: own, read_only, .. } = identity else {
        return Ok(None);
    };
    let Some(repo) = RepositoryAliases::resolve(addresses, owner, name).await? else {
        return Ok(None);
    };
    Ok((repo.repo.id == own.repo.id).then_some(DeployKeyReach { repo, writable: !read_only }))
}
