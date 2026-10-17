//! REST API: Forks: `/repos/{owner}/{repo}/forks`.
//!
//! Milestone 3 workstream A (API continuation) owns this file and `openapi/forks.json`, the
//! `OpenAPI` fragment documenting exactly these routes (merged into the served document).
//!
//! A child of `api`, so it can use the parent's private `route()` helper, `Caller`,
//! `Pager`, `ApiError` and `ApiJson`.
//!
//! Everything goes through [`rbit_services::ForkService`]: anyone who can read a
//! repository may fork it (with the `write` scope, as for creating a repository) into
//! their account or an organisation where they may create repositories. Forking is
//! synchronous, but answers `202 Accepted` like GitHub so existing clients work.
//! Documented deviations: forks are listed newest first only (no `sort`), and
//! `default_branch_only` is accepted and ignored (every branch and tag is copied).

use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use rbit_core::{TokenScope, repository::OwnedRepository};
use rbit_services::{ServiceError, forks::ForkInput};
use serde::Deserialize;
use serde_json::Value;

use super::{
    ApiMethod::{Get, Post},
    ApiRoute,
    auth::Caller,
    error::{ApiError, ApiResult},
    json,
    pagination::{Paged, Pager},
    repos, route,
};
use crate::AppState;

/// This area's endpoints, appended to [`super::table`].
pub(super) fn routes() -> Vec<ApiRoute> {
    let base = "/repos/{owner}/{repo}/forks";
    vec![route(Get, base, list), route(Post, base, create)]
}

/// `GET /repos/{owner}/{repo}/forks`: direct forks the caller may read, newest first.
pub async fn list(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path((owner, name)): Path<(String, String)>,
) -> ApiResult<Paged<Value>> {
    let ctx = caller.open_repo(&state, &owner, &name).await?;
    let limit = u32::try_from(pager.limit()).unwrap_or(u32::MAX);
    let offset = u32::try_from(pager.offset()).unwrap_or(u32::MAX);
    let forks = &state.services.forks;
    let (repos, total) = forks.page(caller.user(), &ctx, limit, offset).await?;
    let fork_ids = forks.forks_among(&repos.iter().map(|repo| repo.repo.id).collect::<Vec<_>>()).await?;
    let base = state.config.server.base_url();
    let items = repos.iter().map(|repo: &OwnedRepository| json::repository(repo, None, fork_ids.contains(&repo.repo.id), base)).collect();
    Ok(pager.paged(items, Some(total), false))
}

/// The optional body of `POST /repos/{owner}/{repo}/forks`; GitHub's
/// `default_branch_only` is accepted and ignored.
#[derive(Debug, Default, Deserialize)]
pub struct ForkBody {
    organization: Option<String>,
    name: Option<String>,
}

/// Validation messages say what they are about; attribute them to the closest field.
fn field_error(error: ServiceError) -> ApiError {
    let field = match &error {
        ServiceError::Invalid(message) if message.contains("create repositories for") => "organization",
        _ => "name",
    };
    ApiError::service(error, field)
}

/// `POST /repos/{owner}/{repo}/forks`: `202 Accepted` with the new repository (and its
/// `parent`) once it is copied.
pub async fn create(
    State(state): State<AppState>,
    caller: Caller,
    Path((owner, name)): Path<(String, String)>,
    body: Bytes,
) -> ApiResult<Response> {
    let ctx = caller.open_repo(&state, &owner, &name).await?;
    let user = caller.require(TokenScope::Write)?;
    let body: ForkBody = if body.iter().all(u8::is_ascii_whitespace) {
        ForkBody::default()
    } else {
        serde_json::from_slice(&body).map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, format!("Problems parsing JSON: {error}")))?
    };
    let input = ForkInput { owner: body.organization.unwrap_or_default(), name: body.name.unwrap_or_default() };
    let fork = state.services.forks.fork(user, &ctx, input).await.map_err(field_error)?;
    let fork = caller.open_repo(&state, fork.owner.name.as_str(), fork.repo.name.as_str()).await?;
    let value = repos::detail(&state, &caller, &fork).await?;
    let location = value["url"].as_str().unwrap_or_default().to_owned();
    Ok((StatusCode::ACCEPTED, [(header::LOCATION, location)], Json(value)).into_response())
}

#[cfg(test)]
mod tests {
    use rbit_core::{OrgRole, Visibility};
    use rbit_services::orgs::CreateOrganization;
    use serde_json::json;

