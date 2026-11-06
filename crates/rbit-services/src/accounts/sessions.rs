//! Legacy session APIs delegate policy to identity and retain infrastructure audit mapping.

use rbit_auth::SessionToken;
use rbit_core::User;
use rbit_identity::sessions::{SessionApplication, SessionError, SessionPolicy};
use rbit_storage::{AccountStore, SessionRecord, SessionStore, StorageError};

use super::{AccountService, ActiveSession, ClientInfo, SessionUser};
use crate::{
    ServiceError,
    audit::{AuditAction, AuditEntry},
};

impl AccountService {
    fn session_application(&self) -> SessionApplication<'_, SessionStore, AccountStore> {
        SessionApplication::new(&self.sessions, &self.accounts, self.session_application_policy())
    }

    pub async fn start_session(&self, user: &User) -> Result<SessionToken, ServiceError> {
        self.start_session_for(user, &ClientInfo::default()).await
    }
    pub async fn start_session_for(&self, user: &User, client: &ClientInfo) -> Result<SessionToken, ServiceError> {
        self.session_application().start(user, client).await.map_err(map)
    }
    pub async fn resume_session(&self, token: &SessionToken) -> Result<Option<SessionUser>, ServiceError> {
        self.session_application().resume(token).await.map_err(map)
    }
    pub async fn end_session(&self, token: &SessionToken) -> Result<(), ServiceError> {
        self.session_application().end(token).await.map_err(map)
    }
    pub async fn sessions(&self, user: &User, current: Option<&SessionToken>) -> Result<Vec<ActiveSession>, ServiceError> {
        self.session_application().list(user, current).await.map_err(map)
    }
    pub async fn revoke_session(&self, user: &User, id: &str, client: &ClientInfo) -> Result<(), ServiceError> {
        let record = self.session_application().revoke(user, id).await.map_err(map)?;
        self.audit.record(AuditEntry::new(AuditAction::SessionRevoked).by(user).from_ip(client.ip).detail(describe_session(&record))).await;
        Ok(())
    }
    pub async fn revoke_other_sessions(&self, user: &User, current: &SessionToken, client: &ClientInfo) -> Result<usize, ServiceError> {
        let removed = self.session_application().revoke_others(user.id(), current).await.map_err(map)?;
        let detail = format!("{removed} session{} ended", if removed == 1 { "" } else { "s" });
        self.audit.record(AuditEntry::new(AuditAction::OtherSessionsRevoked).by(user).from_ip(client.ip).detail(detail)).await;
        Ok(removed)
    }
    pub fn session_lifetime_seconds(&self) -> i64 {
        self.session_application_policy().lifetime_seconds()
    }
    pub(super) fn session_application_policy(&self) -> SessionPolicy {
        SessionPolicy {
            lifetime_days: self.config.security.session_lifetime_days,
            max_age_days: self.config.security.session_max_age_days,
            notifications: self.config.issues.notifications,
        }
    }
}

fn map(error: SessionError<StorageError, StorageError>) -> ServiceError {
    match error {
        SessionError::Session(error) | SessionError::Account(error) => error.into(),
        SessionError::NotFound => ServiceError::NotFound,
    }
}

pub(super) fn describe_session(record: &SessionRecord) -> String {
    let agent: String = record.user_agent.chars().take(80).collect();
    match &record.ip_address {
        Some(ip) => format!("{agent} from {ip}").trim().to_owned(),
        None => agent,
    }
}
