//! Infrastructure effects/evidence bridge; session attribution and ownership are identity-owned.

use rbit_auth::{SessionToken, TokenDigest};
use rbit_core::AccountId;
use rbit_identity::sessions::effects::{
    EnforcementError, IdentityPasswordChangeGuard, PasswordChangeEvidence, PasswordChangeEvidenceSource, SessionEffectsError,
    SessionEffectsReport, SessionFinalizer, SessionMutationContext, SessionMutationPermit, SessionRevocationEffects,
};
use rbit_storage::{AccountStore, SessionStore};
use tokio::sync::broadcast::{self, error::TryRecvError};

use super::AccountService;
use crate::{
    DomainEvent, ServiceError,
    audit::{AuditAction, AuditEntry, AuditService},
};

#[derive(Debug, Clone)]
pub(crate) struct PasswordSessionEffects {
    accounts: AccountStore,
    sessions: SessionStore,
    audit: AuditService,
}
impl PasswordSessionEffects {
    pub(super) fn new(accounts: AccountStore, sessions: SessionStore, audit: AuditService) -> Self {
        Self { accounts, sessions, audit }
    }
}
impl SessionRevocationEffects for PasswordSessionEffects {
    type Error = ServiceError;
    async fn revoke(&self, account: AccountId, keep: Option<TokenDigest>) -> Result<(), Self::Error> {
        let removed = self.sessions.delete_for_account(account, keep).await?;
        let mut entry = AuditEntry::new(AuditAction::SessionsRevokedAfterPasswordChange)
            .detail(format!("{removed} session{} ended", if removed == 1 { "" } else { "s" }));
        if let Ok(Some(user)) = self.accounts.find_user(account).await {
            entry = entry.by(&user);
        }
        self.audit.record(entry).await;
        Ok(())
    }
    fn failed(&self, account: AccountId, keep: Option<TokenDigest>, error: &Self::Error) {
        if keep.is_some() {
            tracing::error!(%error, user = account.0, "failed to end other sessions after a password change");
        } else {
            tracing::error!(%error, user = account.0, "failed to end sessions after a password change");
        }
    }
    fn unproved(&self, account: AccountId, reason: &'static str) {
        tracing::error!(user = account.0, reason, "session revocation completion is unproved");
    }
}

struct Evidence(broadcast::Receiver<DomainEvent>);
impl PasswordChangeEvidenceSource for Evidence {
    fn try_next(&mut self) -> PasswordChangeEvidence {
        match self.0.try_recv() {
            Ok(DomainEvent::PasswordChanged { user }) => PasswordChangeEvidence::Changed(user),
            Ok(_) => PasswordChangeEvidence::Other,
            Err(TryRecvError::Lagged(_)) => PasswordChangeEvidence::Lagged,
            Err(TryRecvError::Empty) => PasswordChangeEvidence::Empty,
            Err(TryRecvError::Closed) => PasswordChangeEvidence::Closed,
        }
    }
}

#[derive(Debug)]
pub struct PasswordChangeGuard {
    inner: Option<IdentityPasswordChangeGuard<PasswordSessionEffects, Evidence>>,
}
impl PasswordChangeGuard {
    pub async fn finish(self, succeeded: bool) {
        if let Some(guard) = self.inner {
            guard.finish(succeeded).await;
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AccountSessionFinalizer(SessionFinalizer<PasswordSessionEffects>);
impl AccountSessionFinalizer {
    pub(crate) async fn password_changed(&self, context: &SessionMutationContext, account: AccountId) -> Result<(), ServiceError> {
        self.0.committed(context, account, false).await.map_err(map_enforcement)
    }
    pub(crate) async fn preserving_current(&self, context: &SessionMutationContext, account: AccountId) -> Result<(), ServiceError> {
        self.0.committed(context, account, true).await.map_err(map_enforcement)
    }
}

impl AccountService {
    pub fn guard_password_change(&self, user: AccountId, current: &SessionToken) -> PasswordChangeGuard {
        let inner = self.password_changes.guard(user, current.digest(), || Evidence(self.events.subscribe()));
        if inner.is_err() {
            tracing::warn!(user = user.0, "password change guard refused during shutdown");
        }
        PasswordChangeGuard { inner: inner.ok() }
    }
    pub(crate) fn reserve_password_change(&self) -> Result<SessionMutationPermit, ServiceError> {
        self.password_changes.reserve().map_err(map_ownership)
    }
    pub(crate) fn password_change_finalizer(&self) -> AccountSessionFinalizer {
        AccountSessionFinalizer(self.password_changes.finalizer())
    }
    pub(crate) fn close_session_effects(&self) {
        self.password_changes.close();
    }
    pub fn session_effect_report(&self) -> SessionEffectsReport {
        self.password_changes.report()
    }
    pub async fn shutdown_session_effects(&self) {
        let report = self.password_changes.shutdown().await;
        if report.abnormal > 0 {
            tracing::error!(?report, "session finalization has abnormal or unproved outcomes");
        } else if report.returned_errors > 0 {
            tracing::warn!(?report, "session finalization drained with operation errors");
        }
    }
}

pub(crate) fn map_ownership(error: SessionEffectsError) -> ServiceError {
    match error {
        SessionEffectsError::Closed => ServiceError::Unavailable { retry_after_seconds: 1 },
        other => ServiceError::internal(other),
    }
}
fn map_enforcement(error: EnforcementError<ServiceError>) -> ServiceError {
    match error {
        EnforcementError::Effects(error) => error,
        EnforcementError::Ownership(error) => map_ownership(error),
    }
}
