//! Talking to identity providers: OIDC discovery, key sets, the token
//! exchange, the userinfo endpoint and GitHub's user API.
//!
//! Milestone 3 workstream O (external sign-in) owns this file.
//!
//! The HTTP client follows no redirects, ignores proxy variables, applies
//! `[auth] timeout_seconds` to every request and resolves names through the webhooks'
//! [`GuardedResolver`], so a provider (or a discovery document naming endpoints) cannot
//! make rbit reach internal addresses unless `[auth] allow_private_addresses` is set;
//! address literals are checked before sending. Responses are read up to
//! [`MAX_RESPONSE_BYTES`]. Endpoints must be `https` unless private addresses are allowed.

use std::{net::IpAddr, sync::Arc, time::Duration};

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_core::network::is_internal_address;
use reqwest::{Url, header};
use serde::Deserialize;
use serde_json::Value;

use crate::webhooks::delivery::{GuardedResolver, install_crypto_provider};

/// Sent as `User-Agent` (GitHub's API refuses requests without one).
pub const USER_AGENT: &str = concat!("rbit/", env!("CARGO_PKG_VERSION"));
/// `application/x-www-form-urlencoded` escaping (everything but `A-Za-z0-9*-._`).
const FORM: &AsciiSet = &NON_ALPHANUMERIC.remove(b'*').remove(b'-').remove(b'.').remove(b'_');
/// Largest response body read from a provider.
pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// Why talking to a provider failed. The message goes to the log; users see a summary.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("{0}")]
    Refused(String),
    #[error("request to {url} failed: {message}")]
    Http { url: String, message: String },
    #[error("{url} answered {status}: {body}")]
    Status { url: String, status: u16, body: String },
    #[error("{url} sent an unexpected response: {message}")]
    Response { url: String, message: String },
}

/// The HTTP client for one instance's providers.
#[derive(Debug, Clone)]
pub struct ProviderClient {
    http: reqwest::Client,
    allow_private: bool,
}

impl ProviderClient {
    pub fn new(allow_private: bool, timeout: Duration) -> Result<Self, reqwest::Error> {
        install_crypto_provider();
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(timeout)
            .connect_timeout(timeout)
            .pool_max_idle_per_host(2)
            .dns_resolver(Arc::new(GuardedResolver::new(allow_private)))
            .build()?;
        Ok(Self { http, allow_private })
    }

    /// Parses `url` and checks it may be contacted: `https` (or `http` with private
    /// addresses allowed), and no internal address literal unless allowed.
    pub fn endpoint(&self, url: &str) -> Result<Url, ProviderError> {
        let parsed = Url::parse(url).map_err(|error| ProviderError::Refused(format!("{url} is not a valid URL: {error}")))?;
        match parsed.scheme() {
            "https" => {}
            "http" if self.allow_private => {}
            _ => return Err(ProviderError::Refused(format!("{url} must use https"))),
        }
        let Some(host) = parsed.host_str() else {
            return Err(ProviderError::Refused(format!("{url} has no host")));
        };
        let literal = host.trim_start_matches('[').trim_end_matches(']').parse::<IpAddr>().ok();
        if !self.allow_private && literal.is_some_and(is_internal_address) {
            return Err(ProviderError::Refused(format!("{url} is an internal address ([auth] allow_private_addresses is off)")));
        }
        Ok(parsed)
    }

    /// `GET url` expecting JSON, with an optional bearer token and extra headers.
    pub async fn get_json(&self, url: &str, bearer: Option<&str>, headers: &[(&str, &str)]) -> Result<Value, ProviderError> {
        let endpoint = self.endpoint(url)?;
        let mut request = self.http.get(endpoint).header(header::ACCEPT, "application/json");
        if let Some(token) = bearer {
            request = request.bearer_auth(token);
        }
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        self.send(url, request).await
    }

    /// `POST url` with a form body expecting JSON; `basic` sends client credentials as
    /// HTTP Basic (`client_secret_basic`) instead of in the form.
    pub async fn post_form(&self, url: &str, form: &[(&str, &str)], basic: Option<(&str, &str)>) -> Result<Value, ProviderError> {
        let endpoint = self.endpoint(url)?;
        let mut request = self.http.post(endpoint).header(header::ACCEPT, "application/json").form(form);
        if let Some((id, secret)) = basic {
            // RFC 6749 §2.3.1: both parts are form-urlencoded before Basic encoding.
            let encode = |value: &str| utf8_percent_encode(value, FORM).to_string();
            request = request.basic_auth(encode(id), Some(encode(secret)));
        }
        self.send(url, request).await
    }

