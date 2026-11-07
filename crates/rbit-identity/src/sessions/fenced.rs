//! Explicit inactive epoch owner: original proof, captured bindings and one SQL read.
mod proof;
mod view;
use super::application::{DAY, LAST_SEEN_RESOLUTION};
use super::{
    ActiveSession, ClientInfo, EpochSessionRepository, EpochSessionSnapshot, EpochSessionUserRepository, SessionClock, SessionPolicy,
    SessionRecord, SystemSessionClock, session_id,
};
use crate::SessionToken;
pub use proof::EpochIssuanceProof;
use rbit_identity_models::credential::CredentialEpoch;
pub use view::{IssuedEpochSession, ResolvedEpochSession};

#[derive(Debug)]
pub enum EpochSessionError<S, A> {
    Session(S),
    Account(A),
    Rejected,
    Collision,
}
pub type EpochSessionResult<T, S, A> = Result<T, EpochSessionError<S, A>>;

#[derive(Debug)]
pub struct EpochSessionApplication<'a, S, A, C = SystemSessionClock> {
    sessions: &'a S,
    accounts: &'a A,
    policy: SessionPolicy,
    clock: C,
}
impl<'a, S: EpochSessionRepository, A: EpochSessionUserRepository> EpochSessionApplication<'a, S, A> {
    pub fn new(sessions: &'a S, accounts: &'a A, policy: SessionPolicy) -> Self {
        Self::with_clock(sessions, accounts, policy, SystemSessionClock)
    }
}
impl<'a, S: EpochSessionRepository, A: EpochSessionUserRepository, C: SessionClock> EpochSessionApplication<'a, S, A, C> {
    pub fn with_clock(sessions: &'a S, accounts: &'a A, policy: SessionPolicy, clock: C) -> Self {
        Self { sessions, accounts, policy, clock }
    }

    /// The sole original-proof issuance check. Never convert User/current epoch into
    /// proof or restamp an old origin. Snapshot-only INITIAL cannot issue a session.
    pub async fn issue<P: EpochIssuanceProof>(
        &self,
        proof: &P,
        client: &ClientInfo,
    ) -> EpochSessionResult<IssuedEpochSession<S::Snapshot>, S::Error, A::Error> {
        let original = proof.original_epoch();
        if original == CredentialEpoch::INITIAL {
            return Err(EpochSessionError::Rejected);
        }
        let Some(user) = self
            .accounts
            .find_session_user_at_epoch(proof.account(), self.policy.notifications, original)
            .await
            .map_err(EpochSessionError::Account)?
        else {
            return Err(EpochSessionError::Rejected);
        };
        if user.user.id() != proof.account() {
            return Err(EpochSessionError::Rejected);
        }
        let token = SessionToken::generate();
        let ip = client.ip.map(|ip| ip.to_canonical().to_string());
        let created_for_cap = self.clock.now();
        let expires = self.policy.expiry(created_for_cap, self.clock.now());
        let record = SessionRecord::at(proof.account(), expires, ip, &client.user_agent, self.clock.now());
        let Some(snapshot) = self.sessions.put_bound(token.digest(), record, original).await.map_err(EpochSessionError::Session)? else {
            return Err(EpochSessionError::Collision);
        };
        if !snapshot.is_bound()
            || snapshot.digest() != token.digest()
            || snapshot.epoch() != original
            || snapshot.record().account_id != proof.account()
        {
            return Err(EpochSessionError::Rejected);
        }
        Ok(IssuedEpochSession { token, session: ResolvedEpochSession::new(user, snapshot) })
    }

