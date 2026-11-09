//! The second sign-in step's adapter: the identity step decides, this records the audit.
use super::verification::Claims;
use super::{
    AuditAction, AuditEntry, ClientInfo, SecondFactor, SecondFactorProof, ServiceError, Services, SessionToken, Throttle, User, map_step,
};
use crate::accounts::ProvenAccount;
use rbit_identity::second_factor::{CompletedSignIn, SecondStepError};

impl Services {
    /// Finishes a legacy pending sign-in with TOTP, recovery code or passkey proof.
    /// Expired/used pending returns `NotFound`; incorrect proof returns `InvalidCredentials`.
    /// Receipted pending tokens cannot downgrade to the User-only compatibility API.
    pub async fn complete_sign_in<'a>(
        &self,
        pending: &SessionToken,
        proof: impl Into<SecondFactorProof<'a>>,
        client: &ClientInfo,
    ) -> Result<User, ServiceError> {
        match self.complete_second_factor(pending, proof.into(), client, false).await? {
            CompletedSignIn::Legacy(user) => Ok(user),
            CompletedSignIn::Original(_) => Err(ServiceError::NotFound),
        }
    }

    /// Inactive original-proof seam. Never adopts legacy pending or refreshes its epoch.
    /// The final issuer MUST use its fenced authoritative projection and persistent mode.
    pub async fn complete_original_sign_in<'a>(
        &self,
        pending: &SessionToken,
        proof: impl Into<SecondFactorProof<'a>>,
        client: &ClientInfo,
    ) -> Result<ProvenAccount, ServiceError> {
        match self.complete_second_factor(pending, proof.into(), client, true).await? {
            CompletedSignIn::Original(proven) => Ok(proven),
            CompletedSignIn::Legacy(_) => Err(ServiceError::NotFound),
        }
    }

    async fn complete_second_factor(
        &self,
        pending: &SessionToken,
        proof: SecondFactorProof<'_>,
        client: &ClientInfo,
        original_only: bool,
    ) -> Result<CompletedSignIn, ServiceError> {
        self.complete_second_factor_after(pending, proof, client, original_only, std::future::ready(())).await
    }

    // Private static test seam: production passes Ready (no allocation or I/O),
    // while real-store unit tests pause precisely after the authoritative lookup.
    async fn complete_second_factor_after(
        &self,
        pending: &SessionToken,
        proof: SecondFactorProof<'_>,
        client: &ClientInfo,
        original_only: bool,
        after_lookup: impl std::future::Future<Output = ()> + Send,
    ) -> Result<CompletedSignIn, ServiceError> {
        let throttle = Throttle(&self.throttle);
        let step = self.two_factor.second_step(&throttle);
        let store = &self.two_factor.store;
        let claims_for = |original: Option<&crate::accounts::OriginalProofReceipt>| Claims::for_receipt(store, original);
        match step.complete(pending, proof, original_only, claims_for, after_lookup).await {
            Ok(done) => {
                let method = done.method;
                let entry = match done.factor {
                    SecondFactor::Authenticator => AuditEntry::new(AuditAction::SignIn).detail(format!("{method} and authenticator code")),
                    SecondFactor::Passkey => AuditEntry::new(AuditAction::SignIn).detail(format!("{method} and passkey")),
                    SecondFactor::RecoveryCode => {
                        AuditEntry::new(AuditAction::RecoveryCodeUsed).detail(format!("{method} and recovery code"))
                    }
                };
                self.audit.record(entry.by(done.sign_in.user()).from_ip(client.ip)).await;
                Ok(done.sign_in)
            }
            Err(SecondStepError::WrongFactor { user, refused }) => {
                if let Some(error) = refused {
                    tracing::info!(user = %user.name(), %error, "passkey sign-in refused");
                }
                self.audit
                    .record(AuditEntry::new(AuditAction::TwoFactorFailed).by(&user).from_ip(client.ip).detail("during sign-in"))
                    .await;
                Err(ServiceError::InvalidCredentials)
            }
            Err(error) => Err(map_step(error)),
        }
    }
}

#[cfg(test)]
mod tests;
