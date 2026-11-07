//! Wire-compatible credential forms; Debug never prints submitted data.

use serde::Deserialize;

#[derive(Deserialize)]
pub struct ResetForm {
    #[serde(default)]
    pub(super) new_password: String,
    #[serde(default)]
    pub(super) confirm_password: String,
}

impl std::fmt::Debug for ResetForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResetForm").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn submitted_credentials_are_never_in_debug_output() {
        let secret = "submitted-secret-sentinel";
        assert!(!format!("{:?}", ResetForm { new_password: secret.into(), confirm_password: secret.into() }).contains(secret));
    }
}