    use super::{super::test_support::Fixture, *};

    #[tokio::test]
    async fn forks_are_created_and_listed() {
        let fixture = Fixture::new(|_| {}).await;
        let alice = fixture.user("alice").await;
        let bob = fixture.user("bob").await;
        let carol = fixture.user("carol").await;
        fixture.repo(&alice, "demo", Visibility::Public).await;
        fixture.repo(&alice, "vault", Visibility::Private).await;
        let input = CreateOrganization { name: "acme".into(), ..CreateOrganization::default() };
        let acme = fixture.services.orgs.create(&carol, input).await.unwrap();
        fixture.services.orgs.add_member(&carol, &acme, "bob", OrgRole::Member).await.unwrap();
        let bobs = fixture.token(&bob, TokenScope::Write).await;
        let bobs_read = fixture.token(&bob, TokenScope::Read).await;
        let carols = fixture.token(&carol, TokenScope::Write).await;
        let api = "/api/v1/repos/alice/demo/forks";

        // Without a body: into the caller's account under the same name.
        let (response, fork) = fixture.raw("POST", api, Some(&bobs), None, None).await;
        assert_eq!(response.status(), StatusCode::ACCEPTED, "{fork:?}");
        let fork: Value = serde_json::from_slice(&fork).unwrap();
        assert_eq!(response.headers()[header::LOCATION], "https://git.example.com/api/v1/repos/bob/demo");
        assert_eq!((fork["full_name"].as_str(), fork["fork"].as_bool()), (Some("bob/demo"), Some(true)));
        assert_eq!(fork["parent"]["full_name"], "alice/demo");
        assert_eq!(fork["permissions"], json!({ "admin": false, "push": true, "pull": true }), "capped by the write scope");
        // Into an organisation, renamed.
        let (response, fork) = fixture.send("POST", api, Some(&bobs), Some(json!({ "organization": "acme", "name": "demo-copy" }))).await;
        assert_eq!(response.status(), StatusCode::ACCEPTED, "{fork}");
        assert_eq!(fork["full_name"], "acme/demo-copy");

        // Refusals: a taken name, an organisation the caller is not in, a bad name, scopes, hidden parents.
        assert_eq!(fixture.send("POST", api, Some(&bobs), Some(json!({}))).await.0.status(), StatusCode::CONFLICT);
        for (body, field) in [(json!({ "organization": "alice" }), "organization"), (json!({ "name": "bad name" }), "name")] {
            let (response, error) = fixture.send("POST", api, Some(&bobs), Some(body.clone())).await;
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "{body}: {error}");
            assert_eq!(error["errors"][0]["field"], field, "{body}: {error}");
        }
        assert_eq!(
            fixture.raw("POST", api, Some(&bobs), Some("application/json"), Some(b"{".to_vec())).await.0.status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(fixture.send("POST", api, Some(&bobs_read), None).await.0.status(), StatusCode::FORBIDDEN);
        assert_eq!(fixture.send("POST", api, None, None).await.0.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(fixture.send("POST", "/api/v1/repos/alice/vault/forks", Some(&carols), None).await.0.status(), StatusCode::NOT_FOUND);
        assert_eq!(fixture.send("GET", "/api/v1/repos/alice/vault/forks", Some(&carols), None).await.0.status(), StatusCode::NOT_FOUND);

        // Listed newest first, paged, with totals.
        let (response, list) = fixture.send("GET", &format!("{api}?per_page=1"), None, None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["x-total-count"], "2");
        assert!(response.headers()[header::LINK].to_str().unwrap().contains("rel=\"next\""));
        assert_eq!((list[0]["full_name"].as_str(), list[0]["fork"].as_bool()), (Some("acme/demo-copy"), Some(true)));
        let (_, list) = fixture.send("GET", &format!("{api}?per_page=1&page=2"), None, None).await;
        assert_eq!(list[0]["full_name"], "bob/demo");

        // A private parent's forks stay private: only those who can read them see them.
        let alices = fixture.token(&alice, TokenScope::Write).await;
        let (response, _) =
            fixture.send("POST", "/api/v1/repos/alice/vault/forks", Some(&alices), Some(json!({ "name": "vault-2" }))).await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let (_, list) = fixture.send("GET", "/api/v1/repos/alice/vault/forks", Some(&alices), None).await;
        assert_eq!(list.as_array().map(Vec::len), Some(1));
    }
}
