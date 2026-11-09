//! External sign-in decisions (OIDC and GitHub OAuth providers), with no HTTP, storage or
//! configuration types.
//!
//! * [`callback`]: what a provider's callback must carry before anything is exchanged
//!   (the sealed flow cookie, its provider, the exact `state`, no provider error, a code),
//!   and the text the user sees for each refusal.
//! * [`completion`]: what a verified identity does, behind the [`completion::ExternalAccounts`]
//!   port: sign in its linked account, link it to the account that asked, link it by a
//!   trusted verified address, continue at the "choose a username" step, or refuse. Also
//!   the suspension and second-step gate when signing in.
//! * [`registration`]: finishing the "choose a username" step behind the same port, and
//!   [`pending`]: the open steps.
//! * [`provider`]: the authorization request and how provider answers (ID token,
//!   userinfo, GitHub's user) become a [`VerifiedIdentity`].
//! * [`is_last_method`]: an identity can't be unlinked while it is the account's only
//!   way to sign in.
//!
//! An identity is always the provider's stable subject, never an address or login name,
//! and an unverified address never links anything. The provider HTTP exchange that
//! produces a [`VerifiedIdentity`] stays an adapter outside this crate; the protocol
//! building blocks it uses are in [`crate::oidc`].

pub mod callback;
pub mod completion;
pub mod pending;
pub mod provider;
pub mod registration;

use crate::AccountName;

/// Who the provider says signed in, built only after the callback was verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedIdentity {
    pub provider_id: String,
    /// The provider's stable id for the user (`sub`, or GitHub's numeric id).
    pub subject: String,
    pub email: Option<String>,
    /// The provider vouches for `email`.
    pub email_verified: bool,
    /// The provider's login name (`preferred_username`, GitHub's `login`).
    pub username: Option<String>,
    pub display_name: Option<String>,
    /// The ID token reports multi-factor authentication (`amr` contains `mfa`).
    pub mfa: bool,
}

/// Whether a provider may create accounts on a first sign-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderRegistration {
    /// Never.
    Never,
    /// Whatever the site's registration setting says.
    FollowSite,
    /// Even while the site's registration is closed.
    Always,
}

/// A configured provider's sign-in policy.
#[derive(Debug, Clone, Copy)]
pub struct ProviderPolicy<'a> {
    pub id: &'a str,
    /// The display name used in every message.
    pub name: &'a str,
    /// A verified address may link to the account with the same verified address.
    pub trust_email: bool,
    /// The provider's `mfa` claim satisfies the second step.
    pub trust_mfa: bool,
    pub registration: ProviderRegistration,
}

/// Whether a first sign-in with a provider may create an account, given whether the
/// site's registration is open.
pub fn may_register(registration: ProviderRegistration, registration_open: bool) -> bool {
    match registration {
        ProviderRegistration::Never => false,
        ProviderRegistration::Always => true,
        ProviderRegistration::FollowSite => registration_open,
    }
}

/// The first candidate that makes a valid username once characters outside
/// `[A-Za-z0-9._-]` are dropped (empty when none does).
pub fn suggest_username(candidates: &[Option<&str>]) -> String {
    candidates
        .iter()
        .flatten()
        .map(|candidate| {
            let cleaned: String = candidate.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')).collect();
            cleaned.trim_start_matches('.').chars().take(AccountName::MAX_LEN).collect::<String>()
        })
        .find(|name| name.parse::<AccountName>().is_ok())
        .unwrap_or_default()
}

/// The username suggested for the "choose a username" step: the provider's login, else
/// the local part of its address.
pub fn suggested_username(identity: &VerifiedIdentity) -> String {
    suggest_username(&[identity.username.as_deref(), identity.email.as_deref().and_then(|email| email.split('@').next())])
}

/// Whether the "choose a username" step may mark the provider's address verified: the
/// provider vouches for it and sent one.
pub fn vouched_email(identity: &VerifiedIdentity) -> bool {
    identity.email_verified && identity.email.is_some()
}

/// Refusal when unlinking the account's last way to sign in.
pub const LAST_METHOD: &str =
    "this is the only way to sign in to your account; set a password (with a reset link) or link another provider first";

/// Unlinking `provider_id` would leave the account without a way to sign in: no password,
/// and no other identity at a provider that is still configured. `identities` are the
/// linked provider ids, each with whether it is still configured.
pub fn is_last_method<'a>(has_password: bool, identities: impl IntoIterator<Item = (&'a str, bool)>, provider_id: &str) -> bool {
    !has_password && !identities.into_iter().any(|(id, configured)| configured && id != provider_id)
}

/// `text` without control characters, at most `max` characters long.
pub fn truncate(text: &str, max: usize) -> String {
    text.chars().filter(|c| !c.is_control()).take(max).collect()
}

/// "Corp SSO (alice)" plus `suffix`: the detail of a link audit entry.
pub fn linked_detail(provider_name: &str, identity: &VerifiedIdentity, suffix: &str) -> String {
    let who = identity.username.as_deref().or(identity.email.as_deref()).map(|who| format!(" ({})", truncate(who, 64))).unwrap_or_default();
    format!("{provider_name}{who}{suffix}")
}

#[cfg(test)]
mod tests;
