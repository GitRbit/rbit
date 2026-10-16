//! Organisation teams: members, and the permission each team has on repositories.
//!
//! Milestone 2 workstream A (access) owns this file.

use crate::ServiceContext;

#[derive(Debug, Clone)]
pub struct TeamService {
    #[expect(dead_code, reason = "stub until the workstream implements the service")]
    ctx: ServiceContext,
}

impl TeamService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }
}
