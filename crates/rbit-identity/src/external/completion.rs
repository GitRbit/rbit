//! What a verified identity does at the end of a sign-in or link.
//!
//! In order, for the provider's stable subject:
//!
//! 1. already linked: a link request succeeds only for the same account; otherwise its
//!    use is recorded and it signs in as its account ([`sign_in_linked`]);
//! 2. unknown, and the flow is a link started by a signed-in, active account: link it
//!    to that account;
//! 3. unknown, the provider has `trust_email`, its address is verified and equals the
//!    verified address of an account: link it there and sign in;
//! 4. unknown, and the provider may register accounts now: continue at the "choose a
//!    username" step;
//! 5. otherwise refuse, explaining how to link from settings.
//!
//! Signing in refuses a suspended account and continues at the second step when the
//! account has two-factor authentication, unless the provider has `trust_mfa` and the
//! identity reports `mfa`. The [`ExternalAccounts`] port does every lookup, write and
//! audit record; this module decides their order and all user-facing text.

use std::future::Future;

use super::{ProviderPolicy, VerifiedIdentity, linked_detail, may_register};
use crate::{AccountId, SessionToken, User};

/// Why a link could not be stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkConflict {
    /// The identity is already linked to an account (possibly this one).
    IdentityTaken,
    /// The account already has an identity at this provider.
    ProviderTaken,
}

/// What an audit record says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditKind {
    SignIn,
    SignInFailed,
    IdentityLinked,
}

/// One audit record; the adapter adds the client address.
#[derive(Debug)]
pub struct ExternalAudit<'a> {
    pub kind: AuditKind,
    pub by: Option<&'a User>,
    pub detail: String,
}

/// A linked identity and what its lookup captured, if one was found.
pub type Found<L, O> = Option<(L, O)>;

/// The lookups, writes and effects of external sign-in. `Origin` is what a lookup
/// captured about the account for finishing the sign-in; `Account` is what a finished
/// sign-in yields.
pub trait ExternalAccounts: Sync {
    type Error: Send;
    type Origin: Send + Sync;
    type Linked: Send + Sync;
    type Account: Send;

