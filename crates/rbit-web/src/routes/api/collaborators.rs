//! REST API: Repository collaborators: `/repos/{owner}/{repo}/collaborators…`.
//!
//! Milestone 3 workstream A (API continuation) owns this file and
//! `openapi/collaborators.json`, the `OpenAPI` fragment documenting exactly these routes
//! (merged into the served document).
//!
//! A child of `api`, so it can use the parent's private `route()` helper, `Caller`,
//! `Pager`, `ApiError` and `ApiJson`.
//!
//! Shapes follow GitHub's. Collaborators are managed by repository administrators, so
//! every endpoint needs the `admin` scope and administrator rights: callers who cannot
//! read the repository get 404, those who can but lack either get 403 (as for branch
//! protection). The one exception: a collaborator may remove themselves. People are added
//! directly — `PUT` answers `201` with the collaborator (GitHub answers with an
//! invitation), or `204` when it only changed an existing collaborator's permission.
//! Documented deviations: the list holds direct collaborators only (GitHub's
//! `affiliation=direct`; organisation members and teams are not repeated), and the
//! permissions are rbit's three (`read`, `write`, `admin`; GitHub's `pull` and `push` are
//! accepted as their equivalents).

use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use rbit_core::{Account, RepoAccess, TokenScope, membership::Permission};
use rbit_services::{RepoContext, ServiceError, webhooks::payloads};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    ApiMethod::{Delete, Get, Put},
    ApiRoute,
    auth::Caller,
    error::{ApiError, ApiResult},
    pagination::{Paged, Pager},
    route,
};
use crate::AppState;

/// This area's endpoints, appended to [`super::table`].
pub(super) fn routes() -> Vec<ApiRoute> {
    let base = "/repos/{owner}/{repo}/collaborators";
    vec![
        route(Get, base, list),
        route(Get, format!("{base}/{{username}}"), check),
        route(Put, format!("{base}/{{username}}"), set),
        route(Delete, format!("{base}/{{username}}"), remove),
        route(Get, format!("{base}/{{username}}/permission"), permission),
    ]
}

/// The path parameters of every endpoint.
#[derive(Debug, Deserialize)]
pub struct CollaboratorPath {
    owner: String,
    repo: String,
    #[serde(default)]
    username: String,
}

/// Opens the repository (404 if hidden), then checks the token scope and the caller's
/// rights (403).
async fn open(state: &AppState, caller: &Caller, path: &CollaboratorPath) -> ApiResult<RepoContext> {
    let ctx = caller.open_repo(state, &path.owner, &path.repo).await?;
    caller.require(TokenScope::Admin)?;
    if !ctx.access.can_admin() {
        return Err(ApiError::forbidden("You must be an administrator of this repository"));
    }
    Ok(ctx)
}

/// GitHub's role name for an access level (`none` when there is none).
const fn role_name(access: RepoAccess) -> &'static str {
    match access {
        RepoAccess::None => "none",
        RepoAccess::Read => "read",
        RepoAccess::Write => "write",
        RepoAccess::Admin => "admin",
    }
}

/// A user with GitHub's `permissions` object and `role_name` for `access`.
fn collaborator_json(account: &Account, access: RepoAccess, base_url: &str) -> Value {
    let mut value = payloads::account(account, base_url);
    value["permissions"] = json!({
        "admin": access.can_admin(),
        "maintain": access.can_admin(),
        "push": access.can_write(),
        "triage": access.can_write(),
        "pull": access.can_read(),
    });
    value["role_name"] = Value::String(role_name(access).to_owned());
    value
}

/// `GET …/collaborators`: direct collaborators, by name.
pub async fn list(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(path): Path<CollaboratorPath>,
) -> ApiResult<Paged<Value>> {
    let ctx = open(&state, &caller, &path).await?;
    let listed = state.services.collaborators.list(&ctx).await?;
    let base = state.config.server.base_url();
    Ok(pager.page_of(listed.collaborators.iter().map(|entry| collaborator_json(&entry.user, entry.permission.access(), base)).collect()))
}

/// `GET …/collaborators/{username}`: `204` when the user is a direct collaborator, `404`
/// otherwise.
pub async fn check(State(state): State<AppState>, caller: Caller, Path(path): Path<CollaboratorPath>) -> ApiResult<StatusCode> {
    let ctx = open(&state, &caller, &path).await?;
    let listed = state.services.collaborators.list(&ctx).await?;
    let found = listed.collaborators.iter().any(|entry| entry.user.name.as_str().eq_ignore_ascii_case(path.username.trim()));
    if found { Ok(StatusCode::NO_CONTENT) } else { Err(ApiError::not_found()) }
}

/// The optional body of `PUT …/collaborators/{username}`.
#[derive(Debug, Default, Deserialize)]
pub struct CollaboratorBody {
    permission: Option<String>,
}

