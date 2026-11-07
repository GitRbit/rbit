//! One narrow recovery owner shared by all explicit finalizers; no scope/controller cycle.
mod backend;
use super::{
    AccountService,
    effects::{PasswordSessionEffects, map_ownership},
};
use crate::{ServiceError, credentials};
use backend::RecoveryLease;
use rbit_identity::sessions::{
    SessionRecoveryApplication, SessionRecoveryBatch, SessionRecoveryDisposition, SessionRecoveryError,
    effects::{SessionFinalizer, SessionMutationContext},
};
use rbit_storage::{
    CredentialRecoveryGate, CredentialRecoveryPermit, Database, EpochSessionStore, KvStore, SessionStore, StorageError,
    sqlite::credential_revocations::{CredentialRevocationStore, RevocationIntent},
};
use std::{num::NonZeroU8, sync::Arc};

/// Explicit persistence composition at Services construction; not a service locator.
pub(crate) struct AccountSessionPersistence {
    pub(super) sessions: SessionStore,
    pub(super) revocations: CredentialRevocationStore,
    pub(super) recovery_gate: CredentialRecoveryGate,
}
pub(crate) fn session_persistence(database: &Database, kv: &KvStore) -> AccountSessionPersistence {
    AccountSessionPersistence {
        sessions: kv.sessions(),
        revocations: database.credential_revocations(),
        recovery_gate: kv.credential_recovery_gate(),
    }
}

#[derive(Debug)]
pub(super) struct EpochSessionRecovery {
    revocations: CredentialRevocationStore,
    sessions: EpochSessionStore,
    serial: CredentialRecoveryGate,
}
impl EpochSessionRecovery {
    pub fn new(revocations: CredentialRevocationStore, sessions: EpochSessionStore, serial: CredentialRecoveryGate) -> Self {
        Self { revocations, sessions, serial }
    }
    fn admit(&self) -> Result<CredentialRecoveryPermit, ServiceError> {
        self.serial.try_acquire().ok_or(ServiceError::Unavailable { retry_after_seconds: 1 })
    }
    async fn batch(&self, guard: CredentialRecoveryPermit, after: i64, limit: NonZeroU8) -> Result<SessionRecoveryBatch, ServiceError> {
        let backend = RecoveryLease::new(self, guard).await?;
        SessionRecoveryApplication::new(&backend).batch(after, limit).await.map_err(map)
    }
    async fn exact(&self, guard: CredentialRecoveryPermit, intent: &RevocationIntent) -> Result<SessionRecoveryDisposition, ServiceError> {
        let backend = RecoveryLease::new(self, guard).await?;
        let Some(pending) = self.revocations.find(intent).await? else { return Ok(SessionRecoveryDisposition::AlreadyResolved) };
        SessionRecoveryApplication::new(&backend).reconcile(&pending).await.map_err(map)
    }
}

/// Narrow capability for an already-counted credential worker; owns no controller.
#[derive(Clone)]
pub struct OriginalCredentialFinalizer {
    recovery: Arc<EpochSessionRecovery>,
    authority: SessionFinalizer<PasswordSessionEffects>,
}
/// Exclusive bounded-operation slot held across credential commit and cleanup.
/// It borrows the already-admitted context and cannot outlive that ownership.
pub struct OriginalCredentialMutation<'a> {
    recovery: Arc<EpochSessionRecovery>,
    serial: CredentialRecoveryPermit,
    context: &'a SessionMutationContext,
    authority: SessionFinalizer<PasswordSessionEffects>,
}
impl OriginalCredentialFinalizer {
    /// Call BEFORE the SQL credential transaction, after the required validation,
    /// authentication and password-work ordering. Busy means no SQL commit yet.
    pub fn begin<'a>(&self, context: &'a SessionMutationContext) -> Result<OriginalCredentialMutation<'a>, ServiceError> {
        self.authority.validate_context(context).map_err(map_ownership)?;
        let serial = self.recovery.admit()?;
        Ok(OriginalCredentialMutation { recovery: Arc::clone(&self.recovery), serial, context, authority: self.authority.clone() })
    }
    /// Compatibility for already-committed durable intents: Busy leaves pending
    /// recovery work, never an assertion that the credential commit was rolled back.
    pub async fn committed(&self, context: &SessionMutationContext, intent: &RevocationIntent) -> Result<(), ServiceError> {
        self.begin(context)?.committed(intent).await
    }
}
impl OriginalCredentialMutation<'_> {
    pub async fn committed(self, intent: &RevocationIntent) -> Result<(), ServiceError> {
        self.authority.validate_context(self.context).map_err(map_ownership)?;
        match self.recovery.exact(self.serial, intent).await? {
            SessionRecoveryDisposition::Completed
            | SessionRecoveryDisposition::AlreadyResolved
            | SessionRecoveryDisposition::Superseded => Ok(()),
            // Stopped-upgrade retirement is not this worker's new committed cleanup.
            SessionRecoveryDisposition::Retired | SessionRecoveryDisposition::Deferred | SessionRecoveryDisposition::Unsupported => {
                Err(ServiceError::Unavailable { retry_after_seconds: 1 })
            }
        }
    }
}

impl AccountService {
    /// Explicit inactive capability: callers must commit the persisted decision in
    /// the SAME credential transaction. Production credential services remain legacy.
    pub fn original_credential_finalizer(&self) -> OriginalCredentialFinalizer {
        OriginalCredentialFinalizer { recovery: Arc::clone(&self.original_recovery), authority: self.password_changes.finalizer() }
    }
    /// One bounded sweep page. Cursor is not a readiness proof; restart from zero
    /// after finishing a sweep so deferred older IDs can observe newer acknowledgments.
    pub fn recover_original_session_batch(
        &self,
        after: i64,
        limit: NonZeroU8,
    ) -> impl Future<Output = Result<SessionRecoveryBatch, ServiceError>> {
        let admission = self.reserve_password_change().and_then(|ownership| self.original_recovery.admit().map(|guard| (ownership, guard)));
        let recovery = Arc::clone(&self.original_recovery);
        async move {
            let (ownership, guard) = admission?;
            credentials::finish_value(ownership, move |context| async move {
                let _ownership = context;
                recovery.batch(guard, after, limit).await
            })
            .await
        }
    }
    /// Explicit exact-intent recovery, not the password-changing worker itself.
    pub fn recover_original_session_intent(
        &self,
        intent: RevocationIntent,
    ) -> impl Future<Output = Result<SessionRecoveryDisposition, ServiceError>> {
        let admission = self.reserve_password_change().and_then(|ownership| self.original_recovery.admit().map(|guard| (ownership, guard)));
        let recovery = Arc::clone(&self.original_recovery);
        async move {
            let (ownership, guard) = admission?;
            credentials::finish_value(ownership, move |context| async move {
                let _ownership = context;
                recovery.exact(guard, &intent).await
            })
            .await
        }
    }
}
fn map(error: SessionRecoveryError<StorageError>) -> ServiceError {
    match error {
        SessionRecoveryError::Store(error) => error.into(),
        SessionRecoveryError::InvalidBatch | SessionRecoveryError::InvalidState => {
            ServiceError::internal(std::io::Error::other("credential session recovery state is invalid"))
        }
    }
}

impl std::fmt::Debug for OriginalCredentialFinalizer {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("OriginalCredentialFinalizer").finish_non_exhaustive()
    }
}
impl std::fmt::Debug for OriginalCredentialMutation<'_> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("OriginalCredentialMutation").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
