//! Exact bootstrap token and constant-time verification policy.

use std::fmt::Write as _;
use subtle::ConstantTimeEq as _;

use crate::accounts::{AccountError, AccountRepository};

pub const SETUP_TOKEN_REQUIRED: &str = "enter the setup token printed in the server log";
const SETUP_TOKEN_BYTES: usize = 16;

pub async fn bootstrap_setup_token<R: AccountRepository>(
    repository: &R,
    configured: Option<&str>,
) -> Result<Option<String>, AccountError<R::Error>> {
    if repository.count_users().await.map_err(AccountError::Store)? > 0 {
        return Ok(None);
    }
    if let Some(token) = configured.map(str::trim).filter(|token| !token.is_empty()) {
        return Ok(Some(token.to_owned()));
    }
    let mut bytes = [0_u8; SETUP_TOKEN_BYTES];
    getrandom::fill(&mut bytes).map_err(AccountError::Entropy)?;
    Ok(Some(bytes.iter().fold(String::with_capacity(SETUP_TOKEN_BYTES * 2), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })))
}

pub(super) fn token_matches(expected: Option<&str>, given: &str) -> bool {
    let Some(expected) = expected else { return false };
    let given = given.trim();
    expected.len() == given.len() && bool::from(expected.as_bytes().ct_eq(given.as_bytes()))
}
