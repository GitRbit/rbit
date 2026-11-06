//! Explicit inactive session API; no production route or startup selects it yet.
use super::{AccountService, ClientInfo, OriginalProofReceipt, epoch_repository::OwnedEpochSessions, sessions::describe_session};
use crate::{
    ServiceError,
    audit::{AuditAction, AuditEntry},
};
use rbit_identity::{
    SessionToken,
    sessions::{ActiveSession, EpochSessionApplication, EpochSessionError, IssuedEpochSession, ResolvedEpochSession},
};
use rbit_storage::{EpochSnapshot, StorageError};

pub type OriginalIssuedSession = IssuedEpochSession<EpochSnapshot>;
pub type OriginalResolvedSession = ResolvedEpochSession<EpochSnapshot>;

impl AccountService {
    /// Only an identity-minted original receipt can reach this alternate issuer.
    /// INITIAL, stale, suspended and current-pending origins never issue here.
    pub async fn start_original_session_for(
        &self,
        receipt: &OriginalProofReceipt,
        client: &ClientInfo,
    ) -> Result<OriginalIssuedSession, ServiceError> {
        let sessions = OwnedEpochSessions::new(self);
        EpochSessionApplication::new(&sessions, &self.accounts, self.session_application_policy()).issue(receipt, client).await.map_err(map)
    }
    pub async fn resume_original_session(&self, token: &SessionToken) -> Result<Option<OriginalResolvedSession>, ServiceError> {
        let sessions = OwnedEpochSessions::new(self);
        EpochSessionApplication::new(&sessions, &self.accounts, self.session_application_policy()).resume(token).await.map_err(map)
    }
    pub async fn end_original_session(&self, token: &SessionToken) -> Result<(), ServiceError> {
        let sessions = OwnedEpochSessions::new(self);
        EpochSessionApplication::new(&sessions, &self.accounts, self.session_application_policy()).end(token).await.map_err(map)
    }
    pub async fn original_sessions(&self, current: &OriginalResolvedSession) -> Result<Vec<ActiveSession>, ServiceError> {
        let sessions = OwnedEpochSessions::new(self);
        EpochSessionApplication::new(&sessions, &self.accounts, self.session_application_policy()).list(current).await.map_err(map)
    }
    pub fn revoke_original_session(
        &self,
        current: &OriginalResolvedSession,
        id: &str,
        client: &ClientInfo,
    ) -> impl Future<Output = Result<(), ServiceError>> {
        let ownership = self.reserve_password_change();
        let current = current.clone();
        let id = id.to_owned();
        let ip = client.ip;
        let store = self.sessions.epoch_sidecar();
        let accounts = self.accounts.clone();
        let policy = self.session_application_policy();
        let audit = self.audit.clone();
        async move {
            crate::credentials::finish_value(ownership?, move |context| async move {
                let _ownership = context;
                let record = EpochSessionApplication::new(&store, &accounts, policy).revoke(&current, &id).await.map_err(map_storage)?;
                audit
                    .record(
                        AuditEntry::new(AuditAction::SessionRevoked).by(&current.user().user).from_ip(ip).detail(describe_session(&record)),
                    )
                    .await;
                Ok(())
            })
            .await
        }
    }
    pub fn revoke_other_original_sessions(
        &self,
        current: &OriginalResolvedSession,
        client: &ClientInfo,
    ) -> impl Future<Output = Result<usize, ServiceError>> {
        let ownership = self.reserve_password_change();
        let current = current.clone();
        let ip = client.ip;
        let store = self.sessions.epoch_sidecar();
        let accounts = self.accounts.clone();
        let policy = self.session_application_policy();
        let audit = self.audit.clone();
        async move {
            crate::credentials::finish_value(ownership?, move |context| async move {
                let _ownership = context;
                let removed = EpochSessionApplication::new(&store, &accounts, policy).revoke_others(&current).await.map_err(map_storage)?;
                let detail = format!("{removed} session{} ended", if removed == 1 { "" } else { "s" });
                audit.record(AuditEntry::new(AuditAction::OtherSessionsRevoked).by(&current.user().user).from_ip(ip).detail(detail)).await;
                Ok(removed)
            })
            .await
        }
    }
    pub async fn purge_expired_original_sessions(&self) -> Result<usize, ServiceError> {
        let sessions = OwnedEpochSessions::new(self);
        EpochSessionApplication::new(&sessions, &self.accounts, self.session_application_policy()).purge_expired().await.map_err(map)
    }
}
fn map(error: EpochSessionError<ServiceError, StorageError>) -> ServiceError {
    match error {
        EpochSessionError::Session(error) => error,
        EpochSessionError::Account(error) => error.into(),
        EpochSessionError::Rejected => ServiceError::NotFound,
        EpochSessionError::Collision => ServiceError::Unavailable { retry_after_seconds: 1 },
    }
}

fn map_storage(error: EpochSessionError<StorageError, StorageError>) -> ServiceError {
    match error {
        EpochSessionError::Session(error) | EpochSessionError::Account(error) => error.into(),
        EpochSessionError::Rejected => ServiceError::NotFound,
        EpochSessionError::Collision => ServiceError::Unavailable { retry_after_seconds: 1 },
    }
}