/// rbit's permission for a requested name (GitHub's `pull` and `push` included); absent
/// means `write`, GitHub's default (`push`). Teams use it for repository grants too.
pub(super) fn requested_permission(name: Option<&str>) -> ApiResult<Permission> {
    match name.map(str::trim) {
        None | Some("push") => Ok(Permission::Write),
        Some("pull") => Ok(Permission::Read),
        Some(name) => Permission::parse(name).ok_or_else(|| ApiError::invalid("permission", "permission must be read, write or admin")),
    }
}

/// `PUT …/collaborators/{username}` with an optional `{"permission"}`: `201` with the
/// collaborator when added, `204` when an existing collaborator's permission changed (or
/// already was that).
pub async fn set(State(state): State<AppState>, caller: Caller, Path(path): Path<CollaboratorPath>, body: Bytes) -> ApiResult<Response> {
    let ctx = open(&state, &caller, &path).await?;
    let user = caller.require_user()?;
    let body: CollaboratorBody = if body.iter().all(u8::is_ascii_whitespace) {
        CollaboratorBody::default()
    } else {
        serde_json::from_slice(&body).map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, format!("Problems parsing JSON: {error}")))?
    };
    let permission = requested_permission(body.permission.as_deref())?;
    let collaborators = &state.services.collaborators;
    let added = collaborators.set(user, &ctx, &path.username, permission).await.map_err(|error| ApiError::service(error, "username"))?;
    if !added {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    let (account, access) = collaborators.access_of(&ctx, &path.username).await?;
    let base = state.config.server.base_url();
    let location = format!("{base}/api/v1/repos/{}/collaborators/{}", ctx.repo.full_name(), account.name);
    Ok((StatusCode::CREATED, [(header::LOCATION, location)], Json(collaborator_json(&account, access, base))).into_response())
}

/// `DELETE …/collaborators/{username}`: `204`; administrators remove anyone, collaborators
/// themselves. Users who are not collaborators are `404`.
pub async fn remove(State(state): State<AppState>, caller: Caller, Path(path): Path<CollaboratorPath>) -> ApiResult<StatusCode> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let user = caller.require(TokenScope::Admin)?;
    match state.services.collaborators.remove(user, &ctx, &path.username).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(ServiceError::Invalid(_)) => Err(ApiError::not_found()),
        Err(ServiceError::Forbidden) => Err(ApiError::forbidden("You must be an administrator of this repository")),
        Err(other) => Err(other.into()),
    }
}

/// `GET …/collaborators/{username}/permission`: the access the user has from every grant
/// (`admin`, `write`, `read` or `none`).
pub async fn permission(State(state): State<AppState>, caller: Caller, Path(path): Path<CollaboratorPath>) -> ApiResult<Json<Value>> {
    let ctx = open(&state, &caller, &path).await?;
    let (account, access) = match state.services.collaborators.access_of(&ctx, &path.username).await {
        Ok(found) => found,
        Err(ServiceError::Invalid(_)) => return Err(ApiError::not_found()),
        Err(other) => return Err(other.into()),
    };
    let role = role_name(access);
    let user = collaborator_json(&account, access, state.config.server.base_url());
    Ok(Json(json!({ "permission": role, "role_name": role, "user": user })))
}

#[cfg(test)]
mod tests {
    use rbit_core::Visibility;

    use super::{super::test_support::Fixture, *};

    #[test]
    fn permission_names() {
        assert_eq!(requested_permission(None).unwrap(), Permission::Write, "GitHub's default is push");
        assert_eq!(requested_permission(Some("pull")).unwrap(), Permission::Read);
        assert_eq!(requested_permission(Some("push")).unwrap(), Permission::Write);
        assert_eq!(requested_permission(Some("admin")).unwrap(), Permission::Admin);
        let error = requested_permission(Some("maintain")).unwrap_err();
        assert_eq!((error.status, error.errors[0].field.as_str()), (StatusCode::UNPROCESSABLE_ENTITY, "permission"));
    }

