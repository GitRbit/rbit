//! Static output/policy hooks around ONE existing remote verification worker.
use super::{AccountId, Callback, ExternalAuthService, Flow, OriginalLinkCallback, Refusal, VerifiedIdentity};
#[derive(Debug)]
pub enum OriginalProviderCallback {
    SignIn(Callback),
    Link(OriginalLinkCallback),
}
pub(super) enum CheckError {
    Remote(Refusal),
    Unavailable,
}
impl From<Refusal> for CheckError {
    fn from(value: Refusal) -> Self {
        Self::Remote(value)
    }
}
pub(super) trait Mode {
    type Output: Send;
    fn before_exchange(service: &ExternalAuthService, flow: &Flow) -> Result<(), CheckError>;
    fn finish(service: &ExternalAuthService, flow: Flow, identity: VerifiedIdentity) -> Result<Self::Output, CheckError>;
}
pub(super) struct Legacy;
impl Mode for Legacy {
    type Output = Callback;
    fn before_exchange(_: &ExternalAuthService, _: &Flow) -> Result<(), CheckError> {
        Ok(())
    }
    fn finish(_: &ExternalAuthService, flow: Flow, identity: VerifiedIdentity) -> Result<Callback, CheckError> {
        Ok(Callback { identity, next: flow.next, link_account: flow.link_account.map(AccountId) })
    }
}
pub(super) struct Original;
impl Mode for Original {
    type Output = OriginalProviderCallback;
    fn before_exchange(service: &ExternalAuthService, flow: &Flow) -> Result<(), CheckError> {
        if flow.link_account.is_some() {
            let original =
                service.inner.original_links.validate_callback(&flow.state, &flow.provider).map_err(|_| CheckError::Unavailable)?;
            if flow.link_account != Some(original.account().0) {
                return Err(CheckError::Unavailable);
            }
        }
        Ok(())
    }
    fn finish(service: &ExternalAuthService, flow: Flow, identity: VerifiedIdentity) -> Result<OriginalProviderCallback, CheckError> {
        let callback = Callback { identity, next: flow.next, link_account: flow.link_account.map(AccountId) };
        if callback.link_account.is_some() {
            service
                .inner
                .original_links
                .verified(&flow.state, callback)
                .map(OriginalProviderCallback::Link)
                .map_err(|_| CheckError::Unavailable)
        } else {
            Ok(OriginalProviderCallback::SignIn(callback))
        }
    }
}
