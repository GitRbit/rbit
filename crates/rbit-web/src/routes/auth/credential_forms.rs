//! Wire-compatible credential forms; Debug never prints submitted data.

use serde::Deserialize;

#[derive(Deserialize)]
pub struct LoginForm {
    pub(super) login: String,
    pub(super) password: String,
    pub(super) next: Option<String>,
}

impl std::fmt::Debug for LoginForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginForm").finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
pub struct SignupForm {
    pub(super) username: String,
    pub(super) email: String,
    pub(super) password: String,
    /// Only asked for (and only checked) while the instance has no user.
    #[serde(default)]
    pub(super) setup_token: String,
}

impl std::fmt::Debug for SignupForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignupForm").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn submitted_credentials_are_never_in_debug_output() {
        let secret = "submitted-secret-sentinel";
        assert!(!format!("{:?}", LoginForm { login: secret.into(), password: secret.into(), next: Some(secret.into()) }).contains(secret));
        assert!(
            !format!(
                "{:?}",
                SignupForm { username: secret.into(), email: secret.into(), password: secret.into(), setup_token: secret.into() }
            )
            .contains(secret)
        );
    }
}
