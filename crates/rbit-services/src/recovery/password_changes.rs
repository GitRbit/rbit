//! Password changes retain admitted ownership across proof validation and finalization.

use super::{RecoveryService, parse_token, require_admin, validate_password};
use crate::audit::{AuditAction, AuditEntry};
use crate::{DomainEvent, ServiceError};
use rbit_core::{AccountId, Timestamp, User};

impl RecoveryService {
    /// Consumes a valid reset link; invalid replacement passwords leave it usable.
    pub fn reset_password(&self, token: &str, new_password: String) -> impl std::future::Future<Output = Result<(), ServiceError>> {
        let mut new_password = zeroize::Zeroizing::new(new_password);
        async move {
            let ownership = self.reserve_password_change();
            let account = self.reset_account(token).await?;
            validate_password(self.ctx.config.security.min_password_length, &new_password)?;
            let initial = self.ctx.database.accounts().password_change_required(account.id).await?;
            let attempt = self.passwords.begin()?;
            let ownership = ownership?;
            let hash = if initial {
                let current = self.ctx.database.profiles().password_hash(account.id).await?.ok_or(ServiceError::NotFound)?;
                attempt
                    .hash_if_different(std::mem::take(&mut *new_password), current)
                    .await?
                    .ok_or_else(|| ServiceError::Invalid("choose a different password from your initial password".into()))?
            } else {
                attempt.hash(std::mem::take(&mut *new_password)).await?
            };
            let digest = parse_token(token)?;
            let store = self.store();
            let finalizer = self.session_finalizer();
            let audit = self.audit();
            let events = self.ctx.events.clone();
            crate::credentials::finish_update(ownership, move |context| async move {
                store.reset_password(&digest, &hash, Timestamp::now()).await?.ok_or(ServiceError::NotFound)?;
                finalizer.committed(&context, account.id).await?;
                tracing::info!(user = %account.name, "password reset by email link");
                audit.record(AuditEntry::new(AuditAction::PasswordReset).by_login(account.name.as_str()).on(account.name.as_str())).await;
                events.publish(DomainEvent::PasswordChanged { user: account.id });
                Ok(())
            })
            .await
        }
    }

    /// Sets another user's initial password and invalidates outstanding reset links.
    pub fn admin_set_password(
        &self,
        actor: &User,
        target: AccountId,
        new_password: String,
    ) -> impl std::future::Future<Output = Result<(), ServiceError>> {
        let mut new_password = zeroize::Zeroizing::new(new_password);
        async move {
            require_admin(actor)?;
            let ownership = self.reserve_password_change();
            let account = self.store().find_account(target).await?.ok_or(ServiceError::NotFound)?;
            if account.id == actor.id() {
                return Err(ServiceError::Invalid("change your own password from your account settings".into()));
            }
            validate_password(self.ctx.config.security.min_password_length, &new_password)?;
            let attempt = self.passwords.begin()?;
            let ownership = ownership?;
            let hash = attempt.hash(std::mem::take(&mut *new_password)).await?;
            let store = self.ctx.database.profiles();
            let finalizer = self.session_finalizer();
            let audit = self.audit();
            let events = self.ctx.events.clone();
            let actor = actor.clone();
            crate::credentials::finish_update(ownership, move |context| async move {
                store.set_password_hash(account.id, &hash).await?;
                finalizer.committed(&context, account.id).await?;
                tracing::info!(admin = %actor.name(), user = %account.name, "administrator set a password");
                audit.record(AuditEntry::new(AuditAction::PasswordSet).by(&actor).on(account.name.as_str())).await;
                events.publish(DomainEvent::PasswordChanged { user: account.id });
                Ok(())
            })
            .await
        }
    }
}
