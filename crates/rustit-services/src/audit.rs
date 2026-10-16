//! Security audit log: sign-ins, 2FA changes, session revocations, access tokens and
//! administrator actions.
//!
//! Milestone 1 workstream S owns this file.
//!
//! Recording never fails the operation being recorded: a write error is logged loudly
//! instead, because refusing a sign-in because the log is busy would be worse.

use std::net::IpAddr;

use rustit_core::User;
pub use rustit_storage::sqlite::audit::{AuditAction, AuditCategory, AuditEvent};
use rustit_storage::sqlite::audit::{AuditStore, NewAuditEvent};

use crate::{
    ServiceContext, ServiceError,
    admin::{ListQuery, Paged, require_admin},
};

/// Longest attempted login copied into the log from a failed sign-in.
const MAX_LOGIN_LEN: usize = 64;

/// One entry, built fluently: `AuditEntry::new(action).by(&user).from_ip(ip)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry(NewAuditEvent);

impl AuditEntry {
    pub fn new(action: AuditAction) -> Self {
        Self(NewAuditEvent { action, actor_id: None, actor_name: None, target_name: None, ip_address: None, detail: String::new() })
    }

    /// The signed-in user who acted.
    #[must_use]
    pub fn by(mut self, actor: &User) -> Self {
        self.0.actor_id = Some(actor.id());
        self.0.actor_name = Some(actor.name().to_string());
        self
    }

    /// The login someone tried to sign in as (not verified, so no account id).
    #[must_use]
    pub fn by_login(mut self, login: &str) -> Self {
        let login = login.trim();
        let end = login.char_indices().nth(MAX_LOGIN_LEN).map_or(login.len(), |(index, _)| index);
        self.0.actor_name = Some(login[..end].to_owned());
        self
    }

    /// What the action was applied to: a username, repository, token name…
    #[must_use]
    pub fn on(mut self, target: impl Into<String>) -> Self {
        self.0.target_name = Some(target.into());
        self
    }

    #[must_use]
    pub fn from_ip(mut self, ip: Option<IpAddr>) -> Self {
        self.0.ip_address = ip.map(|ip| ip.to_canonical().to_string());
        self
    }

    #[must_use]
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.0.detail = detail.into();
        self
    }
}

#[derive(Debug, Clone)]
pub struct AuditService {
    store: AuditStore,
}

impl AuditService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        let ServiceContext { database, .. } = ctx;
        Self::from_store(database.audit())
    }

    pub(crate) fn from_store(store: AuditStore) -> Self {
        Self { store }
    }

    /// Appends an entry. Errors are logged, never returned.
    pub async fn record(&self, entry: AuditEntry) {
        let AuditEntry(event) = entry;
        tracing::info!(
            action = event.action.as_str(),
            actor = event.actor_name.as_deref().unwrap_or("-"),
            target = event.target_name.as_deref().unwrap_or("-"),
            ip = event.ip_address.as_deref().unwrap_or("-"),
            "audit"
        );
        if let Err(error) = self.store.append(&event).await {
            tracing::error!(%error, action = event.action.as_str(), "failed to write audit event");
        }
    }

    /// One page of the log, newest first, for administrators. `query.search` matches the
    /// actor, the target or the client address.
    pub async fn list(&self, actor: &User, category: Option<AuditCategory>, query: &ListQuery) -> Result<Paged<AuditEvent>, ServiceError> {
        require_admin(actor)?;
        Ok(Paged::new(self.store.list(category, query.filter()).await?, query))
    }

    /// The user's own most recent security events.
    pub async fn recent_for(&self, user: &User, limit: u8) -> Result<Vec<AuditEvent>, ServiceError> {
        Ok(self.store.list_for_actor(user.id(), i64::from(limit)).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_cap_attempted_logins_and_canonicalise_addresses() {
        let AuditEntry(event) = AuditEntry::new(AuditAction::SignInFailed)
            .by_login(&format!("  {}  ", "x".repeat(200)))
            .from_ip(Some("::ffff:192.0.2.1".parse().unwrap()))
            .detail("wrong password");
        assert_eq!(event.actor_name.unwrap().len(), MAX_LOGIN_LEN);
        assert_eq!(event.actor_id, None);
        assert_eq!(event.ip_address.as_deref(), Some("192.0.2.1"));
        assert_eq!(event.detail, "wrong password");
    }
}
