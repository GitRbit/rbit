//! Finishing the "choose a username" step of a first external sign-in.
//!
//! The account is created without a password (an empty hash, which never verifies),
//! linked to the identity in the same write. Its address is marked verified only when it
//! is the provider's verified one. Registration must still be allowed when the step is
//! submitted; the store's own check uses the site setting read at submission.

use std::future::Future;

use super::{
    ProviderPolicy, VerifiedIdentity,
    completion::{AuditKind, ExternalAccounts, ExternalAudit, LinkConflict, linked_audit, registration_allowed},
    may_register,
};
use crate::{AccountName, Email, User};
use rbit_contracts::NameError;

/// The account to create, already validated.
#[derive(Debug, Clone)]
pub struct NewExternalAccount {
    pub name: AccountName,
    pub email: Email,
    /// Mark `email` verified: it is the provider's verified address.
    pub email_verified: bool,
    /// Registration is open for this provider, as the store re-checks it.
    pub registration_open: bool,
}

/// Why the store refused to create the account.
#[derive(Debug)]
pub enum CreateFailure<E> {
    /// The identity or the account's provider slot is already linked.
    Linked(LinkConflict),
    /// Another account already uses the address.
    EmailTaken,
    Store(E),
}

/// Account creation and the step's own effects.
pub trait ExternalRegistrations: ExternalAccounts {
    /// Whether the site's registration mode is open, as configured right now.
    fn site_registration_open(&self) -> bool;
    /// Creates the account linked to `identity`.
    fn create(
        &self,
        account: NewExternalAccount,
        identity: &VerifiedIdentity,
    ) -> impl Future<Output = Result<Self::Account, CreateFailure<Self::Error>>> + Send;
    /// Closes the step (its token stops working).
    fn end_registration(&self);
    /// Logs and announces the new account.
    fn created(&self, user: &User);
}

/// Why the step was refused.
#[derive(Debug)]
pub enum RegistrationError<E> {
    /// Registration with this provider is closed now.
    Closed,
    /// The chosen username or address is invalid.
    Name(NameError),
    /// Refused with a message that is safe to show.
    Refused(String),
    Port(E),
}

impl<E> From<E> for RegistrationError<E> {
    fn from(error: E) -> Self {
        Self::Port(error)
    }
}

/// Creates the account for the pending `identity` with the chosen `username` and
/// `email`, then records the link and the sign-in.
pub async fn register<P: ExternalRegistrations>(
    port: &P,
    provider: &ProviderPolicy<'_>,
    identity: &VerifiedIdentity,
    username: &str,
    email: &str,
) -> Result<P::Account, RegistrationError<P::Error>> {
    if !registration_allowed(port, provider).await? {
        return Err(RegistrationError::Closed);
    }
    let name: AccountName = username.trim().parse().map_err(RegistrationError::Name)?;
    let email: Email = email.trim().parse().map_err(RegistrationError::Name)?;
    // Only a provider trusted for addresses (`trust_email`, one that controls its users'
    // addresses) vouches for one: another's "verified" may be a claim its user typed, and a
    // verified address here is what later links an account by email.
    let email_verified = provider.trust_email
        && identity.email_verified
        && identity.email.as_deref().is_some_and(|given| given.eq_ignore_ascii_case(email.as_str()));
    let registration_open = may_register(provider.registration, port.site_registration_open());
    let new = NewExternalAccount { name, email, email_verified, registration_open };
    let account = match port.create(new, identity).await {
        Ok(account) => account,
        Err(CreateFailure::Linked(_)) => {
            port.end_registration();
            return Err(RegistrationError::Refused(format!(
                "this {} account is already linked to an account here; sign in instead",
                provider.name
            )));
        }
        Err(CreateFailure::EmailTaken) => {
            return Err(RegistrationError::Refused(format!(
                "an account already uses this email address; sign in to it and link {} under Settings → Security",
                provider.name
            )));
        }
        Err(CreateFailure::Store(error)) => return Err(RegistrationError::Port(error)),
    };
    port.end_registration();
    let user = port.account_user(&account);
    port.created(user);
    port.audit(linked_audit(user, provider, identity, ", new account")).await;
    port.audit(ExternalAudit { kind: AuditKind::SignIn, by: Some(user), detail: provider.name.to_owned() }).await;
    Ok(account)
}
