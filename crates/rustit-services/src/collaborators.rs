//! Repository collaborators: adding people to a repository with a permission level.
//!
//! Milestone 2 workstream A (access) owns this file.

use crate::ServiceContext;

#[derive(Debug, Clone)]
pub struct CollaboratorService {
    #[expect(dead_code, reason = "stub until the workstream implements the service")]
    ctx: ServiceContext,
}

impl CollaboratorService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }
}