    /// The identity linked for `provider`/`subject`, if any.
    fn find(&self, provider: &str, subject: &str) -> impl Future<Output = Result<Found<Self::Linked, Self::Origin>, Self::Error>> + Send;
    fn linked_account(&self, linked: &Self::Linked) -> AccountId;
    /// Records the identity's use and refreshes its address and login.
    fn touch(&self, linked: &Self::Linked, identity: &VerifiedIdentity) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// The account, unless it is missing, suspended or not a user.
    fn active_user(&self, account: AccountId) -> impl Future<Output = Result<Option<User>, Self::Error>> + Send;
    /// The account, whatever its state (for audit records).
    fn user(&self, account: AccountId) -> impl Future<Output = Result<Option<User>, Self::Error>> + Send;
    fn is_suspended(&self, account: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Links the identity to `account` on a signed-in request.
    fn link(
        &self,
        account: AccountId,
        identity: &VerifiedIdentity,
    ) -> impl Future<Output = Result<Result<Self::Linked, LinkConflict>, Self::Error>> + Send;
    /// The account whose verified address is `email`, if any.
    fn verified_email_account(&self, email: &str) -> impl Future<Output = Result<Found<AccountId, Self::Origin>, Self::Error>> + Send;
    /// Links the identity to the account found by its verified address.
    fn link_by_email(
        &self,
        account: AccountId,
        identity: &VerifiedIdentity,
        origin: &Self::Origin,
    ) -> impl Future<Output = Result<Result<Self::Linked, LinkConflict>, Self::Error>> + Send;
    /// Whether the instance still waits for its first (administrator) account.
    fn setup_required(&self) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Whether the site's registration is open.
    fn registration_open(&self) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Opens the "choose a username" step and returns its token.
    fn start_registration(&self, identity: VerifiedIdentity, next: String) -> SessionToken;
    /// Finishes signing in to `account` with what the lookup captured.
    fn finish(&self, account: AccountId, origin: Self::Origin) -> impl Future<Output = Result<Self::Account, Self::Error>> + Send;
    fn account_user<'a>(&self, account: &'a Self::Account) -> &'a User;
    fn two_factor_enabled(&self, account: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Continues at the second step, named after the provider.
    fn start_second_factor(&self, account: Self::Account, method: &str) -> SessionToken;
    fn audit(&self, entry: ExternalAudit<'_>) -> impl Future<Output = ()> + Send;
}

/// The result of a completed sign-in or link.
#[derive(Debug)]
pub enum Completion<A> {
    /// Open a session.
    SignedIn(A),
    /// Continue at the second step, like a password sign-in.
    SecondFactorRequired(SessionToken),
    /// The identity is now linked to the signed-in account that asked for it.
    Linked,
    /// Ask for a username; the token identifies the pending registration.
    Register(SessionToken),
}

/// Why completion stopped.
#[derive(Debug)]
pub enum CompletionError<E> {
    /// Refused with a message that is safe to show.
    Refused(String),
    /// Refused without explanation (a suspended account, or a link by an account that
    /// is no longer active).
    Forbidden,
    Port(E),
}

impl<E> From<E> for CompletionError<E> {
    fn from(error: E) -> Self {
        Self::Port(error)
    }
}

/// Decides what a verified callback means: sign in, second step, link, register or
/// refuse. `link_account` is the signed-in account that started a link, if any.
pub async fn complete<P: ExternalAccounts>(
    port: &P,
    provider: &ProviderPolicy<'_>,
    identity: VerifiedIdentity,
    next: String,
    link_account: Option<AccountId>,
) -> Result<Completion<P::Account>, CompletionError<P::Error>> {
    let taken = || CompletionError::Refused(already_linked(provider.name));

    if let Some((linked, origin)) = port.find(&identity.provider_id, &identity.subject).await? {
        if let Some(account) = link_account {
            return if port.linked_account(&linked) == account { Ok(Completion::Linked) } else { Err(taken()) };
        }
        port.touch(&linked, &identity).await?;
        return sign_in_linked(port, provider, port.linked_account(&linked), origin, &identity).await;
    }

    if let Some(account) = link_account {
        let user = port.active_user(account).await?.ok_or(CompletionError::Forbidden)?;
        return match port.link(account, &identity).await? {
            Ok(_) => {
                port.audit(linked_audit(&user, provider, &identity, "")).await;
                Ok(Completion::Linked)
            }
            Err(conflict) => Err(CompletionError::Refused(link_refused(conflict, provider.name))),
        };
    }

    if provider.trust_email
        && identity.email_verified
        && let Some(email) = identity.email.as_deref()
        && let Some((account, origin)) = port.verified_email_account(email).await?
    {
        return match port.link_by_email(account, &identity, &origin).await? {
            Ok(linked) => {
                if let Some(user) = port.active_user(account).await? {
                    port.audit(linked_audit(&user, provider, &identity, ", by verified email")).await;
                }
                sign_in_linked(port, provider, port.linked_account(&linked), origin, &identity).await
            }
            Err(LinkConflict::ProviderTaken) => Err(CompletionError::Refused(format!(
                "the account with this email address is linked to a different {} account",
                provider.name
            ))),
            Err(LinkConflict::IdentityTaken) => Err(taken()),
        };
    }

    if registration_allowed(port, provider).await? {
        return Ok(Completion::Register(port.start_registration(identity, next)));
    }
    let detail = format!("{}: no linked account", provider.name);
    port.audit(ExternalAudit { kind: AuditKind::SignInFailed, by: None, detail }).await;
    Err(CompletionError::Refused(format!(
        "no account here is linked to this {name} account, and new accounts can't be created with it. If you have an account, \
         sign in with your password and link {name} under Settings → Security",
        name = provider.name
    )))
}

/// Whether a first sign-in with the provider may create an account now. Never on an
/// instance without users: its first account becomes the administrator, which takes
/// the setup token.
pub async fn registration_allowed<P: ExternalAccounts>(port: &P, provider: &ProviderPolicy<'_>) -> Result<bool, P::Error> {
    if port.setup_required().await? {
        return Ok(false);
    }
    Ok(may_register(provider.registration, port.registration_open().await?))
}

/// Signs in as `account`, which a linked identity belongs to: refused while suspended,
/// continued at the second step when the account has two-factor authentication (unless
/// the provider's MFA is trusted and reported).
pub async fn sign_in_linked<P: ExternalAccounts>(
    port: &P,
    provider: &ProviderPolicy<'_>,
    account: AccountId,
    origin: P::Origin,
    identity: &VerifiedIdentity,
) -> Result<Completion<P::Account>, CompletionError<P::Error>> {
    if port.is_suspended(account).await? {
        let user = port.user(account).await?;
        let detail = format!("{}: account suspended", provider.name);
        port.audit(ExternalAudit { kind: AuditKind::SignInFailed, by: user.as_ref(), detail }).await;
        return Err(CompletionError::Forbidden);
    }
    let finished = port.finish(account, origin).await?;
    let user = port.account_user(&finished);
    let trusted_mfa = provider.trust_mfa && identity.mfa;
    if !trusted_mfa && port.two_factor_enabled(user.id()).await? {
        return Ok(Completion::SecondFactorRequired(port.start_second_factor(finished, provider.name)));
    }
    let detail = if trusted_mfa { format!("{} (provider MFA)", provider.name) } else { provider.name.to_owned() };
    port.audit(ExternalAudit { kind: AuditKind::SignIn, by: Some(user), detail }).await;
    Ok(Completion::SignedIn(finished))
}

/// The identity is linked to some other account.
pub fn already_linked(provider_name: &str) -> String {
    format!("this {provider_name} account is already linked to another account here")
}

/// Why a signed-in account could not link the identity.
pub fn link_refused(conflict: LinkConflict, provider_name: &str) -> String {
    match conflict {
        LinkConflict::IdentityTaken => already_linked(provider_name),
        LinkConflict::ProviderTaken => format!("your account is already linked to another {provider_name} account; unlink it first"),
    }
}

/// The audit record of a new link.
pub fn linked_audit<'a>(user: &'a User, provider: &ProviderPolicy<'_>, identity: &VerifiedIdentity, suffix: &str) -> ExternalAudit<'a> {
    ExternalAudit { kind: AuditKind::IdentityLinked, by: Some(user), detail: linked_detail(provider.name, identity, suffix) }
}
