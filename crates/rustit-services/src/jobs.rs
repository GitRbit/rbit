//! Background jobs: post-push cache warming, repository maintenance, pruning.
//!
//! Milestone 1 workstream O owns this file.

use crate::ServiceContext;

#[derive(Debug, Clone)]
pub struct JobService {
    ctx: ServiceContext,
}

impl JobService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }

    /// Starts this service's background subscribers. Called once at server start-up.
    pub fn spawn_background(&self) {
        // TODO(workstream O): subscribe to `self.ctx.events`.
        let _ = &self.ctx;
    }
}
