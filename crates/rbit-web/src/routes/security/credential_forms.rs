//! Wire-compatible credential forms; Debug never prints submitted data.

use serde::Deserialize;

#[derive(Deserialize)]
pub struct CodeForm {
    #[serde(default)]
    pub(super) code: String,
}

impl std::fmt::Debug for CodeForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodeForm").finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
pub struct PasswordAndCodeForm {
    #[serde(default)]
    pub(super) password: String,
    #[serde(default)]
    pub(super) code: String,
    /// The browser's answer to the page's passkey challenge, instead of a code.
    #[serde(default)]
    pub(super) passkey: String,
}

impl std::fmt::Debug for PasswordAndCodeForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordAndCodeForm").finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
pub struct AddPasskeyForm {
    #[serde(default)]
    pub(super) name: String,
    /// The browser's answer to the creation challenge (filled in by `passkey.js`).
    #[serde(default)]
    pub(super) credential: String,
}

impl std::fmt::Debug for AddPasskeyForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AddPasskeyForm").finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
pub struct TwoFactorForm {
    #[serde(default)]
    pub(super) code: String,
    /// The browser's answer to the page's passkey challenge, instead of a code.
    #[serde(default)]
    pub(super) passkey: String,
    pub(super) next: Option<String>,
}

impl std::fmt::Debug for TwoFactorForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TwoFactorForm").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn submitted_credentials_are_never_in_debug_output() {
        let secret = "submitted-secret-sentinel";
        assert!(!format!("{:?}", CodeForm { code: secret.into() }).contains(secret));
        assert!(
            !format!("{:?}", PasswordAndCodeForm { password: secret.into(), code: secret.into(), passkey: secret.into() }).contains(secret)
        );
        assert!(!format!("{:?}", AddPasskeyForm { name: secret.into(), credential: secret.into() }).contains(secret));
        assert!(
            !format!("{:?}", TwoFactorForm { code: secret.into(), passkey: secret.into(), next: Some(secret.into()) }).contains(secret)
        );
    }
}
