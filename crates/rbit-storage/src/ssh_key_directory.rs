//! SSH sign-in lookups over the existing key and account SQL.
//!
//! The holder lookup is the key store's single fingerprint query (user keys, then deploy
//! keys); the password-change flag is one account read. Decisions belong to
//! `rbit_repositories::ssh_keys`.
use crate::{
    StorageError,
    sqlite::{
        Database,
        ssh_keys::{KeyHolder, SshKeyStore},
    },
};
use rbit_core::AccountId;
use rbit_repositories::ssh_keys::{SshKeyDirectory, StoredKeyHolder};

/// Borrowed sign-in persistence; constructing it opens no account store.
pub struct SshKeyDirectoryStore<'a> {
    database: &'a Database,
    keys: &'a SshKeyStore,
}

impl<'a> SshKeyDirectoryStore<'a> {
    /// `keys` is the caller's open key store, reused for the holder lookup.
    pub fn new(database: &'a Database, keys: &'a SshKeyStore) -> Self {
        Self { database, keys }
    }
}

impl std::fmt::Debug for SshKeyDirectoryStore<'_> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("SshKeyDirectoryStore").finish_non_exhaustive()
    }
}

impl SshKeyDirectory for SshKeyDirectoryStore<'_> {
    type Error = StorageError;

    async fn holder(&self, fingerprint: &str) -> Result<Option<StoredKeyHolder>, StorageError> {
        Ok(self.keys.find_by_fingerprint(fingerprint).await?.map(|holder| match holder {
            KeyHolder::User { key, user, suspended } => StoredKeyHolder::User { key: key.id, user, suspended },
            KeyHolder::Deploy { key, repo } => StoredKeyHolder::Deploy { key: key.id, repo, read_only: key.read_only },
        }))
    }

    async fn password_change_required(&self, user: AccountId) -> Result<bool, StorageError> {
        self.database.accounts().password_change_required(user).await
    }
}
