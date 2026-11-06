//! Administrative account creation shares the instance password-work owner.

use super::{AdminService, require_admin};
use crate::audit::{AuditAction, AuditEntry};
use crate::{DomainEvent, ServiceError, SignUp};
use rbit_core::User;
use rbit_identity::accounts::AccountRegistration;

impl AdminService {
    /// Creates an account. Works even while registration is closed.
    pub fn create_user<'a>(
        &'a self,
        actor: &'a User,
        input: SignUp,
        is_admin: bool,
    ) -> impl Future<Output = Result<User, ServiceError>> + Send + 'a {
        let repository = self.ctx.database.accounts();
        // Constructing the protected input future has no policy, hash or store effects.
        let mut password = zeroize::Zeroizing::new(input.password);
        let input = SignUp { username: input.username, email: input.email, password: String::new() };
        async move {
            require_admin(actor)?;
            let user = AccountRegistration::new(&repository, &self.passwords, self.ctx.config.security.min_password_length, None)
                .create_user_with_initial_password(SignUp { password: std::mem::take(&mut *password), ..input }, is_admin)
                .await
                .map_err(crate::accounts::application_error::registration)?;
            tracing::info!(admin = %actor.name(), user = %user.name(), is_admin, "administrator created account");
            // Subscribers (e.g. email verification) treat every new account the same way.
            self.ctx.events.publish(DomainEvent::UserSignedUp { user: user.id() });
            let detail = if is_admin { "as an administrator" } else { "" };
            self.audit.record(AuditEntry::new(AuditAction::UserCreated).by(actor).on(user.name().as_str()).detail(detail)).await;
            Ok(user)
        }
    }
}
