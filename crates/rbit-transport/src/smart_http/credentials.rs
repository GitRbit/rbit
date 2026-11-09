//! What a Git client's `Authorization` header carries, and how the server asks for one.
//!
//! Only parsing lives here: which credential is then checked, and against what, belongs
//! to identity and Actions. A malformed header is treated as no credential at all.

use base64::{Engine, engine::general_purpose::STANDARD};

/// The `WWW-Authenticate` challenge that makes Git prompt for (better) credentials.
pub const BASIC_CHALLENGE: &str = "Basic realm=\"rbit\", charset=\"UTF-8\"";

/// The body sent with [`BASIC_CHALLENGE`].
pub const AUTHENTICATION_REQUIRED: &str = "authentication required";

/// `Basic <base64(login:password)>` (scheme `Basic` or `basic`): the login and the
/// password, which may itself contain colons. `None` for any other or malformed value.
pub fn basic_credentials(authorization: &str) -> Option<(String, String)> {
    let encoded = authorization.strip_prefix("Basic ").or_else(|| authorization.strip_prefix("basic "))?;
    let decoded = String::from_utf8(STANDARD.decode(encoded.trim()).ok()?).ok()?;
    let (login, password) = decoded.split_once(':')?;
    Some((login.to_owned(), password.to_owned()))
}

/// A workflow task token sent as `Authorization: Bearer` (or `token`, any case), as Git's
/// `http.extraHeader` can carry it. Personal access tokens are never read this way: they
/// stay Basic passwords on Git.
pub fn bearer_task_token(authorization: &str) -> Option<&str> {
    let (scheme, token) = authorization.trim().split_once(' ')?;
    let token = token.trim();
    ((scheme.eq_ignore_ascii_case("bearer") || scheme.eq_ignore_ascii_case("token")) && rbit_core::actions::looks_like_task_token(token))
        .then_some(token)
}
