//! Outstanding settings-page challenges: bounded, short-lived and single-use.

use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use super::webauthn::Challenge;
use rbit_contracts::AccountId;

/// How long a passkey challenge issued on a settings page stays valid.
const CHALLENGE_TTL: Duration = Duration::from_mins(10);
/// Accounts with outstanding settings challenges kept at most (oldest beyond this are dropped).
const MAX_CHALLENGES: usize = 10_000;
/// Outstanding challenges per account and ceremony.
const CHALLENGES_PER_ACCOUNT: usize = 4;

/// What a settings-page challenge is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ceremony {
    /// Registering a new passkey.
    Register,
    /// Confirming a sensitive change with an existing passkey.
    Confirm,
}

type Issued = HashMap<(AccountId, Ceremony), Vec<(Challenge, Instant)>>;

/// The process-local owner of settings-page challenges, newest last per account.
#[derive(Debug, Default)]
pub struct ChallengeBook(Mutex<Issued>);

impl ChallengeBook {
    fn lock(&self) -> MutexGuard<'_, Issued> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A fresh challenge for `ceremony`. The account's few most recent ones stay valid, so
    /// a second tab (or a page fetched again) doesn't break the one already open.
    pub fn issue(&self, account: AccountId, ceremony: Ceremony) -> Challenge {
        let challenge = Challenge::generate();
        let now = Instant::now();
        let mut challenges = self.lock();
        challenges.retain(|_, issued| {
            issued.retain(|(_, at)| now.duration_since(*at) < CHALLENGE_TTL);
            !issued.is_empty()
        });
        if challenges.len() >= MAX_CHALLENGES
            && let Some(oldest) = challenges.iter().min_by_key(|(_, issued)| issued.first().map(|(_, at)| *at)).map(|(key, _)| *key)
        {
            challenges.remove(&oldest);
        }
        let issued = challenges.entry((account, ceremony)).or_default();
        if issued.len() >= CHALLENGES_PER_ACCOUNT {
            issued.remove(0);
        }
        issued.push((challenge.clone(), now));
        challenge
    }

    /// Takes (so it can't be used twice) the account's outstanding challenge for
    /// `ceremony` that `response` answers.
    pub fn take(&self, account: AccountId, ceremony: Ceremony, response: &str) -> Option<Challenge> {
        let answered = Challenge::answered_by(response).ok()?;
        let mut challenges = self.lock();
        let issued = challenges.get_mut(&(account, ceremony))?;
        let index = issued.iter().position(|(challenge, _)| *challenge == answered)?;
        let (challenge, at) = issued.remove(index);
        (at.elapsed() < CHALLENGE_TTL).then_some(challenge)
    }
}
