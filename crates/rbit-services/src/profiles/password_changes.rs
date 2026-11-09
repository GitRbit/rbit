//! A counted lease spans verification, commit and adopted finalization ownership.

use super::{ProfileService, Throttle, map, map_with};
use crate::{
    DomainEvent, ServiceError,
    audit::{AuditAction, AuditEntry, AuditService},
};
use rbit_core::User;
use rbit_identity::profiles::commit_password_change;

impl ProfileService {
    /// Replaces the password after re-checking the current one.
    pub fn change_password(
        &self,
        user: &User,
        current_password: String,
        new_password: String,
    ) -> impl std::future::Future<Output = Result<(), ServiceError>> {
        let mut current_password = zeroize::Zeroizing::new(current_password);
        let mut new_password = zeroize::Zeroizing::new(new_password);
        async move {
            let (store, accounts, throttle) = (self.store(), self.ctx.database.accounts(), Throttle(&self.throttle));
            let app = self.application(&store, &accounts, &throttle);
            // The identity application reserves before its first await but reports a closed
            // owner only at the original admission boundary, after validation and throttling.
            let prepared = app
                .prepare_password_change(user, std::mem::take(&mut *current_password), std::mem::take(&mut *new_password), || {
                    self.accounts.reserve_password_change()
                })
                .await
                .map_err(|error| map_with(error, |error| error))?;
            // Narrow worker capabilities deliberately exclude the tracking supervisor.
            let finalizer = self.accounts.password_change_finalizer();
            let audit = AuditService::new(self.ctx.clone());
            let events = self.ctx.events.clone();
            let user = user.clone();
            let (hash, verified) = (prepared.hash, prepared.verified);
            crate::credentials::finish_update(prepared.ownership, move |context| async move {
                commit_password_change(&store, user.id(), &hash, &verified).await.map_err(|error| map(widen(error)))?;
                finalizer.preserving_current(&context, user.id()).await?;
                tracing::info!(user = %user.name(), "password changed");
                audit.record(AuditEntry::new(AuditAction::PasswordChanged).by(&user)).await;
                events.publish(DomainEvent::PasswordChanged { user: user.id() });
                Ok(())
            })
            .await
        }
    }
}

/// The commit has no throttle; widen its error to the adapter's mapping type.
fn widen<S>(
    error: rbit_identity::profiles::ProfileError<S, std::convert::Infallible>,
) -> rbit_identity::profiles::ProfileError<S, ServiceError> {
    use rbit_identity::profiles::ProfileError as E;
    match error {
        E::Store(error) => E::Store(error),
        E::Throttle(never) | E::Reserve(never) => match never {},
        E::Rejected(rejection) => E::Rejected(rejection),
        E::Name(error) => E::Name(error),
        E::InvalidCredentials => E::InvalidCredentials,
        E::NotFound => E::NotFound,
        E::Admission(error) => E::Admission(error),
        E::Password(error) => E::Password(error),
    }
}
