//! Two-factor authentication (TOTP) and recovery codes.
//!
//! Milestone 1 workstream S owns this file.

use crate::ServiceContext;

#[derive(Debug, Clone)]
pub struct TwoFactorService {
    ctx: ServiceContext,
}

impl TwoFactorService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }
}
