//! Password reset and email verification.
//!
//! Milestone 1 workstream E owns this file.

use crate::ServiceContext;

#[derive(Debug, Clone)]
pub struct RecoveryService {
    ctx: ServiceContext,
}

impl RecoveryService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }

    /// Starts this service's background subscribers. Called once at server start-up.
    pub fn spawn_background(&self) {
        // TODO(workstream E): subscribe to `self.ctx.events`.
        let _ = &self.ctx;
    }
}
