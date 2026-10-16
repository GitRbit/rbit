//! Outgoing email delivery (SMTP, log, disabled).
//!
//! Milestone 1 workstream E owns this file.

use crate::ServiceContext;

#[derive(Debug, Clone)]
pub struct MailService {
    ctx: ServiceContext,
}

impl MailService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }
}
