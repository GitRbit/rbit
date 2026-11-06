//! Password changes compute first, then finalize token consumption and revocation.

use super::{RecoveryService, parse_token, require_admin, validate_password};
use crate::audit::{AuditAction, AuditEntry};
use crate::{DomainEvent, ServiceError};
use rbit_core::{AccountId, Timestamp, User};

impl RecoveryService {
    /// Sets a new password through a reset link, which is used up. Validates the password
    /// exactly like sign-up; an invalid password leaves the link usable.
    pub async fn reset_password(&self, token: &str, new_password: String) -> Result<(), ServiceError> {
        let account = self.reset_account(token).await?;
        validate_password(self.ctx.config.security.min_password_length, &new_password)?;
        let initial = self.ctx.database.accounts().password_change_required(account.id).await?;
        let attempt = self.passwords.begin()?;
        let hash = if initial {
            let current = self.ctx.database.profiles().password_hash(account.id).await?.ok_or(ServiceError::NotFound)?;
            attempt
                .hash_if_different(new_password, current)
                .await?
                .ok_or_else(|| ServiceError::Invalid("choose a different password from your initial password".to_owned()))?
        } else {
            attempt.hash(new_password).await?
        };

        let digest = parse_token(token)?;
        let service = self.clone();
        crate::credentials::finish_update(async move {
            service.store().reset_password(&digest, &hash, Timestamp::now()).await?.ok_or(ServiceError::NotFound)?;
            service.password_changed(account.id).await?;

            tracing::info!(user = %account.name, "password reset by email link");
            service
                .audit()
                .record(AuditEntry::new(AuditAction::PasswordReset).by_login(account.name.as_str()).on(account.name.as_str()))
                .await;
            service.ctx.events.publish(DomainEvent::PasswordChanged { user: account.id });
            Ok(())
        })
        .await
    }

    /// Sets a user's password directly (for instances without email, or locked-out users).
    /// Validated like sign-up; outstanding reset links stop working.
    pub async fn admin_set_password(&self, actor: &User, target: AccountId, new_password: String) -> Result<(), ServiceError> {
        require_admin(actor)?;
        let account = self.store().find_account(target).await?.ok_or(ServiceError::NotFound)?;
        if account.id == actor.id() {
            return Err(ServiceError::Invalid("change your own password from your account settings".to_owned()));
        }
        validate_password(self.ctx.config.security.min_password_length, &new_password)?;
        let hash = self.passwords.begin()?.hash(new_password).await?;
        let service = self.clone();
        let actor = actor.clone();
        crate::credentials::finish_update(async move {
            service.ctx.database.profiles().set_password_hash(account.id, &hash).await?;
            service.password_changed(account.id).await?;

            tracing::info!(admin = %actor.name(), user = %account.name, "administrator set a password");
            service.audit().record(AuditEntry::new(AuditAction::PasswordSet).by(&actor).on(account.name.as_str())).await;
            service.ctx.events.publish(DomainEvent::PasswordChanged { user: account.id });
            Ok(())
        })
        .await
    }
}
