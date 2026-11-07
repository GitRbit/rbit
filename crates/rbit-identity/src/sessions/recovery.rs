//! Bounded policy over actual durable decisions/effects; concrete owner supplies serialization.
use rbit_identity_models::sessions::{
    PendingSessionRevocation, SessionRevocationAcknowledgment as Ack, SessionRevocationRepository, SessionRevocationState,
};
use std::num::NonZeroU8;
#[derive(Debug)]
pub enum SessionRecoveryError<E> {
    Store(E),
    InvalidBatch,
    InvalidState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionRecoveryDisposition {
    Completed,
    AlreadyResolved,
    Deferred,
    Superseded,
    Unsupported,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionRecoveryBatch {
    pub cursor: i64,
    pub visited: usize,
    pub completed: usize,
    pub already_resolved: usize,
    pub deferred: usize,
    pub superseded: usize,
    pub unsupported: usize,
}
pub struct SessionRecoveryApplication<'a, R> {
    repository: &'a R,
}
impl<R> std::fmt::Debug for SessionRecoveryApplication<'_, R> {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.debug_struct("SessionRecoveryApplication").finish_non_exhaustive()
    }
}
impl<'a, R: SessionRevocationRepository> SessionRecoveryApplication<'a, R> {
    pub fn new(repository: &'a R) -> Self {
        Self { repository }
    }
    pub async fn batch(&self, after: i64, limit: NonZeroU8) -> Result<SessionRecoveryBatch, SessionRecoveryError<R::Error>> {
        if after < 0 {
            return Err(SessionRecoveryError::InvalidBatch);
        }
        let rows = self.repository.pending(after, limit).await.map_err(SessionRecoveryError::Store)?;
        if rows.len() > usize::from(limit.get()) {
            return Err(SessionRecoveryError::InvalidBatch);
        }
        let mut result = SessionRecoveryBatch { cursor: after, ..SessionRecoveryBatch::default() };
        for pending in rows {
            if pending.id() <= result.cursor {
                return Err(SessionRecoveryError::InvalidBatch);
            }
            result.cursor = pending.id();
            result.visited += 1;
            match self.reconcile(&pending).await? {
                SessionRecoveryDisposition::Completed => result.completed += 1,
                SessionRecoveryDisposition::AlreadyResolved => result.already_resolved += 1,
                SessionRecoveryDisposition::Deferred => result.deferred += 1,
                SessionRecoveryDisposition::Superseded => result.superseded += 1,
                SessionRecoveryDisposition::Unsupported => result.unsupported += 1,
            }
        }
        Ok(result)
    }
    pub async fn reconcile(
        &self,
        pending: &PendingSessionRevocation,
    ) -> Result<SessionRecoveryDisposition, SessionRecoveryError<R::Error>> {
        match self.repository.state(pending).await.map_err(SessionRecoveryError::Store)? {
            SessionRevocationState::Resolved => Ok(if pending.request().is_some() {
                SessionRecoveryDisposition::AlreadyResolved
            } else {
                SessionRecoveryDisposition::Unsupported
            }),
            SessionRevocationState::NewerPending => Ok(SessionRecoveryDisposition::Deferred),
            SessionRevocationState::Invalid => Err(SessionRecoveryError::InvalidState),
            SessionRevocationState::NewerCompleted(witness) => {
                let result = self.repository.supersede(pending, &witness).await.map_err(SessionRecoveryError::Store)?;
                Ok(acknowledged(result, SessionRecoveryDisposition::Superseded))
            }
            SessionRevocationState::CurrentPending => {
                let Some(request) = pending.request() else { return Ok(SessionRecoveryDisposition::Unsupported) };
                let applied = self.repository.apply(request).await.map_err(SessionRecoveryError::Store)?;
                let result = self.repository.acknowledge(&request, &applied).await.map_err(SessionRecoveryError::Store)?;
                Ok(acknowledged(result, SessionRecoveryDisposition::Completed))
            }
        }
    }
}
fn acknowledged(ack: Ack, applied: SessionRecoveryDisposition) -> SessionRecoveryDisposition {
    match ack {
        Ack::Applied => applied,
        Ack::AlreadyResolved => SessionRecoveryDisposition::AlreadyResolved,
        Ack::Changed => SessionRecoveryDisposition::Deferred,
    }
}

#[cfg(test)]
mod diagnostics {
    use super::SessionRecoveryApplication;

    #[test]
    fn formatting_requires_no_repository_diagnostics() {
        struct OpaqueRepository;
        let repository = OpaqueRepository;
        let application = SessionRecoveryApplication { repository: &repository };
        assert_eq!(format!("{application:?}"), "SessionRecoveryApplication { .. }");
    }

    #[test]
    fn formatting_never_traverses_repository_state() {
        struct PrivateRepository;
        impl std::fmt::Debug for PrivateRepository {
            fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                panic!("repository state must remain opaque");
            }
        }
        let repository = PrivateRepository;
        let application = SessionRecoveryApplication { repository: &repository };
        assert_eq!(format!("{application:?}"), "SessionRecoveryApplication { .. }");
    }
}
