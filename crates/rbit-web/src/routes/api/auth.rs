//! Who is calling the API: anonymous, the owner of a personal access token, or a running
//! workflow task.
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
//!
//! A workflow task's token (`rbat_…`) is a caller kind of its own, never a user
//! (`docs/design/actions.md` §11.3): it is accepted only on the endpoints of
//! [`TASK_ROUTES`] — reading contents, refs, commits and releases; creating statuses, and
//! creating, editing and deleting releases and their files with `contents: write`; commenting with `issues` or
//! `pull-requests: write` — and everywhere else answers 403. It opens its own repository at
//! its grant and other public ones read-only, writes only to its own, and is re-checked on
//! every request. With Actions disabled it is refused before anything is looked up.
#![allow(unknown_lints, clippy::unused_async_trait_impl)]

use std::sync::Arc;

use axum::{
    extract::{FromRequestParts, MatchedPath},
    http::{HeaderValue, Method, header, request::Parts},
};
use rbit_core::{RepoAccess, TokenScope, User};
use rbit_services::{RepoContext, ServiceError, ThrottleScope, actions::tokens::TaskGrant};

use super::{
    ApiMethod, PREFIX,
    error::{ApiError, ApiResult},
};
use crate::{AppState, client_ip::ClientIp};

/// The caller of an API request.
#[derive(Debug, Clone, Default)]
pub struct Caller {
    user: Option<Arc<User>>,
    scope: Option<TokenScope>,
    // Actions: S.
    /// A workflow task, and what the endpoint it calls needs of it.
    task: Option<(TaskGrant, TaskNeed)>,
    /// The request being answered, for redirects from old repository names.
    request: Arc<RequestTarget>,
}

// Actions: S.
/// What an endpoint on [`TASK_ROUTES`] needs of a task's token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskNeed {
    /// Reading: its own repository at its grant, other public ones.
    Read,
    /// `contents: write`, on its own repository.
    ContentsWrite,
    /// `issues: write` or `pull-requests: write`, on its own repository.
    Comment,
}

impl TaskNeed {
    fn allows(self, grant: &TaskGrant) -> bool {
        match self {
            Self::Read => true,
            Self::ContentsWrite => grant.contents().can_write(),
            Self::Comment => grant.can_comment(),
        }
    }

    /// Whether the endpoint changes the repository, which a task may only do to its own.
    fn writes(self) -> bool {
        !matches!(self, Self::Read)
    }
}

