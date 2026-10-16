//! Forks: copying a repository to another owner and remembering where it came from.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.

use crate::{RepositoryService, ServiceContext};

#[derive(Debug, Clone)]
pub struct ForkService {
    #[expect(dead_code, reason = "stub until the workstream implements the service")]
    ctx: ServiceContext,
    #[expect(dead_code, reason = "stub until the workstream implements forks")]
    repositories: RepositoryService,
}

impl ForkService {
    pub(crate) fn new(ctx: ServiceContext, repositories: RepositoryService) -> Self {
        Self { ctx, repositories }
    }
}
