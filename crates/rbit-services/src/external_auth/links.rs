//! Inactive manual linking requires the original authenticated session, not a bare ID.
mod owner;
use super::{
    Callback, CallbackParams, ClientInfo, ExternalAuthService, Flow, LinkConflict, Mutex, NewExternalIdentity, OriginalProviderCallback,
    ProviderConfig, ServiceError, Services, Timestamp,
};
use crate::accounts::OriginalResolvedSession;
use crate::audit::{AuditAction, AuditEntry};
pub(super) use owner::OriginalLinks;
use rbit_identity::external::{completion, linked_detail};
use rbit_identity_models::sessions::SessionPromotion;
use rbit_storage::EpochSessionStore;
use std::sync::Weak;

/// Minted only after callback verification against a retained original link flow.
/// No public constructor/Serde, and loss on restart requires starting a fresh flow.
///
/// ```compile_fail
/// use rbit_services::external_auth::OriginalLinkCallback;
/// fn encode(value: &OriginalLinkCallback) { serde_json::to_string(value).unwrap(); }
/// ```
pub struct OriginalLinkCallback {
    callback: Callback,
    nonce: String,
    spec: SessionPromotion,
    owner: Weak<Mutex<owner::State>>,
}
impl std::fmt::Debug for OriginalLinkCallback {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("OriginalLinkCallback").finish_non_exhaustive()
    }
}
fn unavailable() -> ServiceError {
    ServiceError::Unavailable { retry_after_seconds: 1 }
}
fn capture(current: &OriginalResolvedSession) -> Result<SessionPromotion, ServiceError> {
    let spec = current.snapshot().promotion_spec().ok_or_else(unavailable)?;
    if spec.account() != current.user().user.id() {
        return Err(ServiceError::Forbidden);
    }
    Ok(spec)
}
impl ExternalAuthService {
    /// Explicit original counterpart; callers retain the existing link-CSRF check.
    /// Reservation precedes state/PKCE/discovery work and never creates legacy authority.
    pub async fn begin_original_link(
        &self,
        provider: &str,
        next: &str,
        current: &OriginalResolvedSession,
    ) -> Result<(String, String), ServiceError> {
        let reservation = self.inner.original_links.reserve()?;
        let spec = capture(current)?;
        let actual = self.inner.original_links.sessions.get_live(spec.digest()).await?.ok_or_else(unavailable)?;
        if actual.promotion_spec() != Some(spec) {
            return Err(unavailable());
        }
        let flow = Flow::new(provider, next, Some(spec.account().0), Timestamp::now().seconds());
        let response = self.begin_flow(&flow).await?;
        reservation.publish(&flow, spec)?;
        Ok(response)
    }
    /// Existing cookie/state/PKCE/provider verification remains the only remote verifier.
    /// A legacy sealed bare-ID flow has no retained context and refuses before exchange.
    pub async fn original_link_callback(
        &self,
        provider: &str,
        cookie: Option<&str>,
        params: CallbackParams<'_>,
        client: &ClientInfo,
    ) -> Result<OriginalLinkCallback, ServiceError> {
        match self.callback_original(provider, cookie, params, client).await? {
            OriginalProviderCallback::Link(callback) => Ok(callback),
            OriginalProviderCallback::SignIn(_) => Err(ServiceError::Forbidden),
        }
    }
}
impl Services {
    /// No context is consumed before owned dispatch. A current freshly reauthenticated
    /// session cannot replace the binding captured before leaving for the provider.
    pub fn complete_original_link<'a>(
        &'a self,
        callback: OriginalLinkCallback,
        current: &'a OriginalResolvedSession,
        client: &'a ClientInfo,
    ) -> impl Future<Output = Result<(), ServiceError>> + Send + 'a {
        let ownership = self.accounts.reserve_password_change();
        let client = client.clone();
        let original = capture(current);
        let links = self.external_auth.inner.original_links.clone();
        let identities = self.external_auth.inner.database.external_identities();
        let accounts = self.external_auth.inner.database.accounts();
        let audit = self.external_auth.inner.audit.clone();
        let config = self.external_auth.provider(&callback.callback.identity.provider_id).map(|provider| provider.config.clone());
        async move {
            let ownership = ownership?;
            if original? != callback.spec {
                return Err(ServiceError::Forbidden);
            }
            let config = config.ok_or(ServiceError::NotFound)?;
            crate::credentials::finish_value(ownership, move |context| async move {
                let _owned = context;
                let actual = links.sessions.get_live(callback.spec.digest()).await?.ok_or_else(unavailable)?;
                if actual.promotion_spec() != Some(callback.spec) {
                    return Err(unavailable());
                }
                let callback = links.claim(callback)?;
                let account = actual.record().account_id;
                let linked = identities.find(&callback.identity.provider_id, &callback.identity.subject).await?;
                let user = accounts.find_session_user_at_epoch(account, false, actual.epoch()).await?.ok_or_else(unavailable)?.user;
                if let Some(linked) = linked {
                    return if linked.account_id == account { Ok(()) } else { Err(taken(&config)) };
                }
                let identity = NewExternalIdentity {
                    provider_id: &callback.identity.provider_id,
                    subject: &callback.identity.subject,
                    email: callback.identity.email.as_deref(),
                    username: callback.identity.username.as_deref(),
                };
                match identities.link_at_epoch(account, identity, Timestamp::now(), actual.epoch()).await? {
                    rbit_storage::sqlite::external_identities::GuardedIdentityLink::Linked(_) => {
                        let detail = linked_detail(&config.name, &callback.identity, "");
                        audit
                            .record(AuditEntry::new(AuditAction::ExternalIdentityLinked).by(&user).from_ip(client.ip).detail(detail))
                            .await;
                        Ok(())
                    }
                    rbit_storage::sqlite::external_identities::GuardedIdentityLink::Conflict(conflict) => {
                        Err(conflict_error(conflict, &config))
                    }
                    rbit_storage::sqlite::external_identities::GuardedIdentityLink::Unavailable => Err(unavailable()),
                }
            })
            .await
        }
    }
}
fn taken(config: &ProviderConfig) -> ServiceError {
    ServiceError::Invalid(completion::already_linked(&config.name))
}
fn conflict_error(conflict: LinkConflict, config: &ProviderConfig) -> ServiceError {
    let conflict = match conflict {
        LinkConflict::IdentityTaken => completion::LinkConflict::IdentityTaken,
        LinkConflict::ProviderTaken => completion::LinkConflict::ProviderTaken,
    };
    ServiceError::Invalid(completion::link_refused(conflict, &config.name))
}

#[cfg(test)]
mod tests;