    async fn send(&self, url: &str, request: reqwest::RequestBuilder) -> Result<Value, ProviderError> {
        let http = |error: &reqwest::Error| ProviderError::Http { url: url.to_owned(), message: describe(error) };
        let mut response = request.send().await.map_err(|error| http(&error))?;
        let status = response.status();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| http(&error))? {
            body.extend_from_slice(&chunk);
            if body.len() > MAX_RESPONSE_BYTES {
                return Err(ProviderError::Response { url: url.to_owned(), message: "response too large".to_owned() });
            }
        }
        // Token endpoints answer errors as JSON with 400; keep them for the caller.
        let parsed: Result<Value, _> = serde_json::from_slice(&body);
        match (status.is_success(), parsed) {
            (true, Ok(value)) => Ok(value),
            (true, Err(_)) => Err(ProviderError::Response { url: url.to_owned(), message: "the body is not JSON".to_owned() }),
            (false, parsed) => {
                let body = match parsed {
                    Ok(value) => oauth_error(&value).unwrap_or_else(|| String::from_utf8_lossy(&body).chars().take(200).collect()),
                    Err(_) => String::from_utf8_lossy(&body).chars().take(200).collect(),
                };
                Err(ProviderError::Status { url: url.to_owned(), status: status.as_u16(), body })
            }
        }
    }
}

/// `error: error_description` from an OAuth error response (RFC 6749 §5.2), if it is one.
pub fn oauth_error(value: &Value) -> Option<String> {
    let error = value.get("error")?.as_str()?;
    Some(match value.get("error_description").and_then(Value::as_str) {
        Some(description) => format!("{error}: {}", description.chars().take(200).collect::<String>()),
        None => error.to_owned(),
    })
}

/// The error and its causes on one line.
fn describe(error: &reqwest::Error) -> String {
    let mut message = if error.is_timeout() { "timed out".to_owned() } else { error.to_string() };
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        let text = cause.to_string();
        if !message.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = cause.source();
    }
    message
}

// ---------------------------------------------------------------------- OIDC discovery

/// How the client authenticates at the token endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenAuth {
    /// HTTP Basic (`client_secret_basic`, the OIDC default).
    Basic,
    /// In the form body (`client_secret_post`).
    Post,
}

/// What rbit uses from `/.well-known/openid-configuration`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovery {
    /// Exactly as the document states it; ID tokens must carry this `iss`.
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    pub userinfo_endpoint: Option<String>,
    pub token_auth: TokenAuth,
}

#[derive(Deserialize)]
struct DiscoveryJson {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
    userinfo_endpoint: Option<String>,
    #[serde(default)]
    token_endpoint_auth_methods_supported: Option<Vec<String>>,
    #[serde(default)]
    code_challenge_methods_supported: Option<Vec<String>>,
}

impl Discovery {
    /// Checks a discovery document fetched for `configured_issuer`: the issuer must be the
    /// configured one (a trailing slash aside, OIDC Discovery §4.3), and PKCE `S256`
    /// must not be ruled out.
    pub fn parse(value: Value, configured_issuer: &str) -> Result<Self, String> {
        let document: DiscoveryJson = serde_json::from_value(value).map_err(|error| format!("not a discovery document: {error}"))?;
        if document.issuer.trim_end_matches('/') != configured_issuer.trim_end_matches('/') {
            return Err(format!("the document's issuer {:?} is not the configured {configured_issuer:?}", document.issuer));
        }
        if document.code_challenge_methods_supported.as_ref().is_some_and(|methods| !methods.iter().any(|method| method == "S256")) {
            return Err("the provider does not support PKCE with S256".to_owned());
        }
        let token_auth = match &document.token_endpoint_auth_methods_supported {
            Some(methods) if !methods.iter().any(|method| method == "client_secret_basic") => {
                if methods.iter().any(|method| method == "client_secret_post") {
                    TokenAuth::Post
                } else {
                    return Err("the provider supports neither client_secret_basic nor client_secret_post".to_owned());
                }
            }
            _ => TokenAuth::Basic,
        };
        Ok(Self {
            issuer: document.issuer,
            authorization_endpoint: document.authorization_endpoint,
            token_endpoint: document.token_endpoint,
            jwks_uri: document.jwks_uri,
            userinfo_endpoint: document.userinfo_endpoint,
            token_auth,
        })
    }
}

/// The discovery document's URL for `issuer`.
pub fn discovery_url(issuer: &str) -> String {
    format!("{}/.well-known/openid-configuration", issuer.trim_end_matches('/'))
}

// ---------------------------------------------------------------------- GitHub

