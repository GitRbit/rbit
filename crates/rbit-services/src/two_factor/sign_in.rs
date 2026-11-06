//! One proof-consumption/accounting worker for legacy and original-receipt MFA.
use super::{
    AuditAction, AuditEntry, ClientInfo, SecondFactor, SecondFactorProof, ServiceError, Services, SessionToken, ThrottleScope, User,
};
use crate::accounts::{OriginalProofReceipt, ProvenAccount};

enum CompletedSignIn {
    Legacy(User),
    Original(ProvenAccount),
}
impl CompletedSignIn {
    fn user(&self) -> &User {
        match self {
            Self::Legacy(user) => user,
            Self::Original(proven) => proven.user(),
        }
    }
}
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
        let (account, original) = self.two_factor.pending_proof(pending).ok_or(ServiceError::NotFound)?;
        if original.is_some() != original_only {
            return Err(ServiceError::NotFound);
        }
        let key = account.0.to_string();
        self.throttle.check(ThrottleScope::TwoFactor, &key)?;
        let user = match &original {
            Some(receipt) => {
                self.two_factor.accounts.find_session_user_at_epoch(account, false, receipt.original_epoch()).await?.map(|row| row.user)
            }
            None => self.two_factor.accounts.find_active_user(account).await?,
        };
        let Some(user) = user else {
            self.two_factor.end_pending(pending);
            return Err(ServiceError::NotFound);
        };
        let challenge = matches!(proof, SecondFactorProof::Passkey(_)).then(|| self.two_factor.take_pending_challenge(pending)).flatten();
        match self.two_factor.verify(&user, proof, challenge).await {
            Ok(factor) => {
                let method = self.claim_sign_in(pending, original.as_ref()).ok_or(ServiceError::NotFound)?;
                let result = match original {
                    Some(receipt) => CompletedSignIn::Original(receipt.bind_user(user).map_err(|_| ServiceError::NotFound)?),
                    None => CompletedSignIn::Legacy(user),
                };
                self.throttle.reset(ThrottleScope::TwoFactor, &key);
                let entry = match factor {
                    SecondFactor::Authenticator => AuditEntry::new(AuditAction::SignIn).detail(format!("{method} and authenticator code")),
                    SecondFactor::Passkey => AuditEntry::new(AuditAction::SignIn).detail(format!("{method} and passkey")),
                    SecondFactor::RecoveryCode => {
                        AuditEntry::new(AuditAction::RecoveryCodeUsed).detail(format!("{method} and recovery code"))
                    }
                };
                self.audit.record(entry.by(result.user()).from_ip(client.ip)).await;
                Ok(result)
            }
            Err(ServiceError::InvalidCredentials) => {
                self.throttle.record_failure(ThrottleScope::TwoFactor, &key);
                self.two_factor.pending_failed(pending);
                self.audit
                    .record(AuditEntry::new(AuditAction::TwoFactorFailed).by(&user).from_ip(client.ip).detail("during sign-in"))
                    .await;
                Err(ServiceError::InvalidCredentials)
            }
            Err(other) => Err(other),
        }
    }
    fn claim_sign_in(&self, pending: &SessionToken, original: Option<&OriginalProofReceipt>) -> Option<String> {
        match original {
            Some(receipt) => self.two_factor.finish_original_pending(pending, receipt),
            None => self.two_factor.finish_pending(pending),
        }
    }
}