    /// One captured redb read + one original-epoch SQL projection. Common live
    /// reads do not renew/write, spawn an owner task, or scan the account index.
    pub async fn resume(&self, token: &SessionToken) -> EpochSessionResult<Option<ResolvedEpochSession<S::Snapshot>>, S::Error, A::Error> {
        let Some(snapshot) = self.sessions.get_snapshot(token.digest()).await.map_err(EpochSessionError::Session)? else { return Ok(None) };
        let record = snapshot.record();
        let now = self.clock.now();
        if !snapshot.is_bound() || snapshot.epoch() == CredentialEpoch::INITIAL || record.expires_at <= now {
            return Ok(None);
        }
        if self.policy.maximum_end(record.created_at).is_some_and(|end| end <= now) {
            self.sessions.delete(&snapshot).await.map_err(EpochSessionError::Session)?;
            return Ok(None);
        }
        let Some(user) = self
            .accounts
            .find_session_user_at_epoch(record.account_id, self.policy.notifications, snapshot.epoch())
            .await
            .map_err(EpochSessionError::Account)?
        else {
            self.sessions.delete(&snapshot).await.map_err(EpochSessionError::Session)?;
            return Ok(None);
        };
        if user.user.id() != record.account_id {
            return Err(EpochSessionError::Rejected);
        }
        let renewed = self.policy.expiry(record.created_at, self.clock.now());
        let renew = renewed.seconds() - record.expires_at.seconds() > DAY;
        let seen = now.seconds() - record.last_seen_at.seconds() >= LAST_SEEN_RESOLUTION;
        if (renew || seen) && !self.sessions.renew(&snapshot, renewed, now).await.map_err(EpochSessionError::Session)? {
            return Ok(None);
        }
        Ok(Some(ResolvedEpochSession::new(user, snapshot)))
    }

    pub async fn end(&self, token: &SessionToken) -> EpochSessionResult<(), S::Error, A::Error> {
        if let Some(snapshot) = self.sessions.get_snapshot(token.digest()).await.map_err(EpochSessionError::Session)?
            && snapshot.is_bound()
        {
            self.sessions.logout(&snapshot).await.map_err(EpochSessionError::Session)?;
        }
        Ok(())
    }

    pub async fn list(&self, current: &ResolvedEpochSession<S::Snapshot>) -> EpochSessionResult<Vec<ActiveSession>, S::Error, A::Error> {
        let snapshot = current.snapshot();
        let rows = self.sessions.list_bound(snapshot.record().account_id).await.map_err(EpochSessionError::Session)?;
        let current_digest = snapshot.digest();
        Ok(rows
            .into_iter()
            .filter(|row| row.is_bound() && row.epoch() == snapshot.epoch() && row.record().account_id == snapshot.record().account_id)
            .map(|row| ActiveSession::from_stored(row.into_stored(), Some(&current_digest)))
            .collect())
    }

    pub async fn revoke(
        &self,
        current: &ResolvedEpochSession<S::Snapshot>,
        public_id: &str,
    ) -> EpochSessionResult<SessionRecord, S::Error, A::Error> {
        let snapshot = current.snapshot();
        let rows = self.sessions.list_bound(snapshot.record().account_id).await.map_err(EpochSessionError::Session)?;
        let expected = rows
            .into_iter()
            .find(|row| {
                row.is_bound()
                    && row.epoch() == snapshot.epoch()
                    && row.record().account_id == snapshot.record().account_id
                    && session_id(&row.digest()) == public_id
            })
            .ok_or(EpochSessionError::Rejected)?;
        if !self.sessions.delete(&expected).await.map_err(EpochSessionError::Session)? {
            return Err(EpochSessionError::Rejected);
        }
        Ok(expected.into_stored().record)
    }
    pub async fn revoke_others(&self, current: &ResolvedEpochSession<S::Snapshot>) -> EpochSessionResult<usize, S::Error, A::Error> {
        self.sessions.revoke_others(current.snapshot()).await.map_err(EpochSessionError::Session)?.ok_or(EpochSessionError::Rejected)
    }
    pub async fn purge_expired(&self) -> EpochSessionResult<usize, S::Error, A::Error> {
        self.sessions.purge_expired_bound().await.map_err(EpochSessionError::Session)
    }
}
