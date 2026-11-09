//! Checks on a provider's callback before any code is exchanged.
//!
//! The sealed flow cookie must open (it is the browser's proof that it started this
//! sign-in, within [`FLOW_TTL_SECONDS`]), be for this provider, and carry exactly the
//! returned `state`. A provider error is then reported in its own words, and only a
//! present code of at most [`MAX_CODE_LEN`] bytes goes on to the exchange. Every refusal
//! carries a log reason and a message that is safe to show.

use super::{ProviderPolicy, truncate};
use crate::oidc::{Flow, FlowKey};

/// How long a started sign-in may take at the provider.
pub const FLOW_TTL_SECONDS: i64 = 10 * 60;

/// Longest authorization code accepted.
pub const MAX_CODE_LEN: usize = 2048;

/// Shown when the flow cookie or `state` does not match, or no code came back.
pub const EXPIRED: &str = "the sign-in took too long or was started in another browser; try again";

/// Shown when the flow key itself is unavailable.
pub const FAILED: &str = "something went wrong; try again";

/// What the provider sent back to the callback URL.
#[derive(Debug, Clone, Copy, Default)]
pub struct CallbackParams<'a> {
    pub code: Option<&'a str>,
    pub state: Option<&'a str>,
    pub error: Option<&'a str>,
    pub error_description: Option<&'a str>,
}

/// A refused callback: what to log, and what to tell the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub log: String,
    pub user: String,
}

impl Refusal {
    pub fn new(log: impl Into<String>, user: impl Into<String>) -> Self {
        Self { log: log.into(), user: user.into() }
    }
}

/// Opens the flow cookie and checks the callback against it. Returns the flow and the
/// authorization code to exchange.
pub fn check<'a>(
    key: &FlowKey,
    provider: &ProviderPolicy<'_>,
    cookie: Option<&str>,
    params: CallbackParams<'a>,
    now: i64,
) -> Result<(Flow, &'a str), Refusal> {
    let flow = cookie
        .and_then(|value| key.open(value, now, FLOW_TTL_SECONDS))
        .ok_or_else(|| Refusal::new("missing, forged or expired flow cookie", EXPIRED))?;
    if flow.provider != provider.id {
        return Err(Refusal::new("flow cookie for another provider", EXPIRED));
    }
    if !params.state.is_some_and(|state| flow.state_matches(state)) {
        return Err(Refusal::new("state mismatch", EXPIRED));
    }
    if let Some(error) = params.error {
        let detail = params.error_description.map_or_else(|| error.to_owned(), |description| format!("{error}: {description}"));
        let message = if error == "access_denied" {
            format!("{} didn't sign you in (access was denied)", provider.name)
        } else {
            format!("{} reported an error: {}", provider.name, truncate(&detail, 120))
        };
        return Err(Refusal::new(format!("provider error {detail}"), message));
    }
    let code = params.code.filter(|code| !code.is_empty() && code.len() <= MAX_CODE_LEN).ok_or_else(|| Refusal::new("no code", EXPIRED))?;
    Ok((flow, code))
}

/// The refusal when the provider could not confirm the identity after the exchange.
pub fn unconfirmed(provider: &ProviderPolicy<'_>, log: impl Into<String>) -> Refusal {
    Refusal::new(log, format!("{} couldn't confirm who you are; try again", provider.name))
}