    #[tokio::test]
    async fn collaborators_are_managed_by_administrators() {
        let fixture = Fixture::new(|_| {}).await;
        let alice = fixture.user("alice").await;
        let bob = fixture.user("bob").await;
        let carol = fixture.user("carol").await;
        fixture.repo(&alice, "demo", Visibility::Public).await;
        fixture.repo(&alice, "vault", Visibility::Private).await;
        let admin = fixture.token(&alice, TokenScope::Admin).await;
        let write = fixture.token(&alice, TokenScope::Write).await;
        let bobs = fixture.token(&bob, TokenScope::Admin).await;
        let carols = fixture.token(&carol, TokenScope::Admin).await;
        let api = "/api/v1/repos/alice/demo/collaborators";

        let (response, list) = fixture.send("GET", api, Some(&admin), None).await;
        assert_eq!((response.status(), list), (StatusCode::OK, json!([])));

        // Added with a permission: 201 with the collaborator; again: 204.
        let (response, added) = fixture.send("PUT", &format!("{api}/bob"), Some(&admin), Some(json!({ "permission": "push" }))).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{added}");
        assert_eq!(response.headers()[header::LOCATION], "https://git.example.com/api/v1/repos/alice/demo/collaborators/bob");
        assert_eq!((added["login"].as_str(), added["role_name"].as_str()), (Some("bob"), Some("write")));
        assert_eq!(added["permissions"], json!({ "admin": false, "maintain": false, "push": true, "triage": true, "pull": true }));
        let (response, _) = fixture.send("PUT", &format!("{api}/bob"), Some(&admin), Some(json!({ "permission": "admin" }))).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        // Without a body: GitHub's default (push).
        let (response, carol_entry) = fixture.raw("PUT", &format!("{api}/carol"), Some(&admin), None, None).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{carol_entry:?}");

        let (response, list) = fixture.send("GET", api, Some(&admin), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["x-total-count"], "2");
        let rows: Vec<_> = list.as_array().unwrap().iter().map(|entry| (entry["login"].clone(), entry["role_name"].clone())).collect();
        assert_eq!(rows, [(json!("bob"), json!("admin")), (json!("carol"), json!("write"))]);
        assert_eq!(fixture.send("GET", &format!("{api}/bob"), Some(&admin), None).await.0.status(), StatusCode::NO_CONTENT);
        assert_eq!(fixture.send("GET", &format!("{api}/BOB"), Some(&admin), None).await.0.status(), StatusCode::NO_CONTENT);
        assert_eq!(fixture.send("GET", &format!("{api}/alice"), Some(&admin), None).await.0.status(), StatusCode::NOT_FOUND);

        // The effective permission counts every grant.
        let (response, permission) = fixture.send("GET", &format!("{api}/alice/permission"), Some(&admin), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!((permission["permission"].as_str(), permission["user"]["login"].as_str()), (Some("admin"), Some("alice")));
        let (_, permission) = fixture.send("GET", &format!("{api}/carol/permission"), Some(&admin), None).await;
        assert_eq!(permission["permission"], "write");
        fixture.user("dave").await;
        let (_, permission) = fixture.send("GET", &format!("{api}/dave/permission"), Some(&admin), None).await;
        assert_eq!((permission["permission"].as_str(), permission["role_name"].as_str()), (Some("read"), Some("read")), "public");
        let (_, permission) = fixture.send("GET", "/api/v1/repos/alice/vault/collaborators/dave/permission", Some(&admin), None).await;
        assert_eq!(permission["permission"], "none");
        let (response, _) = fixture.send("GET", &format!("{api}/ghost/permission"), Some(&admin), None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        // Validation.
        for (who, body, field) in [
            ("dave", json!({ "permission": "maintain" }), "permission"),
            ("alice", json!({}), "username"),
            ("ghost", json!({}), "username"),
        ] {
            let (response, error) = fixture.send("PUT", &format!("{api}/{who}"), Some(&admin), Some(body)).await;
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "{who}: {error}");
            assert_eq!(error["errors"][0]["field"], field);
        }
        let (response, _) =
            fixture.raw("PUT", &format!("{api}/dave"), Some(&admin), Some("application/json"), Some(b"{nope".to_vec())).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // Scopes and rights: 403 for those who see the repository, 404 for the rest.
        assert_eq!(fixture.send("GET", api, Some(&write), None).await.0.status(), StatusCode::FORBIDDEN);
        assert_eq!(fixture.send("GET", api, Some(&carols), None).await.0.status(), StatusCode::FORBIDDEN, "a writer is no admin");
        assert_eq!(fixture.send("PUT", &format!("{api}/dave"), Some(&carols), None).await.0.status(), StatusCode::FORBIDDEN);
        assert_eq!(fixture.send("DELETE", &format!("{api}/bob"), Some(&carols), None).await.0.status(), StatusCode::FORBIDDEN);
        assert_eq!(fixture.send("GET", api, None, None).await.0.status(), StatusCode::UNAUTHORIZED);
        for path in ["/api/v1/repos/alice/vault/collaborators", "/api/v1/repos/alice/vault/collaborators/bob/permission"] {
            assert_eq!(fixture.send("GET", path, Some(&carols), None).await.0.status(), StatusCode::NOT_FOUND, "{path}");
        }
        assert_eq!(
            fixture.send("PUT", "/api/v1/repos/alice/vault/collaborators/carol", Some(&carols), None).await.0.status(),
            StatusCode::NOT_FOUND
        );

        // Bob (now admin) manages too; Carol leaves by herself; unknown entries are 404.
        assert_eq!(fixture.send("GET", api, Some(&bobs), None).await.0.status(), StatusCode::OK);
        assert_eq!(fixture.send("DELETE", &format!("{api}/carol"), Some(&carols), None).await.0.status(), StatusCode::NO_CONTENT);
        assert_eq!(fixture.send("DELETE", &format!("{api}/carol"), Some(&admin), None).await.0.status(), StatusCode::NOT_FOUND);
        assert_eq!(fixture.send("DELETE", &format!("{api}/ghost"), Some(&admin), None).await.0.status(), StatusCode::NOT_FOUND);
        assert_eq!(fixture.send("DELETE", &format!("{api}/bob"), Some(&admin), None).await.0.status(), StatusCode::NO_CONTENT);
        let (_, list) = fixture.send("GET", api, Some(&admin), None).await;
        assert_eq!(list, json!([]));
    }
}
