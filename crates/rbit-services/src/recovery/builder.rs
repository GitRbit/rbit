//! Recovery dependencies must share the instance password owner explicitly.

use super::{RecoveryService, RequestThrottle};
use crate::{AccountService, InstanceService, MailService, ServiceContext, ThrottleService};
use rbit_identity::PasswordWork;
use std::sync::Arc;

/// Assembles a [`RecoveryService`] from shared services. Anything not supplied is built
/// from the context (its own mail transport from `[mail]`, its own throttle, and the site
/// name from `[server]`).
#[derive(Debug)]
pub struct RecoveryServiceBuilder {
    passwords: PasswordWork,
    ctx: ServiceContext,
    mail: Option<MailService>,
    throttle: Option<Arc<dyn RequestThrottle>>,
    instance: Option<InstanceService>,
    accounts: Option<AccountService>,
}

impl RecoveryServiceBuilder {
    /// The mail service to send through (share `Services::mail`).
    #[must_use]
    pub fn mail(mut self, mail: MailService) -> Self {
        self.mail = Some(mail);
        self
    }

    /// The rate limiter to consult (share `Services::throttle`).
    #[must_use]
    pub fn throttle(mut self, throttle: impl RequestThrottle + 'static) -> Self {
        self.throttle = Some(Arc::new(throttle));
        self
    }

    /// Runtime instance settings, for the site name in messages.
    #[must_use]
    pub fn instance(mut self, instance: InstanceService) -> Self {
        self.instance = Some(instance);
        self
    }

    /// Shares password-change session enforcement with signed-in request guards.
    #[must_use]
    pub fn accounts(mut self, accounts: AccountService) -> Self {
        self.accounts = Some(accounts);
        self
    }

    pub fn build(self) -> RecoveryService {
        let ctx = self.ctx;
        RecoveryService {
            passwords: self.passwords,
            mail: self.mail.unwrap_or_else(|| MailService::new(ctx.clone())),
            throttle: self.throttle.unwrap_or_else(|| Arc::new(ThrottleService::new(ctx.clone()))),
            instance: self.instance,
            accounts: self.accounts,
            ctx,
        }
    }
}

impl RecoveryService {
    pub fn builder(ctx: ServiceContext, passwords: PasswordWork) -> RecoveryServiceBuilder {
        RecoveryServiceBuilder { passwords, ctx, mail: None, throttle: None, instance: None, accounts: None }
    }
}
