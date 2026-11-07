//! The existing bounded pending owner also retains original first-factor receipts.
use super::{
    AccountId, Challenge, HashMap, Instant, MAX_PENDING_SIGN_INS, PENDING_SIGN_IN_ATTEMPTS, PENDING_SIGN_IN_TTL, PoisonError, SessionToken,
    TokenDigest, TwoFactorService,
};
use crate::accounts::{OriginalProofReceipt, ProvenAccount};
use std::collections::hash_map::Entry;

#[derive(Debug, Clone)]
pub(super) struct PendingSignIn {
    pub account: AccountId,
    pub started: Instant,
    pub failures: u8,
    pub challenge: Option<Challenge>,
    pub method: String,
    pub original: Option<OriginalProofReceipt>,
}
pub(super) type PendingProof = (AccountId, Option<OriginalProofReceipt>);

impl TwoFactorService {
    pub(super) fn lock_pending(&self) -> std::sync::MutexGuard<'_, HashMap<TokenDigest, PendingSignIn>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Opens a pending sign-in for `account`, whose first step was `method` (see
    /// [`PendingSignIn`]); external sign-in (milestone 3, O) continues here too.
    pub(crate) fn start_pending(&self, account: AccountId, method: &str) -> SessionToken {
        self.start_pending_bound(account, method, None)
    }

    /// Inactive seam: starts local MFA with the SAME original password proof. Current
    /// sign-in routes retain legacy pending until the full fenced mode is activated.
    /// The sign-in orchestrator checks enrollment before calling, without another query here.
    pub fn start_original_password_pending(&self, proven: ProvenAccount) -> SessionToken {
        self.start_original_provider_pending(proven, "password")
    }

    /// Inactive provider counterpart, retaining the same proof and existing configured
    /// provider name for the second-factor audit. No enrollment lookup or new map owner.
    pub(crate) fn start_original_provider_pending(&self, proven: ProvenAccount, method: &str) -> SessionToken {
        let (_, receipt) = proven.into_parts();
        self.start_pending_bound(receipt.account(), method, Some(receipt))
    }

    fn start_pending_bound(&self, account: AccountId, method: &str, original: Option<OriginalProofReceipt>) -> SessionToken {
        let token = SessionToken::generate();
        let now = Instant::now();
        let mut pending = self.lock_pending();
        pending.retain(|_, entry| now.duration_since(entry.started) < PENDING_SIGN_IN_TTL);
        if pending.len() >= MAX_PENDING_SIGN_INS
            && let Some(oldest) = pending.iter().min_by_key(|(_, entry)| entry.started).map(|(digest, _)| *digest)
        {
            pending.remove(&oldest);
        }
        pending.insert(
            token.digest(),
            PendingSignIn { account, started: now, failures: 0, challenge: None, method: method.to_owned(), original },
        );
        token
    }

    /// The account waiting for its second factor under `token`, if still valid.
    pub fn pending_account(&self, token: &SessionToken) -> Option<AccountId> {
        let mut pending = self.lock_pending();
        let digest = token.digest();
        match pending.get(&digest) {
            Some(entry) if entry.started.elapsed() < PENDING_SIGN_IN_TTL => Some(entry.account),
            Some(_) => {
                pending.remove(&digest);
                None
            }
            None => None,
        }
    }

    /// Claims a successfully verified sign-in exactly once, provided it has not expired
    /// or been cancelled while its proof was being checked.
    pub(super) fn finish_pending(&self, token: &SessionToken) -> Option<String> {
        let mut pending = self.lock_pending();
        let Entry::Occupied(entry) = pending.entry(token.digest()) else { return None };
        if entry.get().original.is_some() {
            return None;
        }
        Some(entry.remove()).filter(|entry| entry.started.elapsed() < PENDING_SIGN_IN_TTL).map(|entry| entry.method)
    }

    /// Takes the pending sign-in's passkey challenge, so each works once.
    pub(super) fn take_pending_challenge(&self, token: &SessionToken) -> Option<Challenge> {
        self.lock_pending().get_mut(&token.digest()).and_then(|entry| entry.challenge.take())
    }

    /// Counts a wrong code; after too many the pending sign-in is dropped.
    pub(super) fn pending_failed(&self, token: &SessionToken) {
        let mut pending = self.lock_pending();
        let digest = token.digest();
        if let Some(entry) = pending.get_mut(&digest) {
            entry.failures += 1;
            if entry.failures >= PENDING_SIGN_IN_ATTEMPTS {
                pending.remove(&digest);
            }
        }
    }

    /// Abandons a pending sign-in (completed, or cancelled by the user).
    pub fn end_pending(&self, token: &SessionToken) {
        self.lock_pending().remove(&token.digest());
    }
    pub(super) fn pending_proof(&self, token: &SessionToken) -> Option<PendingProof> {
        let mut pending = self.lock_pending();
        let digest = token.digest();
        match pending.get(&digest) {
            Some(entry) if entry.started.elapsed() < PENDING_SIGN_IN_TTL => Some((entry.account, entry.original.clone())),
            Some(_) => {
                pending.remove(&digest);
                None
            }
            None => None,
        }
    }

    pub(super) fn finish_original_pending(&self, token: &SessionToken, expected: &OriginalProofReceipt) -> Option<String> {
        let mut pending = self.lock_pending();
        let Entry::Occupied(entry) = pending.entry(token.digest()) else { return None };
        let snapshot = entry.get();
        let receipt = snapshot.original.as_ref()?;
        if snapshot.account != expected.account()
            || receipt.account() != expected.account()
            || receipt.original_epoch() != expected.original_epoch()
        {
            return None;
        }
        Some(entry.remove()).filter(|entry| entry.started.elapsed() < PENDING_SIGN_IN_TTL).map(|entry| entry.method)
    }
}

#[cfg(test)]
mod tests;
