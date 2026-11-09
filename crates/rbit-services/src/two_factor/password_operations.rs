//! Owned plaintext is protected before constructing password-operation futures.

use super::verification::Claims;
use super::{AuditAction, AuditEntry, ClientInfo, SecondFactorProof, ServiceError, Services, Throttle, User, formatted, map, map_step};
use rbit_identity::second_factor::SecondStepError;
use rbit_storage::StorageError;

impl Services {
    /// Audits a wrong factor in settings; other outcomes map unchanged.
    pub(super) async fn settings_failure(
        &self,
        error: SecondStepError<StorageError, ServiceError, StorageError>,
        client: &ClientInfo,
    ) -> ServiceError {
        let SecondStepError::WrongFactor { user, refused } = error else { return map_step(error) };
        if let Some(error) = refused {
            tracing::info!(user = %user.name(), %error, "passkey sign-in refused");
        }
        self.audit.record(AuditEntry::new(AuditAction::TwoFactorFailed).by(&user).from_ip(client.ip).detail("in settings")).await;
        ServiceError::InvalidCredentials
    }

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
            let throttle = Throttle(&self.throttle);
            let claims = Claims::legacy(&self.two_factor.store);
            let step = self.two_factor.second_step(&throttle);
            let removed = step.remove_passkey(user, id, std::mem::take(&mut *password), proof, &claims).await;
            let name = match removed {
                Ok(name) => name,
                Err(error) => return Err(self.settings_failure(error, client).await),
            };
            self.audit.record(AuditEntry::new(AuditAction::PasskeyRemoved).by(user).from_ip(client.ip).detail(&name)).await;
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
            let throttle = Throttle(&self.throttle);
            let claims = Claims::legacy(&self.two_factor.store);
            let step = self.two_factor.second_step(&throttle);
            if let Err(error) = step.disable_authenticator(user, std::mem::take(&mut *password), proof.into(), &claims).await {
                return Err(self.settings_failure(error, client).await);
            }
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
