//! Counted recovery ownership and narrow session-finalization capabilities.

use rbit_core::AccountId;
use rbit_identity::sessions::effects::{SessionMutationContext, SessionMutationPermit};
use rbit_storage::SessionStore;

use super::RecoveryService;
use crate::{
    ServiceError,
    accounts::effects::{AccountSessionFinalizer, map_ownership},
};

#[derive(Debug)]
pub(super) enum RecoverySessionFinalizer {
    Account(AccountSessionFinalizer),
    Standalone(SessionStore),
}
impl RecoverySessionFinalizer {
    pub(super) async fn committed(&self, context: &SessionMutationContext, account: AccountId) -> Result<(), ServiceError> {
        match self {
            Self::Account(finalizer) => finalizer.password_changed(context, account).await,
            Self::Standalone(sessions) => {
                sessions.delete_for_account(account, None).await?;
                Ok(())
            }
        }
    }
}

impl RecoveryService {
    pub(super) fn reserve_password_change(&self) -> Result<SessionMutationPermit, ServiceError> {
        if let Some(accounts) = &self.accounts {
            return accounts.reserve_password_change();
        }
        self.fallback_session_effects
            .as_ref()
            .ok_or_else(|| ServiceError::internal(std::io::Error::other("recovery finalization owner is missing")))?
            .reserve()
            .map_err(map_ownership)
    }
    pub(super) fn session_finalizer(&self) -> RecoverySessionFinalizer {
        self.accounts.as_ref().map_or_else(
            || RecoverySessionFinalizer::Standalone(self.ctx.kv.sessions()),
            |accounts| RecoverySessionFinalizer::Account(accounts.password_change_finalizer()),
        )
    }
}
