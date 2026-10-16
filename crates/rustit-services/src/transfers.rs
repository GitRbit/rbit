//! Transferring a repository to another user or organisation.
//!
//! Milestone 2 workstream A (access) owns this file.

use crate::ServiceContext;

#[derive(Debug, Clone)]
pub struct TransferService {
    #[expect(dead_code, reason = "stub until the workstream implements the service")]
    ctx: ServiceContext,
}

impl TransferService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }
}
