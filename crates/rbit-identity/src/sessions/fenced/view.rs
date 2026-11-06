use super::super::{EpochSessionSnapshot, SessionUser};
use crate::SessionToken;

/// Account flags and original binding captured by the epoch-aware resolver.
/// No public constructor: attaching a mutable User is not session authority.
#[derive(Clone)]
pub struct ResolvedEpochSession<S> {
    user: SessionUser,
    snapshot: S,
}
impl<S: EpochSessionSnapshot> ResolvedEpochSession<S> {
    pub(super) fn new(user: SessionUser, snapshot: S) -> Self {
        Self { user, snapshot }
    }
    pub fn user(&self) -> &SessionUser {
        &self.user
    }
    pub fn snapshot(&self) -> &S {
        &self.snapshot
    }
    pub fn into_parts(self) -> (SessionUser, S) {
        (self.user, self.snapshot)
    }
}
impl<S> std::fmt::Debug for ResolvedEpochSession<S> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("ResolvedEpochSession").finish_non_exhaustive()
    }
}
pub struct IssuedEpochSession<S> {
    pub token: SessionToken,
    pub session: ResolvedEpochSession<S>,
}
impl<S> std::fmt::Debug for IssuedEpochSession<S> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("IssuedEpochSession").finish_non_exhaustive()
    }
}
