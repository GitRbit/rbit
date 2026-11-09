//! The bounded, process-local owner of pending second sign-in steps.
//!
//! A pending entry may also retain an inactive original-proof receipt. Legacy and
//! receipted entries can never complete each other's step.

use std::{
    collections::{HashMap, hash_map::Entry},
    sync::{Mutex, MutexGuard, PoisonError},
    time::Instant,
};

use super::{MAX_PENDING_SIGN_INS, PENDING_SIGN_IN_ATTEMPTS, PENDING_SIGN_IN_TTL};
use crate::accounts::{OriginalProofReceipt, ProvenAccount};
use crate::passkeys::webauthn::Challenge;
use crate::{SessionToken, TokenDigest};
use rbit_contracts::AccountId;

/// One pending second step, keyed by its token's digest.
#[derive(Debug, Clone)]
pub struct PendingSignIn {
    pub account: AccountId,
    pub started: Instant,
    pub failures: u8,
    /// The passkey challenge offered on the second-step page, used at most once.
    pub challenge: Option<Challenge>,
    /// How the first step was passed (`password`, or a provider), for the audit.
    pub method: String,
    pub original: Option<OriginalProofReceipt>,
}

/// The account and, for receipted entries, the original proof awaiting a second factor.
pub type PendingProof = (AccountId, Option<OriginalProofReceipt>);

#[derive(Debug, Default)]
pub struct PendingSignIns(Mutex<HashMap<TokenDigest, PendingSignIn>>);

impl PendingSignIns {
    /// The raw entries, for deterministic tests of expiry; production never mutates them.
    #[doc(hidden)]
    pub fn lock(&self) -> MutexGuard<'_, HashMap<TokenDigest, PendingSignIn>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Opens a pending sign-in for `account`, whose first step was `method`.
    pub fn start(&self, account: AccountId, method: &str) -> SessionToken {
        self.start_bound(account, method, None)
    }

    /// Inactive seam: retains the SAME original proof through the second step. No route
    /// selects it until the fenced session protocol is activated as a whole.
    pub fn start_original(&self, proven: ProvenAccount, method: &str) -> SessionToken {
        let (_, receipt) = proven.into_parts();
        self.start_bound(receipt.account(), method, Some(receipt))
    }

    fn start_bound(&self, account: AccountId, method: &str, original: Option<OriginalProofReceipt>) -> SessionToken {
        let token = SessionToken::generate();
        let now = Instant::now();
        let mut pending = self.lock();
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
    pub fn account(&self, token: &SessionToken) -> Option<AccountId> {
        self.proof(token).map(|(account, _)| account)
    }

    /// The account and its original proof, if the entry is still valid; expired entries go.
    pub fn proof(&self, token: &SessionToken) -> Option<PendingProof> {
        let mut pending = self.lock();
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

    /// Claims a verified legacy sign-in exactly once, unless it expired or was cancelled
    /// while its proof was checked. Receipted entries are never claimed here.
    pub fn finish(&self, token: &SessionToken) -> Option<String> {
        let mut pending = self.lock();
        let Entry::Occupied(entry) = pending.entry(token.digest()) else { return None };
        if entry.get().original.is_some() {
            return None;
        }
        Some(entry.remove()).filter(|entry| entry.started.elapsed() < PENDING_SIGN_IN_TTL).map(|entry| entry.method)
    }

    /// Claims a verified receipted sign-in exactly once, only for the same account and
    /// original epoch it was opened with.
    pub fn finish_original(&self, token: &SessionToken, expected: &OriginalProofReceipt) -> Option<String> {
        let mut pending = self.lock();
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

    /// Offers `challenge` for the entry's passkey step; replaces an earlier offer.
    pub fn offer_challenge(&self, token: &SessionToken, challenge: Challenge) {
        if let Some(entry) = self.lock().get_mut(&token.digest()) {
            entry.challenge = Some(challenge);
        }
    }

    /// Takes the entry's passkey challenge, so each works once.
    pub fn take_challenge(&self, token: &SessionToken) -> Option<Challenge> {
        self.lock().get_mut(&token.digest()).and_then(|entry| entry.challenge.take())
    }

    /// Counts a wrong factor; after too many the entry is dropped.
    pub fn failed(&self, token: &SessionToken) {
        let mut pending = self.lock();
        let digest = token.digest();
        if let Some(entry) = pending.get_mut(&digest) {
            entry.failures += 1;
            if entry.failures >= PENDING_SIGN_IN_ATTEMPTS {
                pending.remove(&digest);
            }
        }
    }

    /// Abandons a pending sign-in (completed, cancelled or invalidated).
    pub fn end(&self, token: &SessionToken) {
        self.lock().remove(&token.digest());
    }
}
