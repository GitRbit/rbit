//! Narrow event-handler capabilities, deliberately excluding the subscriber supervisor.

use super::{RecoveryService, map};
use crate::{DomainEvent, InstanceService, MailService, ServiceError};
use rbit_core::{Config, Email};
use rbit_identity::recovery::{RecoveryLinks, RecoveryThrottle};
use rbit_mail::{
    Mail,
    messages::{EmailChanged, VerifyEmail, VerifyReason, mask_address},
};
use rbit_storage::sqlite::recovery::{RecoveryAccount, RecoveryStore, TokenPurpose};
use std::sync::Arc;

pub(super) struct EmailEvents {
    config: Arc<Config>,
    store: RecoveryStore,
    mail: MailService,
    instance: Option<InstanceService>,
}

impl RecoveryService {
    pub(super) fn email_events(&self) -> EmailEvents {
        EmailEvents { config: Arc::clone(&self.ctx.config), store: self.store(), mail: self.mail.clone(), instance: self.instance.clone() }
    }
}

/// Event-driven mail is not a user request, so it spends no request budget.
struct Unthrottled;

impl RecoveryThrottle for Unthrottled {
    type Error = ServiceError;
    fn check_client(&self, _: &str) -> Result<(), ServiceError> {
        Ok(())
    }
    fn check_recipient(&self, _: &str) -> Result<(), ServiceError> {
        Ok(())
    }
}

impl EmailEvents {
    fn links<'a>(&self, store: &'a RecoveryStore) -> RecoveryLinks<'a, RecoveryStore, Unthrottled> {
        RecoveryLinks::new(store, &Unthrottled, self.mail.is_enabled())
    }

    /// Reacts to one domain event. Public so tests can drive it without timing games.
    pub(super) async fn handle(&self, event: &DomainEvent) -> Result<(), ServiceError> {
        match event {
            DomainEvent::UserSignedUp { user } => {
                let store = self.store();
                // Accounts registered through a provider that verified the address start
                // verified; there is nothing to confirm.
                let Some(account) = self.links(&store).sign_up_verification(*user).await.map_err(map)? else { return Ok(()) };
                self.send_verification(&account, VerifyReason::SignUp).await
            }
            DomainEvent::EmailChanged { user, previous } => {
                let store = self.store();
                let changed = self.links(&store).changed_email_verification(*user, previous).await.map_err(map)?;
                let Some(account) = changed else { return Ok(()) };
                self.send_verification(&account, VerifyReason::Changed).await?;
                self.notify_previous_address(&account, previous)
            }
            DomainEvent::RepositoryPushed { .. }
            | DomainEvent::RepositoryCreated { .. }
            | DomainEvent::RepositoryDeleted { .. }
            | DomainEvent::RepositoryTransferred { .. }
            | DomainEvent::RepositoryArchived { .. }
            | DomainEvent::RepositoryForked { .. }
            | DomainEvent::ReleasePublished { .. }
            | DomainEvent::PasswordChanged { .. }
            | DomainEvent::Issue { .. }
            | DomainEvent::IssueComment { .. }
            | DomainEvent::PullRequestReview { .. }
            | DomainEvent::CollaboratorChanged { .. }
            | DomainEvent::CommitStatus { .. }
            // Actions: integration (I).
            | DomainEvent::ActionsRun { .. }
            | DomainEvent::ActionsJob { .. } => Ok(()),
        }
    }
    pub(super) async fn send_verification(&self, account: &RecoveryAccount, reason: VerifyReason) -> Result<(), ServiceError> {
        let store = self.store();
        let token = self.links(&store).issue(account, TokenPurpose::Verify).await.map_err(map)?;
        let message = VerifyEmail {
            site_name: self.site_name(),
            base_url: self.base_url().to_owned(),
            username: account.name.to_string(),
            link: format!("{}/email/verify/{token}", self.base_url()),
            valid_for: "2 days".to_owned(),
            reason,
        };
        self.mail.queue(Mail::render(account.email.clone(), &message).map_err(ServiceError::internal)?).await;
        Ok(())
    }

    fn notify_previous_address(&self, account: &RecoveryAccount, previous: &str) -> Result<(), ServiceError> {
        let Ok(previous) = previous.parse::<Email>() else { return Ok(()) };
        let message = EmailChanged {
            site_name: self.site_name(),
            base_url: self.base_url().to_owned(),
            username: account.name.to_string(),
            new_address: mask_address(account.email.as_str()),
        };
        self.mail.send(Mail::render(previous, &message).map_err(ServiceError::internal)?);
        Ok(())
    }

    fn store(&self) -> RecoveryStore {
        self.store.clone()
    }
    fn site_name(&self) -> String {
        self.instance.as_ref().map_or_else(|| self.config.server.site_name.clone(), |instance| instance.current().site_name)
    }
    fn base_url(&self) -> &str {
        self.config.server.base_url()
    }
}
