//! Wire-compatible credential forms; Debug never prints submitted data.

use serde::Deserialize;

#[derive(Deserialize)]
pub struct SetPasswordForm {
    #[serde(default)]
    pub(super) new_password: String,
    #[serde(default)]
    pub(super) confirm_password: String,
}

impl std::fmt::Debug for SetPasswordForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetPasswordForm").finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
pub struct NewUserForm {
    #[serde(default)]
    pub(super) username: String,
    #[serde(default)]
    pub(super) email: String,
    #[serde(default)]
    pub(super) password: String,
    pub(super) is_admin: Option<String>,
}

impl std::fmt::Debug for NewUserForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewUserForm").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn submitted_credentials_are_never_in_debug_output() {
        let secret = "submitted-secret-sentinel";
        assert!(!format!("{:?}", SetPasswordForm { new_password: secret.into(), confirm_password: secret.into() }).contains(secret));
        assert!(
            !format!(
                "{:?}",
                NewUserForm { username: secret.into(), email: secret.into(), password: secret.into(), is_admin: Some(secret.into()) }
            )
            .contains(secret)
        );
    }
}
