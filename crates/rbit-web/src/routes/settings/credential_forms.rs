//! Wire-compatible credential forms; Debug never prints submitted data.

use serde::Deserialize;

#[derive(Deserialize)]
pub struct EmailFormInput {
    #[serde(default)]
    pub(super) email: String,
    #[serde(default)]
    pub(super) current_password: String,
}

impl std::fmt::Debug for EmailFormInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmailFormInput").finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
pub struct UsernameFormInput {
    #[serde(default)]
    pub(super) username: String,
    #[serde(default)]
    pub(super) current_password: String,
}

impl std::fmt::Debug for UsernameFormInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsernameFormInput").finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
pub struct PasswordFormInput {
    #[serde(default, rename = "current_password")]
    pub(super) current: String,
    #[serde(default, rename = "new_password")]
    pub(super) new: String,
    #[serde(default, rename = "confirm_password")]
    pub(super) confirmation: String,
}

impl std::fmt::Debug for PasswordFormInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordFormInput").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn submitted_credentials_are_never_in_debug_output() {
        let secret = "submitted-secret-sentinel";
        assert!(!format!("{:?}", EmailFormInput { email: secret.into(), current_password: secret.into() }).contains(secret));
        assert!(!format!("{:?}", UsernameFormInput { username: secret.into(), current_password: secret.into() }).contains(secret));
        assert!(
            !format!("{:?}", PasswordFormInput { current: secret.into(), new: secret.into(), confirmation: secret.into() })
                .contains(secret)
        );
    }
}

#[cfg(test)]
mod wire_tests {
    use super::*;
    #[test]
    fn field_names_defaults_and_confirmation_payloads_remain_wire_compatible() {
        let empty: PasswordFormInput = serde_json::from_str("{}").unwrap();
        assert!(empty.current.is_empty() && empty.new.is_empty() && empty.confirmation.is_empty());
        let input: PasswordFormInput =
            serde_json::from_str(r#"{"current_password":"old","new_password":"new","confirm_password":"confirm"}"#).unwrap();
        assert_eq!((input.current.as_str(), input.new.as_str(), input.confirmation.as_str()), ("old", "new", "confirm"));
        let email: EmailFormInput = serde_json::from_str("{}").unwrap();
        let username: UsernameFormInput = serde_json::from_str("{}").unwrap();
        assert!(email.email.is_empty() && email.current_password.is_empty());
        assert!(username.username.is_empty() && username.current_password.is_empty());
    }
}