/// The only endpoints a workflow task's token is accepted on (§11.3), as [`super::table`]
/// spells them. Everything else answers 403 to a task.
pub const TASK_ROUTES: &[(ApiMethod, &str, TaskNeed)] = &[
    (ApiMethod::Get, "/repos/{owner}/{repo}", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/branches", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/branches/{*branch}", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/tags", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/commits", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/commits/{*reference}", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/releases", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/releases/latest", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/releases/tags/{*tag}", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/releases/{release_id}", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/releases/{release_id}/assets", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/releases/assets/{asset_id}", TaskNeed::Read),
    // A status can satisfy a required check, so a read-only (untrusted) task may not post one.
    (ApiMethod::Post, "/repos/{owner}/{repo}/statuses/{sha}", TaskNeed::ContentsWrite),
    (ApiMethod::Post, "/repos/{owner}/{repo}/releases", TaskNeed::ContentsWrite),
    (ApiMethod::Post, "/repos/{owner}/{repo}/releases/{release_id}/assets", TaskNeed::ContentsWrite),
    // Actions: releases from workflows. What publishing actions do besides creating: finish a
    // draft, replace a file, start over.
    (ApiMethod::Patch, "/repos/{owner}/{repo}/releases/{release_id}", TaskNeed::ContentsWrite),
    (ApiMethod::Delete, "/repos/{owner}/{repo}/releases/{release_id}", TaskNeed::ContentsWrite),
    (ApiMethod::Delete, "/repos/{owner}/{repo}/releases/assets/{asset_id}", TaskNeed::ContentsWrite),
    (ApiMethod::Post, "/repos/{owner}/{repo}/issues/{number}/comments", TaskNeed::Comment),
];

/// What `method path` (a route of [`super::table`]) needs of a task; `None` when tasks may
/// not call it at all.
pub fn task_route(method: ApiMethod, path: &str) -> Option<TaskNeed> {
    TASK_ROUTES.iter().find(|(allowed, template, _)| *allowed == method && *template == path).map(|(_, _, need)| *need)
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

    // Actions: task tokens.
    /// The workflow task calling, for the endpoints of [`TASK_ROUTES`] that act for one.
    pub fn task(&self) -> Option<&TaskGrant> {
        self.task.as_ref().map(|(grant, _)| grant)
    }

    /// The authenticated user; 401 for anonymous requests, 403 for a workflow task.
    pub fn require_user(&self) -> ApiResult<&User> {
        if self.task.is_some() {
            return Err(ApiError::forbidden("This endpoint does not act for workflow tasks"));
        }
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
        // Actions: S. A task: its own repository at its grant, others public and read-only.
        if let (Some((grant, need)), Some(actions)) = (&self.task, &state.services.actions) {
            let ctx = if *need == TaskNeed::Comment {
                actions.open_for_task_comment(grant, owner, name).await?
            } else {
                actions.open_for_task(grant, owner, name).await?
            };
            if need.writes() && ctx.repo.repo.id != grant.repo {
                return Err(ApiError::forbidden("A workflow task may change only its own repository"));
            }
            return Ok(ctx);
        }
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
        // Actions: foundation (F). No task token is valid while Actions is disabled, so it is
        // refused before anything is looked up.
        if rbit_core::actions::looks_like_task_token(token) {
            let grant = match &state.services.actions {
                Some(actions) => actions.authenticate_task(token).await?,
                None => None,
            };
            let Some(grant) = grant else {
                throttle.record_client_failure(ThrottleScope::ApiAuth, &address);
                return Err(ApiError::unauthorized("Bad credentials"));
            };
            // Actions: S. Only the allowlisted endpoints, with the permission they need.
            let route = parts
                .extensions
                .get::<MatchedPath>()
                .and_then(|matched| matched.as_str().strip_prefix(PREFIX))
                .zip(api_method(&parts.method));
            let Some(need) = route.and_then(|(path, method)| task_route(method, path)) else {
                return Err(ApiError::forbidden("Workflow task tokens cannot be used for this endpoint"));
            };
            if !need.allows(&grant) {
                return Err(ApiError::forbidden("This workflow task's token does not have the permission this endpoint needs"));
            }
            throttle.check(ThrottleScope::ApiRequest, &format!("task:{}", grant.task.0))?;
            return Ok(Self { task: Some((grant, need)), request, ..Self::default() });
        }
        let Some((user, scope)) = state.services.tokens.authenticate(token).await? else {
            throttle.record_client_failure(ThrottleScope::ApiAuth, &address);
            return Err(ApiError::unauthorized("Bad credentials"));
        };
        throttle.check(ThrottleScope::ApiRequest, &user.id().0.to_string())?;
        Ok(Self { user: Some(Arc::new(user)), scope: Some(scope), task: None, request })
    }
}

// Actions: S.
fn api_method(method: &Method) -> Option<ApiMethod> {
    Some(match *method {
        Method::GET => ApiMethod::Get,
        Method::POST => ApiMethod::Post,
        Method::PUT => ApiMethod::Put,
        Method::PATCH => ApiMethod::Patch,
        Method::DELETE => ApiMethod::Delete,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use rbit_core::{Account, AccountId, AccountKind, Timestamp};

    use super::*;

    // Actions: S.
    /// §11.3: every route of the table either is on the task allowlist or refuses tasks, and
    /// the allowlist names only routes that exist; the dangerous ones are not on it.
    #[test]
    fn the_task_allowlist_names_only_routes_of_the_table() {
        let mut config = rbit_core::Config::default();
        config.actions.enabled = true;
        let table = super::super::table(&config);
        for (method, path, _) in TASK_ROUTES {
            assert!(table.iter().any(|route| route.method == *method && route.path == *path), "{method:?} {path} is not a route");
        }
        let accepting: Vec<_> = table.iter().filter(|route| task_route(route.method, &route.path).is_some()).collect();
        assert_eq!(accepting.len(), TASK_ROUTES.len(), "{accepting:?}");
        for refused in ["/user", "/user/repos", "/repos/{owner}/{repo}/hooks", "/repos/{owner}/{repo}/forks", "/repos/{owner}/{repo}/pulls"]
        {
            for method in [ApiMethod::Get, ApiMethod::Post, ApiMethod::Patch, ApiMethod::Delete] {
                assert_eq!(task_route(method, refused), None, "{method:?} {refused}");
            }
        }
        assert_eq!(task_route(ApiMethod::Patch, "/repos/{owner}/{repo}"), None, "a task never changes repository settings");
        assert_eq!(task_route(ApiMethod::Post, "/repos/{owner}/{repo}/issues"), None, "nor opens issues");
    }

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
        Caller { user: Some(Arc::new(user)), scope, task: None, request: Arc::default() }
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
