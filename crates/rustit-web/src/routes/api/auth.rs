//! Who is calling the API: anonymous, or the owner of a personal access token.
//!
//! Tokens arrive as `Authorization: Bearer rit_…` (or GitHub's `token rit_…`); cookies are
//! never read, so the API needs no CSRF protection and a browser session cannot be
//! ridden. Presented tokens are throttled per client address (`ApiAuth`; invalid ones cost
//! extra), authenticated requests per account and anonymous ones per address
//! (`ApiRequest`). Expired tokens and tokens of suspended accounts are refused like
//! unknown ones. With `[api] enabled = false` every request is answered 404 here.
#![allow(clippy::unused_async_trait_impl)]

use std::sync::Arc;

use axum::{
    extract::FromRequestParts,
    http::{HeaderValue, header, request::Parts},
};
use rustit_core::{RepoAccess, TokenScope, User};
use rustit_services::{RepoContext, ThrottleScope};

use super::error::{ApiError, ApiResult};
use crate::{AppState, client_ip::ClientIp};

/// The caller of an API request.
#[derive(Debug, Clone, Default)]
pub struct Caller {
    user: Option<Arc<User>>,
    scope: Option<TokenScope>,
}

impl Caller {
    pub fn user(&self) -> Option<&User> {
        self.user.as_deref()
    }

    /// The authenticated user; 401 for anonymous requests.
    pub fn require_user(&self) -> ApiResult<&User> {
        self.user().ok_or_else(|| ApiError::unauthorized("Requires authentication"))
    }

    /// The authenticated user, if their token's scope includes `scope`; 401 for anonymous
    /// requests and 403 for a token with a narrower scope.
    pub fn require(&self, scope: TokenScope) -> ApiResult<&User> {
        let user = self.require_user()?;
        match self.scope {
            Some(granted) if !granted.allows(scope) => Err(ApiError::forbidden(format!(
                "This token's scope ({}) does not allow this; it needs the {} scope",
                granted.as_str(),
                scope.as_str()
            ))),
            _ => Ok(user),
        }
    }

    /// Caps what the caller may do with a repository to the token's scope.
    pub fn limit(&self, access: RepoAccess) -> RepoAccess {
        self.scope.map_or(access, |scope| scope.limit(access))
    }

    /// Opens `owner/name` as the caller sees it, with access capped by the token's scope.
    /// Repositories the caller cannot read are 404.
    pub async fn open_repo(&self, state: &AppState, owner: &str, name: &str) -> ApiResult<RepoContext> {
        let mut ctx = state.services.repositories.open(self.user(), owner, name).await?;
        ctx.access = self.limit(ctx.access);
        Ok(ctx)
    }
}

/// The token in an `Authorization` header: `Bearer <token>` or `token <token>`.
pub fn presented_token(value: &HeaderValue) -> Option<&str> {
    let value = value.to_str().ok()?.trim();
    let (scheme, token) = value.split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") || scheme.eq_ignore_ascii_case("token")).then_some(token).filter(|token| !token.is_empty())
}

impl FromRequestParts<AppState> for Caller {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        if !state.config.api.enabled {
            return Err(ApiError::not_found());
        }
        let ClientIp(ip) = ClientIp::resolve(&parts.extensions, &parts.headers, &state.config.security);
        let address = ip.to_canonical().to_string();
        let throttle = &state.services.throttle;

        let Some(authorization) = parts.headers.get(header::AUTHORIZATION) else {
            throttle.check_client(ThrottleScope::ApiRequest, &address)?;
            return Ok(Self::default());
        };
        throttle.check_client(ThrottleScope::ApiAuth, &address)?;
        let Some(token) = presented_token(authorization) else {
            throttle.record_client_failure(ThrottleScope::ApiAuth, &address);
            return Err(ApiError::unauthorized("Use Authorization: Bearer <personal access token>"));
        };
        let Some((user, scope)) = state.services.tokens.authenticate(token).await? else {
            throttle.record_client_failure(ThrottleScope::ApiAuth, &address);
            return Err(ApiError::unauthorized("Bad credentials"));
        };
        throttle.check(ThrottleScope::ApiRequest, &user.id().0.to_string())?;
        Ok(Self { user: Some(Arc::new(user)), scope: Some(scope) })
    }
}

#[cfg(test)]
mod tests {
    use rustit_core::{Account, AccountId, AccountKind, Timestamp};

    use super::*;

    fn caller(scope: Option<TokenScope>) -> Caller {
        let account = Account {
            id: AccountId(1),
            kind: AccountKind::User,
            name: "ada".parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(0),
        };
        let user = User { account, email: "ada@example.com".parse().unwrap(), is_admin: false };
        Caller { user: Some(Arc::new(user)), scope }
    }

    #[test]
    fn reads_bearer_and_token_schemes() {
        let token = |value: &'static str| presented_token(&HeaderValue::from_static(value)).map(str::to_owned);
        assert_eq!(token("Bearer rit_abc").as_deref(), Some("rit_abc"));
        assert_eq!(token("bearer   rit_abc ").as_deref(), Some("rit_abc"));
        assert_eq!(token("token rit_abc").as_deref(), Some("rit_abc"));
        assert_eq!(token("Basic YWxpY2U6cGFzcw=="), None);
        assert_eq!(token("Bearer"), None);
        assert_eq!(token("Bearer "), None);
    }

    #[test]
    fn scopes_gate_actions_and_cap_access() {
        let read = caller(Some(TokenScope::Read));
        assert!(read.require(TokenScope::Read).is_ok());
        let refused = read.require(TokenScope::Write).unwrap_err();
        assert_eq!(refused.status, axum::http::StatusCode::FORBIDDEN);
        assert!(refused.message.contains("needs the write scope"));
        assert_eq!(read.limit(RepoAccess::Admin), RepoAccess::Read);

        let admin = caller(Some(TokenScope::Admin));
        assert!(admin.require(TokenScope::Admin).is_ok());
        assert_eq!(admin.limit(RepoAccess::Write), RepoAccess::Write, "never above the owner's access");

        let anonymous = Caller::default();
        assert_eq!(anonymous.require(TokenScope::Read).unwrap_err().status, axum::http::StatusCode::UNAUTHORIZED);
        assert_eq!(anonymous.limit(RepoAccess::Read), RepoAccess::Read);
    }
}
