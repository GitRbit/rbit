//! Owned plaintext is protected before constructing password-operation futures.

use super::{AuditAction, AuditEntry, ClientInfo, SecondFactorProof, ServiceError, Services, User, formatted, map};

impl Services {
    /// Removes passkey `id`; needs the password and a proof.
    pub fn remove_passkey<'a>(
        &self,
        user: &User,
        id: i64,
        password: String,
        proof: impl Into<SecondFactorProof<'a>>,
        client: &ClientInfo,
    ) -> impl std::future::Future<Output = Result<(), ServiceError>> {
        let mut password = zeroize::Zeroizing::new(password);
        async move {
            let Some(passkey) = self.two_factor.store.passkeys(user.id()).await?.into_iter().find(|passkey| passkey.id == id) else {
                return Err(ServiceError::NotFound);
            };
            self.check_password_and_proof(user, std::mem::take(&mut *password), proof.into(), client).await?;
            self.two_factor.store.remove_passkey(user.id(), id).await?;
            self.audit.record(AuditEntry::new(AuditAction::PasskeyRemoved).by(user).from_ip(client.ip).detail(&passkey.name)).await;
            Ok(())
        }
    }

    /// Removes the authenticator app; needs the password and a proof. Two-factor
    /// authentication stays on if the account has passkeys.
    pub fn disable_two_factor<'a>(
        &self,
        user: &User,
        password: String,
        proof: impl Into<SecondFactorProof<'a>>,
        client: &ClientInfo,
    ) -> impl std::future::Future<Output = Result<(), ServiceError>> {
        let mut password = zeroize::Zeroizing::new(password);
        async move {
            self.check_password_and_proof(user, std::mem::take(&mut *password), proof.into(), client).await?;
            self.two_factor.store.disable(user.id()).await?;
            self.audit
                .record(AuditEntry::new(AuditAction::TwoFactorDisabled).by(user).from_ip(client.ip).detail("authenticator app"))
                .await;
            Ok(())
        }
    }

    /// Replaces the recovery codes (invalidating the old ones); needs the password and a proof.
    pub fn regenerate_recovery_codes<'a>(
        &self,
        user: &User,
        password: String,
        proof: impl Into<SecondFactorProof<'a>>,
        client: &ClientInfo,
    ) -> impl std::future::Future<Output = Result<Vec<String>, ServiceError>> {
        let mut password = zeroize::Zeroizing::new(password);
        async move {
            self.check_password_and_proof(user, std::mem::take(&mut *password), proof.into(), client).await?;
            let codes = self.two_factor.authenticator().regenerate_recovery_codes(user).await.map_err(map)?;
            self.audit.record(AuditEntry::new(AuditAction::RecoveryCodesRegenerated).by(user).from_ip(client.ip)).await;
            Ok(formatted(&codes))
        }
    }
}
