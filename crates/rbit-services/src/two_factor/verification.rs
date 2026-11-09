//! One cryptographic verifier with static legacy/original proof-write adapters.
use super::{
    AccountId, Challenge, MAX_RESPONSE_LEN, RelyingParty, SecondFactor, SecondFactorProof, ServiceError, TwoFactorService, TwoFactorStore,
    User, convert,
};
use crate::accounts::OriginalProofReceipt;
use rbit_identity::two_factor::{CodeFactor, RecoveryDigest, SecondFactorClaims};
use rbit_storage::sqlite::two_factor::CredentialProofWrite;
use std::future::Future;

impl TwoFactorService {
    pub(super) async fn verify(
        &self,
        user: &User,
        proof: SecondFactorProof<'_>,
        challenge: Option<Challenge>,
    ) -> Result<SecondFactor, ServiceError> {
        self.verify_with(user, proof, challenge, Legacy).await
    }
    pub(super) async fn verify_original(
        &self,
        user: &User,
        proof: SecondFactorProof<'_>,
        challenge: Option<Challenge>,
        receipt: &OriginalProofReceipt,
    ) -> Result<SecondFactor, ServiceError> {
        if receipt.account() != user.id() {
            return Err(ServiceError::NotFound);
        }
        self.verify_with(user, proof, challenge, Original(receipt)).await
    }
    /// Checks a second factor for an enrolled account: a 6-digit code (each time step
    /// works once), an unused recovery code (spent) or a passkey's answer to `challenge`.
    /// [`ServiceError::InvalidCredentials`] otherwise.
    async fn verify_with<M: ClaimMode>(
        &self,
        user: &User,
        proof: SecondFactorProof<'_>,
        challenge: Option<Challenge>,
        mode: M,
    ) -> Result<SecondFactor, ServiceError> {
        let code = match proof {
            SecondFactorProof::Code(code) => code,
            SecondFactorProof::Passkey(response) => {
                let challenge = challenge.ok_or(ServiceError::InvalidCredentials)?;
                return self.verify_passkey_with(user, &challenge, response, mode).await.map(|()| SecondFactor::Passkey);
            }
        };
        let claims = Claims { store: &self.store, mode };
        match self.authenticator().verify_code(user, code, &claims).await.map_err(|error| convert(error, |error| error))? {
            CodeFactor::RecoveryCode => Ok(SecondFactor::RecoveryCode),
            CodeFactor::Authenticator => Ok(SecondFactor::Authenticator),
        }
    }

    async fn verify_passkey_with<M: ClaimMode>(
        &self,
        user: &User,
        challenge: &Challenge,
        response: &str,
        mode: M,
    ) -> Result<(), ServiceError> {
        if response.len() > MAX_RESPONSE_LEN {
            return Err(ServiceError::InvalidCredentials);
        }
        let rp = self.relying_party()?;
        let refused = |error: rbit_auth::WebauthnError| {
            tracing::info!(user = %user.name(), %error, "passkey sign-in refused");
            ServiceError::InvalidCredentials
        };
        let credential_id = RelyingParty::assertion_credential_id(response).map_err(refused)?;
        let Some(passkey) = self.store.find_passkey(user.id(), &credential_id).await? else {
            return Err(ServiceError::InvalidCredentials);
        };
        let count = rp.verify_assertion(challenge, response, &passkey.public_key, passkey.sign_count).map_err(refused)?;
        if mode.passkey(&self.store, user.id(), passkey.id, passkey.sign_count, count).await? {
            Ok(())
        } else {
            Err(ServiceError::InvalidCredentials)
        }
    }
}

/// The identity claim port over a write mode: legacy claims or original-epoch claims.
struct Claims<'a, M> {
    store: &'a TwoFactorStore,
    mode: M,
}

impl<M: ClaimMode + Sync> SecondFactorClaims for Claims<'_, M> {
    type Error = ServiceError;
    fn spend_recovery_code(&self, account: AccountId, digest: &RecoveryDigest) -> impl Future<Output = Result<bool, ServiceError>> + Send {
        self.mode.recovery(self.store, account, digest)
    }
    fn claim_step(&self, account: AccountId, secret: &[u8], step: u64) -> impl Future<Output = Result<bool, ServiceError>> + Send {
        self.mode.totp(self.store, account, secret, step)
    }
}

trait ClaimMode: Copy + Send + Sync {
    fn recovery(
        self,
        store: &TwoFactorStore,
        account: AccountId,
        digest: &[u8; 32],
    ) -> impl Future<Output = Result<bool, ServiceError>> + Send;
    fn totp(
        self,
        store: &TwoFactorStore,
        account: AccountId,
        secret: &[u8],
        step: u64,
    ) -> impl Future<Output = Result<bool, ServiceError>> + Send;
    fn passkey(
        self,
        store: &TwoFactorStore,
        account: AccountId,
        id: i64,
        seen: u32,
        next: u32,
    ) -> impl Future<Output = Result<bool, ServiceError>> + Send;
}
#[derive(Clone, Copy)]
struct Legacy;
impl ClaimMode for Legacy {
    async fn recovery(self, store: &TwoFactorStore, account: AccountId, digest: &[u8; 32]) -> Result<bool, ServiceError> {
        Ok(store.spend_recovery_code(account, digest).await?)
    }
    async fn totp(self, store: &TwoFactorStore, account: AccountId, secret: &[u8], step: u64) -> Result<bool, ServiceError> {
        Ok(store.claim_step(account, secret, step).await?)
    }
    async fn passkey(self, store: &TwoFactorStore, account: AccountId, id: i64, seen: u32, next: u32) -> Result<bool, ServiceError> {
        Ok(store.use_passkey(account, id, seen, next).await?)
    }
}
#[derive(Clone, Copy)]
struct Original<'a>(&'a OriginalProofReceipt);
impl ClaimMode for Original<'_> {
    async fn recovery(self, store: &TwoFactorStore, account: AccountId, digest: &[u8; 32]) -> Result<bool, ServiceError> {
        claim(store.spend_recovery_code_at_epoch(account, digest, self.0.original_epoch()).await?)
    }
    async fn totp(self, store: &TwoFactorStore, account: AccountId, secret: &[u8], step: u64) -> Result<bool, ServiceError> {
        claim(store.claim_step_at_epoch(account, secret, step, self.0.original_epoch()).await?)
    }
    async fn passkey(self, store: &TwoFactorStore, account: AccountId, id: i64, seen: u32, next: u32) -> Result<bool, ServiceError> {
        claim(store.use_passkey_at_epoch(account, id, seen, next, self.0.original_epoch()).await?)
    }
}
fn claim(outcome: CredentialProofWrite) -> Result<bool, ServiceError> {
    match outcome {
        CredentialProofWrite::Claimed => Ok(true),
        CredentialProofWrite::Rejected => Ok(false),
        CredentialProofWrite::Invalidated => Err(ServiceError::NotFound),
    }
}
