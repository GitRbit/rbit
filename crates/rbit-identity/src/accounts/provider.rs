//! Inactive verified-provider receipt application. Remote verification stays with
//! the existing callback adapter; nonce/PKCE is never treated as a local password.
use super::{
    ProvenAccount,
    proof::{creation_proven, provider_proven},
};
use rbit_contracts::AccountId;
use rbit_identity_models::accounts::{NewUser, ProviderAccountSnapshot, ProviderCreationRepository, ProviderLoginRepository};
use std::fmt;

/// Captured by the FIRST authoritative account acceptance query. No constructor,
/// Serde or epoch-refresh operation is exposed to the callback orchestrator.
///
/// ```compile_fail
/// use rbit_identity::accounts::ProviderOrigin;
/// use rbit_identity_models::accounts::ProviderAccountSnapshot;
/// fn forge(snapshot: ProviderAccountSnapshot) { let _ = ProviderOrigin(snapshot); }
/// ```
///
/// ```compile_fail
/// use rbit_identity::accounts::ProviderOrigin;
/// fn serialize(origin: &ProviderOrigin) { serde_json::to_string(origin).unwrap(); }
/// ```
pub struct ProviderOrigin(ProviderAccountSnapshot);
impl ProviderOrigin {
    pub fn account(&self) -> AccountId {
        self.0.user.id()
    }
    pub fn original_epoch(&self) -> rbit_identity_models::credential::CredentialEpoch {
        self.0.fence.epoch
    }
    pub fn was_pending(&self) -> bool {
        self.0.fence.pending
    }
}
impl fmt::Debug for ProviderOrigin {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("ProviderOrigin").field("proof", &"<redacted>").finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum ProviderProofError<E> {
    Store(E),
    /// A changed/pending/unavailable account is not a wrong provider credential.
    Unavailable,
}

#[derive(Debug)]
pub struct ProviderAuthentication;
impl ProviderAuthentication {
    /// The port publishes only after its account+identity transaction commits. No
    /// current-epoch lookup is permitted after insertion or after a failed commit.
    pub async fn register<R: ProviderCreationRepository>(
        repository: &R,
        new: NewUser,
        registration_open: bool,
    ) -> Result<ProvenAccount, R::Error> {
        Ok(creation_proven(repository.create_original(new, registration_open).await?))
    }
    pub async fn capture_linked<R: ProviderLoginRepository>(
        repository: &R,
        provider: &str,
        subject: &str,
    ) -> Result<Option<(R::Identity, ProviderOrigin)>, R::Error> {
        Ok(repository.find_linked_snapshot(provider, subject).await?.map(|snapshot| (snapshot.identity, ProviderOrigin(snapshot.account))))
    }

    pub async fn capture_verified_email<R: ProviderLoginRepository>(
        repository: &R,
        email: &str,
    ) -> Result<Option<ProviderOrigin>, R::Error> {
        Ok(repository.find_verified_email_snapshot(email).await?.map(ProviderOrigin))
    }

    /// Consume only genuine first-acceptance evidence. This intermediate result is
    /// suitable for optional local MFA, NEVER a replacement for the final issuer's
    /// authoritative fixed-epoch lookup. Captured pending cannot be rehabilitated.
    pub fn first_factor(origin: ProviderOrigin) -> Option<ProvenAccount> {
        if origin.0.fence.pending {
            return None;
        }
        Some(provider_proven(origin.0.user, origin.0.fence.epoch))
    }

    /// Call at the existing final active-user lookup, AFTER the legacy suspension
    /// check/audit. Captured pending blocks even if later cleared; changed epoch or
    /// new pending intent blocks in the authoritative one-statement projection.
    pub async fn complete<R: ProviderLoginRepository>(
        repository: &R,
        origin: ProviderOrigin,
    ) -> Result<ProvenAccount, ProviderProofError<R::Error>> {
        if origin.0.fence.pending {
            return Err(ProviderProofError::Unavailable);
        }
        let user = repository
            .find_active_at_epoch(origin.0.user.id(), origin.0.fence.epoch)
            .await
            .map_err(ProviderProofError::Store)?
            .filter(|user| user.id() == origin.0.user.id())
            .ok_or(ProviderProofError::Unavailable)?;
        Ok(provider_proven(user, origin.0.fence.epoch))
    }
}