/// What rbit uses from GitHub's `/user` and `/user/emails`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubUser {
    /// The numeric user id (stable across renames), as text.
    pub id: String,
    pub login: String,
    pub name: Option<String>,
    /// The primary address, if GitHub has verified it.
    pub verified_email: Option<String>,
}

/// Reads `/user` (and, for the address, `/user/emails`).
pub fn parse_github_user(user: &Value, emails: Option<&Value>) -> Result<GithubUser, String> {
    let id = match user.get("id") {
        Some(Value::Number(id)) if id.is_u64() => id.to_string(),
        _ => return Err("GitHub's /user has no numeric id".to_owned()),
    };
    let login = user.get("login").and_then(Value::as_str).ok_or("GitHub's /user has no login")?.to_owned();
    let name = user.get("name").and_then(Value::as_str).filter(|name| !name.is_empty()).map(str::to_owned);
    let verified_email = emails.and_then(Value::as_array).and_then(|list| {
        list.iter()
            .find(|entry| entry.get("primary") == Some(&Value::Bool(true)) && entry.get("verified") == Some(&Value::Bool(true)))
            .and_then(|entry| entry.get("email").and_then(Value::as_str))
            .map(str::to_owned)
    });
    Ok(GithubUser { id, login, name, verified_email })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn client(allow_private: bool) -> ProviderClient {
        ProviderClient::new(allow_private, Duration::from_secs(5)).unwrap()
    }

    #[test]
    fn endpoints_need_https_and_public_addresses() {
        assert!(client(false).endpoint("https://sso.example.com/token").is_ok());
        assert!(client(false).endpoint("http://sso.example.com/token").is_err());
        assert!(client(false).endpoint("https://127.0.0.1/token").is_err());
        assert!(client(false).endpoint("https://[::1]:8443/token").is_err());
        assert!(client(false).endpoint("ftp://sso.example.com").is_err());
        assert!(client(true).endpoint("http://127.0.0.1:9000/token").is_ok());
    }

    #[test]
    fn discovery_checks_the_issuer_and_pkce() {
        let document = |issuer: &str, extra: Value| {
            let mut value = json!({
                "issuer": issuer,
                "authorization_endpoint": "https://sso.example.com/auth",
                "token_endpoint": "https://sso.example.com/token",
                "jwks_uri": "https://sso.example.com/keys",
            });
            value.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            value
        };
        let parsed = Discovery::parse(document("https://sso.example.com/", json!({})), "https://sso.example.com").unwrap();
        assert_eq!(parsed.issuer, "https://sso.example.com/", "kept exactly, for the iss check");
        assert_eq!(parsed.token_auth, TokenAuth::Basic);
        assert!(Discovery::parse(document("https://evil.example.com", json!({})), "https://sso.example.com").is_err());
        let post = document("https://sso.example.com", json!({ "token_endpoint_auth_methods_supported": ["client_secret_post"] }));
        assert_eq!(Discovery::parse(post, "https://sso.example.com").unwrap().token_auth, TokenAuth::Post);
        let plain = document("https://sso.example.com", json!({ "code_challenge_methods_supported": ["plain"] }));
        assert!(Discovery::parse(plain, "https://sso.example.com").unwrap_err().contains("PKCE"));
        assert!(Discovery::parse(json!({ "issuer": "x" }), "x").is_err());
        assert_eq!(discovery_url("https://sso.example.com/realms/a/"), "https://sso.example.com/realms/a/.well-known/openid-configuration");
    }

    #[test]
    fn github_users_use_the_verified_primary_address() {
        let user = json!({ "id": 583_231, "login": "octocat", "name": "The Octocat", "email": "public@example.com" });
        let emails = json!([
            { "email": "old@example.com", "primary": false, "verified": true },
            { "email": "octocat@example.com", "primary": true, "verified": true },
        ]);
        let parsed = parse_github_user(&user, Some(&emails)).unwrap();
        assert_eq!(parsed.id, "583231");
        assert_eq!(parsed.login, "octocat");
        assert_eq!(parsed.verified_email.as_deref(), Some("octocat@example.com"));
        let unverified = json!([{ "email": "octocat@example.com", "primary": true, "verified": false }]);
        assert_eq!(parse_github_user(&user, Some(&unverified)).unwrap().verified_email, None);
        assert_eq!(parse_github_user(&user, None).unwrap().verified_email, None, "the public profile address is never trusted");
        assert!(parse_github_user(&json!({ "login": "x" }), None).is_err());
        assert_eq!(oauth_error(&json!({ "error": "bad_verification_code" })).as_deref(), Some("bad_verification_code"));
    }
}
