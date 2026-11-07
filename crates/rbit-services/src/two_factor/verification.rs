//! One cryptographic verifier with static legacy/original proof-write adapters.
use super::{
    AccountId, Challenge, MAX_RESPONSE_LEN, RecoveryCode, RelyingParty, SecondFactor, SecondFactorProof, ServiceError, Timestamp,
    TotpSecret, TwoFactorRecord, TwoFactorService, TwoFactorStore, User,
};
use crate::accounts::OriginalProofReceipt;
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
        if let Some(recovery) = RecoveryCode::parse(code) {
            return if mode.recovery(&self.store, user.id(), &recovery.digest(user.id().0)).await? {
                Ok(SecondFactor::RecoveryCode)
            } else {
                Err(ServiceError::InvalidCredentials)
            };
        }
        let Some(record) = self.store.find(user.id()).await?.filter(TwoFactorRecord::is_enabled) else {
            return Err(ServiceError::InvalidCredentials);
        };
        let secret = TotpSecret::from_bytes(&record.secret).ok_or(ServiceError::InvalidCredentials)?;
        match secret.verify(code, Timestamp::now().seconds()) {
            Some(step) if mode.totp(&self.store, user.id(), &record.secret, step).await? => Ok(SecondFactor::Authenticator),
            _ => Err(ServiceError::InvalidCredentials),
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

trait ClaimMode: Copy + Send {
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
