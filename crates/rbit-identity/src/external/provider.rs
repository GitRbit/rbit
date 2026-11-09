//! What is asked of a provider, and how its answers become a [`VerifiedIdentity`].
//!
//! The HTTP exchange, discovery and the outbound address guard are an adapter outside
//! this crate; these functions decide the request parameters and read the answers:
//!
//! * the authorization request: the code flow with PKCE `S256` and, for OIDC, a nonce;
//! * an OIDC identity comes from the verified ID token; the userinfo endpoint is asked
//!   only when the token has no address, and its answer counts only for the same
//!   subject;
//! * a GitHub identity is its numeric id and login, with the verified primary address
//!   only.

use serde_json::Value;

use super::VerifiedIdentity;
use crate::oidc::{IdToken, Pkce};

/// The authorization request's query parameters, in order.
pub fn authorization_query<'a>(
    client_id: &'a str,
    redirect_uri: &'a str,
    scopes: &'a str,
    state: &'a str,
    challenge: &'a str,
    nonce: Option<&'a str>,
) -> Vec<(&'static str, &'a str)> {
    let mut query = vec![
        ("response_type", "code"),
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("scope", scopes),
        ("state", state),
        ("code_challenge", challenge),
        ("code_challenge_method", Pkce::METHOD),
    ];
    if let Some(nonce) = nonce {
        query.push(("nonce", nonce));
    }
    query
}

/// The identity a verified ID token names.
pub fn oidc_identity(provider_id: &str, token: IdToken) -> VerifiedIdentity {
    VerifiedIdentity {
        provider_id: provider_id.to_owned(),
        subject: token.subject,
        email: token.email,
        email_verified: token.email_verified,
        username: token.preferred_username,
        display_name: token.name,
        mfa: token.amr.iter().any(|method| method == "mfa"),
    }
}

/// Some providers keep the address out of the ID token: then the userinfo endpoint is
/// asked for it.
pub fn needs_userinfo(identity: &VerifiedIdentity) -> bool {
    identity.email.is_none()
}

/// Takes the address (and a login name the token lacked) from a userinfo answer.
/// Returns `false`, changing nothing, when the answer is about another subject.
pub fn apply_userinfo(identity: &mut VerifiedIdentity, info: &Value) -> bool {
    if info.get("sub").and_then(Value::as_str) != Some(identity.subject.as_str()) {
        return false;
    }
    identity.email = info.get("email").and_then(Value::as_str).map(str::to_owned);
    identity.email_verified = matches!(info.get("email_verified"), Some(Value::Bool(true)))
        || info.get("email_verified").and_then(Value::as_str).is_some_and(|value| value.eq_ignore_ascii_case("true"));
    identity.username = identity.username.take().or_else(|| info.get("preferred_username").and_then(Value::as_str).map(str::to_owned));
    true
}

/// A GitHub user: its numeric `id`, `login` and display name, and its verified primary
/// address if GitHub listed one.
pub fn github_identity(
    provider_id: &str,
    id: String,
    login: String,
    name: Option<String>,
    verified_email: Option<String>,
) -> VerifiedIdentity {
    VerifiedIdentity {
        provider_id: provider_id.to_owned(),
        subject: id,
        email_verified: verified_email.is_some(),
        email: verified_email,
        username: Some(login),
        display_name: name,
        mfa: false,
    }
}

#[cfg(test)]
mod tests;
