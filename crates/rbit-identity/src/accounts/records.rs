//! Plain port records preserve legacy fields; Debug never formats credential values.

use std::fmt;

use crate::{AccountName, Email, User};

/// Raw sign-up input. Application entry takes the password into protected ownership.
#[derive(Clone, Default)]
pub struct SignUp {
    pub username: String,
    pub email: String,
    pub password: String,
}

impl fmt::Debug for SignUp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignUp").field("username", &self.username).field("email", &self.email).field("password", &"<redacted>").finish()
    }
}

/// Validated input for one authoritative account/login transaction. The hash is PHC.
pub struct NewUser {
    pub name: AccountName,
    pub email: Email,
    pub display_name: Option<String>,
    pub password_hash: String,
    pub is_admin: bool,
}

impl fmt::Debug for NewUser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NewUser")
            .field("name", &self.name)
            .field("email", &self.email)
            .field("display_name", &self.display_name)
            .field("password_hash", &"<redacted>")
            .field("is_admin", &self.is_admin)
            .finish()
    }
}

/// A resolved login candidate. Suspension is revealed only after valid verification.
pub struct LoginRecord {
    pub user: User,
    pub password_hash: String,
    pub suspended: bool,
}

impl fmt::Debug for LoginRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoginRecord")
            .field("user", &self.user)
            .field("password_hash", &"<redacted>")
            .field("suspended", &self.suspended)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Account, AccountId, AccountKind};
    use rbit_contracts::Timestamp;

    #[test]
    fn credential_fields_are_redacted_in_every_port_input_record() {
        let secret = "credential-should-never-appear";
        assert!(!format!("{:?}", SignUp { password: secret.into(), ..SignUp::default() }).contains(secret));
        let new = NewUser {
            name: "alice".parse().unwrap(),
            email: "alice@example.com".parse().unwrap(),
            display_name: None,
            password_hash: secret.into(),
            is_admin: false,
        };
        assert!(!format!("{new:?}").contains(secret));
        let user = User {
            account: Account {
                id: AccountId(1),
                kind: AccountKind::User,
                name: new.name,
                display_name: None,
                bio: None,
                created_at: Timestamp(0),
            },
            email: new.email,
            is_admin: false,
        };
        let record = LoginRecord { user, password_hash: secret.into(), suspended: false };
        assert!(!format!("{record:?}").contains(secret));
    }
}
