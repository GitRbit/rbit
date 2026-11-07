//! Synchronous attribution and tracked revocation over narrow injected effects.

use super::{SessionEffectScope, SessionEffectsError, SessionEffectsReport, SessionMutationContext, SessionMutationPermit};
use super::{ledger::PasswordChangeLedger, scope::Statistics};
use crate::TokenDigest;
use rbit_contracts::AccountId;
use std::{future::Future, sync::Arc};

pub trait SessionRevocationEffects: Clone + Send + Sync + 'static {
    type Error: std::error::Error + Send + Sync + 'static;
    /// Completes authoritative deletion and its audit before returning.
    fn revoke(&self, account: AccountId, keep: Option<TokenDigest>) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn failed(&self, account: AccountId, keep: Option<TokenDigest>, error: &Self::Error);
    fn unproved(&self, account: AccountId, reason: &'static str);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordChangeEvidence {
    Changed(AccountId),
    Other,
    Lagged,
    Empty,
    Closed,
}
pub trait PasswordChangeEvidenceSource: Send {
    fn try_next(&mut self) -> PasswordChangeEvidence;
}

#[derive(Debug)]
pub enum EnforcementError<E> {
    Effects(E),
    Ownership(SessionEffectsError),
}

#[derive(Debug, Clone)]
pub struct PasswordSessionEnforcement<F> {
    scope: SessionEffectScope,
    finalizer: SessionFinalizer<F>,
}

/// Narrow capability for an already tracked credential finalization. No scope owner,
/// task registry or broad service handle is captured by its worker.
#[derive(Debug, Clone)]
pub struct SessionFinalizer<F> {
    ledger: Arc<PasswordChangeLedger>,
    effects: F,
    authority: Arc<Statistics>,
}

impl<F: SessionRevocationEffects> PasswordSessionEnforcement<F> {
    pub fn new(effects: F) -> Self {
        let scope = SessionEffectScope::new();
        let finalizer = SessionFinalizer { ledger: Arc::default(), effects, authority: scope.authority() };
        Self { scope, finalizer }
    }
    pub fn reserve(&self) -> Result<SessionMutationPermit, SessionEffectsError> {
        self.scope.reserve()
    }
    pub fn finalizer(&self) -> SessionFinalizer<F> {
        self.finalizer.clone()
    }
    pub fn close(&self) {
        self.scope.close();
    }
    pub fn report(&self) -> SessionEffectsReport {
        self.scope.report()
    }
    pub async fn shutdown(&self) -> SessionEffectsReport {
        self.scope.shutdown().await
    }

    pub fn guard<E: PasswordChangeEvidenceSource>(
        &self,
        account: AccountId,
        current: TokenDigest,
        subscribe: impl FnOnce() -> E,
    ) -> Result<IdentityPasswordChangeGuard<F, E>, SessionEffectsError> {
        let permit = self.scope.reserve()?;
        self.finalizer.ledger.begin(account);
        let mut guard = IdentityPasswordChangeGuard {
            finalizer: self.finalizer(),
            account,
            current,
            evidence: None,
            permit: Some(permit),
            active: true,
        };
        guard.evidence = Some(subscribe());
        Ok(guard)
    }
}

impl<F: SessionRevocationEffects> SessionFinalizer<F> {
    /// Validate an already-admitted context without capturing its scope controller.
    pub fn validate_context(&self, context: &SessionMutationContext) -> Result<(), SessionEffectsError> {
        if context.belongs_to(&self.authority) { Ok(()) } else { Err(SessionEffectsError::WrongScope) }
    }

    pub async fn committed(
        &self,
        context: &SessionMutationContext,
        account: AccountId,
        preserve_current: bool,
    ) -> Result<(), EnforcementError<F::Error>> {
        self.validate_context(context).map_err(EnforcementError::Ownership)?;
        if !preserve_current || self.ledger.on_event(account) {
            self.effects.revoke(account, None).await.map_err(EnforcementError::Effects)?;
        }
        Ok(())
    }
}

pub struct IdentityPasswordChangeGuard<F: SessionRevocationEffects, E: PasswordChangeEvidenceSource> {
    finalizer: SessionFinalizer<F>,
    account: AccountId,
    current: TokenDigest,
    evidence: Option<E>,
    permit: Option<SessionMutationPermit>,
    active: bool,
}
impl<F: SessionRevocationEffects, E: PasswordChangeEvidenceSource> std::fmt::Debug for IdentityPasswordChangeGuard<F, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentityPasswordChangeGuard").field("account", &self.account).finish_non_exhaustive()
    }
}

impl<F: SessionRevocationEffects, E: PasswordChangeEvidenceSource> IdentityPasswordChangeGuard<F, E> {
    pub async fn finish(mut self, succeeded: bool) {
        let Some(evidence) = self.evidence.as_mut() else { return };
        let mut changed = false;
        loop {
            match evidence.try_next() {
                PasswordChangeEvidence::Changed(account) if account == self.account => changed = true,
                PasswordChangeEvidence::Changed(_) | PasswordChangeEvidence::Other => {}
                PasswordChangeEvidence::Lagged => changed = true,
                PasswordChangeEvidence::Empty | PasswordChangeEvidence::Closed => break,
            }
        }
        let outcome = self.finalizer.ledger.finish(self.account, changed && succeeded);
        self.active = false;
        self.evidence = None;
        let keep = if outcome.end_all {
            None
        } else if outcome.end_others {
            Some(self.current)
        } else {
            return;
        };
        let Some(permit) = self.permit.take() else { return };
        if let Some(task) = dispatch(permit, &self.finalizer.effects, self.account, keep)
            && task.await.is_err()
        {
            self.finalizer.effects.unproved(self.account, "session revocation worker stopped abnormally");
        }
    }
}

impl<F: SessionRevocationEffects, E: PasswordChangeEvidenceSource> Drop for IdentityPasswordChangeGuard<F, E> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        self.evidence = None;
        let outcome = self.finalizer.ledger.finish(self.account, false);
        if outcome.end_all
            && let Some(permit) = self.permit.take()
        {
            drop(dispatch(permit, &self.finalizer.effects, self.account, None));
        }
    }
}

fn dispatch<F: SessionRevocationEffects>(
    permit: SessionMutationPermit,
    effects: &F,
    account: AccountId,
    keep: Option<TokenDigest>,
) -> Option<tokio::task::JoinHandle<Result<(), F::Error>>> {
    let worker = effects.clone();
    if let Ok(task) = permit.submit(move |_context| async move {
        let result = worker.revoke(account, keep).await;
        if let Err(error) = &result {
            worker.failed(account, keep, error);
        }
        result
    }) {
        Some(task)
    } else {
        effects.unproved(account, "session revocation could not be submitted");
        None
    }
}
