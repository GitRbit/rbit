//! Actions secrets and variables (§6.3 and §11.10 of the Actions design): the key and its
//! file, sealing and opening with AES-256-GCM bound to the secret's row, what values may
//! hold, and what a task or an environment is given. The file system, storage and audit
//! are the composition's adapters.
//!
//! * A key file holds [`KEY_PREFIX`] and the base64 of 32 random bytes. It is created only
//!   while no secret is sealed: a missing key while secrets exist stays missing, and every
//!   secret then fails closed with the reason ([`load_key`]).
//! * Each sealed value stores the first 8 bytes of SHA-256 of the key that sealed it, so a
//!   replaced key is detected ([`UNREADABLE`]) instead of decrypting garbage. The associated
//!   data binds the scope kind, the scope id and the secret id ([`SecretAddress`]), so a
//!   ciphertext copied to another row or scope does not open. Nonces are fresh for every
//!   seal; values never reach `Debug` output.
//! * A value is 1 to its limit's bytes without NUL ([`check_value`]); a repository or
//!   environment secret is visible to its repository only ([`check_visibility`]).
//! * A task opens the secrets it is given in order, the first of each name winning
//!   (repository over owner, as the store orders them), and fails closed, naming the
//!   secret, when any cannot be opened ([`open_task_secrets`]). An environment's secrets
//!   fail the same way ([`open_environment_secrets`]).
//! * Whether a run is given any is `crate::trust::Grants`.

mod key;
mod key_file;
#[cfg(test)]
mod key_tests;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

pub use key::{KEY_PREFIX, SecretsKey};
pub use key_file::{KeyFile, KeyUnavailable, LoadedKey, load_key};

/// The row a sealed value belongs to: the stored scope kind (`owner`, `repository`,
/// `environment`), the scope's id and the secret's id. Part of the associated data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretAddress<'a> {
    pub scope_kind: &'a str,
    pub scope_id: i64,
    pub secret_id: i64,
}

/// Largest secret value, in bytes (`rbit_workflow::spec::MAX_SECRET_BYTES`).
pub const MAX_VALUE_BYTES: usize = rbit_workflow::spec::MAX_SECRET_BYTES;

/// The message pages and failed tasks show for a value sealed with another key.
pub const UNREADABLE: &str = "cannot be decrypted with the current key — set it again";
/// Largest variable value, in bytes: what `actions_variables.value` holds.
pub const MAX_VARIABLE_BYTES: usize = 48 * 1024;
/// Most secrets, and separately most variables, in one scope. A repository and its owner
/// together stay below the 1,024 a task may receive.
pub const MAX_PER_SCOPE: u64 = 500;

/// Values are 1 to `max` bytes without NUL; `kind` is `secret` or `variable`.
pub fn check_value(kind: &str, value: &str, max: usize) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("a {kind} needs a value"));
    }
    if value.len() > max {
        return Err(format!("a {kind} is at most {} KiB", max / 1024));
    }
    if value.contains('\0') {
        return Err(format!("a {kind} must not contain NUL characters"));
    }
    Ok(())
}

/// A secret of a repository or an environment (`owner_scope` false) is visible to its
/// repository only: anything but visibility to all is refused.
pub fn check_visibility(owner_scope: bool, visible_to_all: bool) -> Result<(), String> {
    if owner_scope || visible_to_all { Ok(()) } else { Err("a repository secret is visible to its repository only".to_owned()) }
}

/// A task's secrets by name from `given` in order, opened by `open`: the first of each name
/// wins and a shadowed one is never opened. `Err` names the first one `open` refuses.
pub fn open_task_secrets<'a, S: 'a>(
    given: impl IntoIterator<Item = (&'a str, &'a S)>,
    mut open: impl FnMut(&'a S) -> Option<String>,
) -> Result<BTreeMap<String, String>, String> {
    let mut secrets = BTreeMap::new();
    for (name, secret) in given {
        if secrets.contains_key(name) {
            continue;
        }
        let value = open(secret).ok_or_else(|| name.to_owned())?;
        secrets.insert(name.to_owned(), value);
    }
    Ok(secrets)
}

/// An environment's secrets in order, opened by `open`. `Err` names the first one `open`
/// refuses; the ones after it are not opened.
pub fn open_environment_secrets<'a, S: 'a>(
    given: impl IntoIterator<Item = (&'a str, &'a S)>,
    mut open: impl FnMut(&'a S) -> Option<String>,
) -> Result<Vec<(String, String)>, String> {
    given.into_iter().map(|(name, secret)| open(secret).map(|value| (name.to_owned(), value)).ok_or_else(|| name.to_owned())).collect()
}
