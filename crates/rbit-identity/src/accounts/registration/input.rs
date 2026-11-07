//! Plaintext enters zeroizing ownership synchronously, before validation or I/O.

use zeroize::Zeroizing;

use super::MAX_PASSWORD_LEN;
use crate::{
    PasswordWork,
    accounts::{AccountError, NewUser, SignUp},
};

pub(super) struct ProtectedSignUp {
    username: String,
    email: String,
    password: Zeroizing<String>,
}

impl ProtectedSignUp {
    pub(super) fn new(input: SignUp) -> Self {
        Self { username: input.username, email: input.email, password: Zeroizing::new(input.password) }
    }

    pub(super) async fn prepare<E>(mut self, passwords: &PasswordWork, minimum: usize, is_admin: bool) -> Result<NewUser, AccountError<E>> {
        let name = self.username.trim().parse().map_err(AccountError::Name)?;
        let email = self.email.trim().parse().map_err(AccountError::Name)?;
        let length = crate::classify_password_length(&self.password, minimum, MAX_PASSWORD_LEN);
        if length == crate::PasswordLength::TooShort {
            return Err(AccountError::Invalid(format!("password must be at least {minimum} characters")));
        }
        if length == crate::PasswordLength::TooLong {
            return Err(AccountError::Invalid(format!("password must be at most {MAX_PASSWORD_LEN} characters")));
        }
        let attempt = passwords.begin().map_err(AccountError::Admission)?;
        let password_hash = attempt.hash(std::mem::take(&mut *self.password)).await.map_err(AccountError::Password)?;
        Ok(NewUser { name, email, display_name: None, password_hash, is_admin })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::ZeroizeOnDrop;

    fn assert_protected<T: ZeroizeOnDrop>(_: &T) {}

    #[test]
    fn synchronous_input_conversion_places_plaintext_in_drop_zeroizing_ownership() {
        let protected = ProtectedSignUp::new(SignUp { password: "sensitive".into(), ..SignUp::default() });
        assert_protected(&protected.password);
        assert_eq!(protected.password.as_str(), "sensitive");
        // No unsafe inspection of freed memory; Zeroizing owns the documented erasure.
    }
}
