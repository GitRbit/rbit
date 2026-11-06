//! Password checks retain account throttles and the exact stored hash used by CAS writes.

use rbit_core::User;

use super::{MAX_PASSWORD_LEN, ProfileService};
use crate::{ServiceError, ThrottleScope};

impl ProfileService {
    pub(super) async fn checked_password_hash(&self, user: &User, password: &str) -> Result<zeroize::Zeroizing<String>, ServiceError> {
        let key = user.id().0.to_string();
        self.throttle.check(ThrottleScope::PasswordCheck, &key)?;
        if password.len() > MAX_PASSWORD_LEN * 4 {
            self.finish_password_check(user, false)?;
        }
        self.store().password_hash(user.id()).await?.map(zeroize::Zeroizing::new).ok_or(ServiceError::NotFound)
    }

    pub(super) fn finish_password_check(&self, user: &User, valid: bool) -> Result<(), ServiceError> {
        let key = user.id().0.to_string();
        if !valid {
            self.throttle.record_failure(ThrottleScope::PasswordCheck, &key);
            return Err(ServiceError::InvalidCredentials);
        }
        self.throttle.reset(ThrottleScope::PasswordCheck, &key);
        Ok(())
    }

    pub(super) fn verify_password(
        &self,
        user: &User,
        password: String,
    ) -> impl std::future::Future<Output = Result<zeroize::Zeroizing<String>, ServiceError>> {
        let mut password = zeroize::Zeroizing::new(password);
        async move {
            let hash = self.checked_password_hash(user, &password).await?;
            let valid = self.accounts.password_work().begin()?.verify(std::mem::take(&mut *password), hash.to_string()).await?;
            self.finish_password_check(user, valid)?;
            Ok(hash)
        }
    }
}
