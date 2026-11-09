//! When the secrets key file is read, refused or created, and what is said when it cannot
//! be used. The file system (and the platform's permission check) is the [`KeyFile`] port.
use std::{fmt, future::Future, io};

use zeroize::Zeroizing;

use super::SecretsKey;

/// Why secrets cannot be used now, in words for settings pages and task logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyUnavailable(pub String);

impl fmt::Display for KeyUnavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The key file at its configured place.
pub trait KeyFile: Sync {
    /// The file as messages name it.
    fn shown(&self) -> String;
    /// Its contents, or `None` when there is no file.
    fn read(&self) -> impl Future<Output = io::Result<Option<Zeroizing<String>>>> + Send;
    /// Refuses a file others may read, where the platform can tell.
    fn check_private(&self) -> Result<(), KeyUnavailable>;
    /// Writes a new file only readable by its owner, failing if one appeared meanwhile.
    fn create(&self, contents: &str) -> impl Future<Output = io::Result<()>> + Send;
}

/// The key and whether it was created now (the adapter says so in its log).
#[derive(Debug)]
pub enum LoadedKey {
    Existing(SecretsKey),
    Created(SecretsKey),
}

impl LoadedKey {
    pub fn into_key(self) -> SecretsKey {
        match self {
            Self::Existing(key) | Self::Created(key) => key,
        }
    }
}

/// Loads the key from `file`, or creates it when there is none and `sealed_exist` says no
/// secret was sealed yet: a missing key while secrets are stored stays missing, so every
/// secret fails closed with the reason instead of being replaced by a new key's garbage.
pub async fn load_key<F: KeyFile, E: fmt::Display>(
    file: &F,
    sealed_exist: impl Future<Output = Result<bool, E>>,
) -> Result<LoadedKey, KeyUnavailable> {
    let shown = file.shown();
    match file.read().await {
        Ok(Some(text)) => {
            file.check_private()?;
            SecretsKey::parse(&text)
                .map(LoadedKey::Existing)
                .map_err(|reason| KeyUnavailable(format!("the secrets key file {shown} is unusable: {reason}")))
        }
        Ok(None) => {
            if sealed_exist.await.map_err(|error| KeyUnavailable(format!("checking for stored secrets failed: {error}")))? {
                return Err(KeyUnavailable(format!(
                    "the secrets key file {shown} is missing while secrets are stored; restore it, or delete the secrets and set them again"
                )));
            }
            let key = SecretsKey::generate().map_err(|error| KeyUnavailable(format!("generating a secrets key failed: {error}")))?;
            file.create(&key.encode())
                .await
                .map_err(|error| KeyUnavailable(format!("creating the secrets key file {shown} failed: {error}")))?;
            Ok(LoadedKey::Created(key))
        }
        Err(error) => Err(KeyUnavailable(format!("reading the secrets key file {shown} failed: {error}"))),
    }
}
