//! Password changes retain admitted ownership across proof validation and finalization.

use super::{RecoveryService, map_commit, map_passwords};
use crate::audit::{AuditAction, AuditEntry};
use crate::{DomainEvent, ServiceError};
use rbit_core::{AccountId, User};
use rbit_identity::recovery::{RecoveryPasswords, commit_admin_password, commit_reset};
use rbit_storage::RecoveryCredentials;

impl RecoveryService {
    fn credentials(&self) -> RecoveryCredentials {
        RecoveryCredentials { accounts: self.ctx.database.accounts(), profiles: self.ctx.database.profiles() }
    }

    /// Consumes a valid reset link; invalid replacement passwords leave it usable.
    pub fn reset_password(&self, token: &str, new_password: String) -> impl std::future::Future<Output = Result<(), ServiceError>> {
        let mut new_password = zeroize::Zeroizing::new(new_password);
        async move {
            let (store, credentials) = (self.store(), self.credentials());
            let min = self.ctx.config.security.min_password_length;
            let passwords = RecoveryPasswords::new(&store, &credentials, &self.passwords, min);
            let prepared = passwords
                .prepare_reset(token, std::mem::take(&mut *new_password), || self.reserve_password_change())
                .await
                .map_err(map_passwords)?;
            let (account, digest, hash) = (prepared.account, prepared.digest, prepared.hash);
            let finalizer = self.session_finalizer();
            let audit = self.audit();
            let events = self.ctx.events.clone();
            crate::credentials::finish_update(prepared.ownership, move |context| async move {
                commit_reset(&store, digest, &hash).await.map_err(map_commit)?;
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
            let (store, credentials) = (self.store(), self.credentials());
            let min = self.ctx.config.security.min_password_length;
            let passwords = RecoveryPasswords::new(&store, &credentials, &self.passwords, min);
            let prepared = passwords
                .prepare_admin_password(actor, target, std::mem::take(&mut *new_password), || self.reserve_password_change())
                .await
                .map_err(map_passwords)?;
            let (account, hash) = (prepared.account, prepared.hash);
            let finalizer = self.session_finalizer();
            let audit = self.audit();
            let events = self.ctx.events.clone();
            let actor = actor.clone();
            crate::credentials::finish_update(prepared.ownership, move |context| async move {
                commit_admin_password(&credentials, account.id, &hash).await.map_err(map_commit)?;
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
