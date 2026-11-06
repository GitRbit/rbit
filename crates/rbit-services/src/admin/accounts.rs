//! Administrative account creation shares the instance password-work owner.

use super::{AdminService, require_admin};
use crate::audit::{AuditAction, AuditEntry};
use crate::{DomainEvent, ServiceError, SignUp, accounts};
use rbit_core::User;

impl AdminService {
    /// Creates an account. Works even while registration is closed.
    pub async fn create_user(&self, actor: &User, input: SignUp, is_admin: bool) -> Result<User, ServiceError> {
        require_admin(actor)?;
        let new = accounts::prepare_user(&self.ctx.config, &self.passwords, input, is_admin).await?;
        let user = self.ctx.database.accounts().create_user_with_initial_password(new).await?;
        tracing::info!(admin = %actor.name(), user = %user.name(), is_admin, "administrator created account");
        // Subscribers (e.g. email verification) treat every new account the same way.
        self.ctx.events.publish(DomainEvent::UserSignedUp { user: user.id() });
        let detail = if is_admin { "as an administrator" } else { "" };
        self.audit.record(AuditEntry::new(AuditAction::UserCreated).by(actor).on(user.name().as_str()).detail(detail)).await;
        Ok(user)
    }
}
