//! A counted lease spans verification, commit and adopted finalization ownership.

use super::ProfileService;
use crate::{
    DomainEvent, ServiceError,
    audit::{AuditAction, AuditEntry, AuditService},
};
use rbit_core::User;

impl ProfileService {
    /// Replaces the password after re-checking the current one.
    pub fn change_password(
        &self,
        user: &User,
        current_password: String,
        new_password: String,
    ) -> impl std::future::Future<Output = Result<(), ServiceError>> {
        let mut current_password = zeroize::Zeroizing::new(current_password);
        let mut new_password = zeroize::Zeroizing::new(new_password);
        async move {
            self.validate_new_password(&new_password)?;
            if current_password.as_str() == new_password.as_str() {
                return Err(ServiceError::Invalid("choose a different password from your current password".into()));
            }
            // Reserve before the first await, but defer closed error until the original
            // admission boundary so validation/throttle/authentication precedence survives.
            let ownership = self.accounts.reserve_password_change();
            let verified = self.checked_password_hash(user, &current_password).await?;
            let attempt = self.accounts.password_work().begin()?;
            let ownership = ownership?;
            let result = attempt
                .verify_and_hash(std::mem::take(&mut *current_password), verified.to_string(), std::mem::take(&mut *new_password))
                .await;
            if matches!(&result, Err(rbit_identity::PasswordError::Hash(_))) {
                self.finish_password_check(user, true)?;
            }
            let hash = result?;
            self.finish_password_check(user, hash.is_some())?;
            let hash = hash.ok_or(ServiceError::InvalidCredentials)?;
            // Narrow worker capabilities deliberately exclude the tracking supervisor.
            let store = self.store();
            let finalizer = self.accounts.password_change_finalizer();
            let audit = AuditService::new(self.ctx.clone());
            let events = self.ctx.events.clone();
            let user = user.clone();
            crate::credentials::finish_update(ownership, move |context| async move {
                if !store.set_password_hash_if_current(user.id(), &hash, &verified).await? {
                    return Err(ServiceError::InvalidCredentials);
                }
                finalizer.preserving_current(&context, user.id()).await?;
                tracing::info!(user = %user.name(), "password changed");
                audit.record(AuditEntry::new(AuditAction::PasswordChanged).by(&user)).await;
                events.publish(DomainEvent::PasswordChanged { user: user.id() });
                Ok(())
            })
            .await
        }
    }
}
