//! SSH public keys for users and deploy keys for repositories.
//!
//! Milestone 1 workstream H owns this file.

use crate::ServiceContext;

#[derive(Debug, Clone)]
pub struct SshKeyService {
    ctx: ServiceContext,
}

impl SshKeyService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }
}
