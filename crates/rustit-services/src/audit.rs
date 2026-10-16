//! Security audit log: sign-ins, 2FA changes, administrator actions.
//!
//! Milestone 1 workstream S owns this file.

use crate::ServiceContext;

#[derive(Debug, Clone)]
pub struct AuditService {
    ctx: ServiceContext,
}

impl AuditService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }
}
