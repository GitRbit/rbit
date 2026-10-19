//! Who is calling the API: anonymous, or the owner of a personal access token.
//!
//! Tokens arrive as `Authorization: Bearer rbit_…` (or GitHub's `token rbit_…`; the legacy
//! `rit_…` spelling works too); cookies are never read, so the API needs no CSRF
//! protection and a browser session cannot be ridden. Presented tokens are throttled per client address (`ApiAuth`; invalid ones cost
//! extra), authenticated requests per account and anonymous ones per address
//! (`ApiRequest`). Expired tokens and tokens of suspended accounts are refused like
//! unknown ones. With `[api] enabled = false` every request is answered 404 here.
//!
//! Repositories are opened by their current name; an old name of a repository the caller
//! may read (it was renamed or transferred) is answered with a redirect to the same
//! endpoint under the new name, as GitHub does ([`ApiError::moved`]). A repository that
//! now has the old name wins, and repositories the caller may not read stay 404, so a
//! new name never leaks.
use std::sync::Arc;

use axum::{
    extract::FromRequestParts,
    http::{HeaderValue, Method, header, request::Parts},
};
use rbit_core::{RepoAccess, TokenScope, User};
use rbit_services::{RepoContext, ServiceError, ThrottleScope};

use super::error::{ApiError, ApiResult};
use crate::{AppState, client_ip::ClientIp};

/// The caller of an API request.
#[derive(Debug, Clone, Default)]
pub struct Caller {
    user: Option<Arc<User>>,
    scope: Option<TokenScope>,
    /// The request being answered, for redirects from old repository names.
    request: Arc<RequestTarget>,
}

/// Method, path and query of a request.
#[derive(Debug, Clone, Default)]
struct RequestTarget {
    method: Method,
    path: String,
    query: Option<String>,
}

impl RequestTarget {
    /// This request's URL with the path segments `owner/name` (right after `repos/` or
    /// `user/starred/`) replaced by `moved_to`, the current full name; `None` if the path
    /// does not name `owner/name` there.
    fn moved(&self, base_url: &str, owner: &str, name: &str, moved_to: &str) -> Option<String> {
        let segments: Vec<&str> = self.path.split('/').collect();
        let at = (1..segments.len().saturating_sub(1)).find(|&i| {
            matches!(segments[i - 1], "repos" | "starred")
                && segments[i].eq_ignore_ascii_case(owner)
                && segments[i + 1].eq_ignore_ascii_case(name)
        })?;
        let mut path = segments[..at].join("/");
        path.push('/');
        path.push_str(moved_to);
        for rest in &segments[at + 2..] {
            path.push('/');
            path.push_str(rest);
        }
        let query = self.query.as_deref().map(|query| format!("?{query}")).unwrap_or_default();
        Some(format!("{base_url}{path}{query}"))
    }
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
    /// Repositories the caller cannot read are 404; an old name of one they can read is a
    /// redirect to this endpoint under its current name.
    pub async fn open_repo(&self, state: &AppState, owner: &str, name: &str) -> ApiResult<RepoContext> {
        let repositories = &state.services.repositories;
        match repositories.open(self.user(), owner, name).await {
            Ok(mut ctx) => {
                ctx.access = self.limit(ctx.access);
                Ok(ctx)
            }
            Err(ServiceError::NotFound) => {
                let moved = repositories.moved_to(self.user(), owner, name).await?;
                let location = moved.and_then(|repo| self.request.moved(state.config.server.base_url(), owner, name, &repo.full_name()));
                Err(location.map_or_else(ApiError::not_found, |location| ApiError::moved(&self.request.method, location)))
            }
            Err(error) => Err(error.into()),
        }
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
        let request = Arc::new(RequestTarget {
            method: parts.method.clone(),
            path: parts.uri.path().to_owned(),
            query: parts.uri.query().map(str::to_owned),
        });
        let ClientIp(ip) = ClientIp::resolve(&parts.extensions, &parts.headers, &state.config.security);
        let address = ip.to_canonical().to_string();
        let throttle = &state.services.throttle;

        let Some(authorization) = parts.headers.get(header::AUTHORIZATION) else {
            throttle.check_client(ThrottleScope::ApiRequest, &address)?;
            return Ok(Self { request, ..Self::default() });
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
        Ok(Self { user: Some(Arc::new(user)), scope: Some(scope), request })
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{Account, AccountId, AccountKind, Timestamp};

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
        Caller { user: Some(Arc::new(user)), scope, request: Arc::default() }
    }

    #[test]
    fn old_repository_names_are_replaced_in_place() {
        let target = |method: Method, path: &str, query: Option<&str>| RequestTarget {
            method,
            path: path.to_owned(),
            query: query.map(str::to_owned),
        };
        let base = "https://git.example.com";
        let commits = target(Method::GET, "/api/v1/repos/Alice/Before/commits/main", Some("per_page=1"));
        assert_eq!(
            commits.moved(base, "alice", "before", "acme/after").as_deref(),
            Some("https://git.example.com/api/v1/repos/acme/after/commits/main?per_page=1")
        );
        let star = target(Method::PUT, "/api/v1/user/starred/alice/before", None);
        assert_eq!(
            star.moved(base, "alice", "before", "alice/after").as_deref(),
            Some("https://git.example.com/api/v1/user/starred/alice/after")
        );
        assert_eq!(star.moved(base, "bob", "before", "bob/after"), None);
        // Only the repository's own segments change, not look-alikes further along.
        let branch = target(Method::GET, "/api/v1/repos/repos/x/branches/repos/x", None);
        assert_eq!(branch.moved(base, "repos", "x", "a/b").as_deref(), Some("https://git.example.com/api/v1/repos/a/b/branches/repos/x"));
    }

    #[test]
    fn reads_bearer_and_token_schemes() {
        let token = |value: &'static str| presented_token(&HeaderValue::from_static(value)).map(str::to_owned);
        assert_eq!(token("Bearer rbit_abc").as_deref(), Some("rbit_abc"));
        assert_eq!(token("bearer   rbit_abc ").as_deref(), Some("rbit_abc"));
        assert_eq!(token("token rbit_abc").as_deref(), Some("rbit_abc"));
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
