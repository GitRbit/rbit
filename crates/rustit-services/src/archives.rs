//! Source archives (`zip`, `tar.gz`) of a commit, with bounded concurrency.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.

use crate::ServiceContext;

#[derive(Debug, Clone)]
pub struct ArchiveService {
    #[expect(dead_code, reason = "stub until the workstream implements the service")]
    ctx: ServiceContext,
}

impl ArchiveService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }
}
