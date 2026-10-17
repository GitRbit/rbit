//! REST API: Organisation teams, their members and repositories: `/orgs/{org}/teams…`.
//!
//! Milestone 3 workstream A (API continuation) owns this file and `openapi/teams.json`, the
//! `OpenAPI` fragment documenting exactly these routes (merged into the served document).
//!
//! A child of `api`, so it can use the parent's private `route()` helper, `Caller`,
//! `Pager`, `ApiError` and `ApiJson`.
//!
//! Shapes follow GitHub's; a team's slug is its name (team names are URL names). Teams
//! are visible to members of the organisation and site administrators, everyone else
//! gets 404. Reading needs the `read` scope; creating, changing and deleting teams and
//! managing their members and repositories is for organisation owners with the `admin`
//! scope (other members get 403), except that members may leave a team themselves.
//! `PUT` of a membership is idempotent and answers `200` with the membership, as on
//! GitHub; repository grants answer `204`. Documented deviations: no nested teams
//! (`parent` is always `null`), no team maintainers (every `role` is `member`), and
//! permissions are rbit's three (GitHub's `pull` and `push` are accepted).

use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use rbit_core::{
    Organization, TokenScope,
    membership::{Permission, Team, TeamSummary},
    repository::OwnedRepository,
};
use rbit_services::{
    ServiceError,
    teams::{TeamContext, TeamInput},
    webhooks::payloads,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    ApiMethod::{Delete, Get, Patch, Post, Put},
    ApiRoute,
    auth::Caller,
    collaborators::requested_permission,
    error::{ApiError, ApiResult},
    json::{ApiJson, present},
    pagination::{Paged, Pager},
    route,
};
use crate::AppState;

/// This area's endpoints, appended to [`super::table`].
pub(super) fn routes() -> Vec<ApiRoute> {
    let teams = "/orgs/{org}/teams";
    let team = |suffix: &str| format!("{teams}/{{team_slug}}{suffix}");
    vec![
        route(Get, teams, list),
        route(Post, teams, create),
        route(Get, team(""), show),
        route(Patch, team(""), update),
        route(Delete, team(""), delete),
        route(Get, team("/members"), members),
        route(Get, team("/memberships/{username}"), membership),
        route(Put, team("/memberships/{username}"), add_member),
        route(Delete, team("/memberships/{username}"), remove_member),
        route(Get, team("/repos"), repositories),
        route(Get, team("/repos/{owner}/{repo}"), repository),
        route(Put, team("/repos/{owner}/{repo}"), set_repository),
        route(Delete, team("/repos/{owner}/{repo}"), remove_repository),
    ]
}

/// The path parameters of every endpoint.
#[derive(Debug, Deserialize)]
pub struct TeamPath {
    org: String,
    #[serde(default)]
    team_slug: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    owner: String,
    #[serde(default)]
    repo: String,
}

fn base(state: &AppState) -> &str {
    state.config.server.base_url()
}

/// The organisation of `path` (404 if there is none).
async fn organization(state: &AppState, caller: &Caller, path: &TeamPath) -> ApiResult<Organization> {
    Ok(state.services.orgs.open(caller.user(), &path.org).await?.org)
}

/// The team of `path` as the caller sees it (404 if hidden or unknown).
async fn open(state: &AppState, caller: &Caller, path: &TeamPath) -> ApiResult<TeamContext> {
    let org = organization(state, caller, path).await?;
    Ok(state.services.teams.open(caller.user(), &org, &path.team_slug).await?)
}

/// `{base}/api/v1/orgs/{org}/teams/{team}`.
fn team_url(org: &Organization, team: &Team, base_url: &str) -> String {
    format!("{base_url}/api/v1/orgs/{}/teams/{}", org.name(), team.name)
}

/// A team in GitHub's shape; `counts` are its members and repositories when known.
fn team_json(org: &Organization, team: &Team, counts: Option<(i64, i64)>, base_url: &str) -> Value {
    let url = team_url(org, team, base_url);
    let mut value = json!({
        "id": team.id.0,
        "name": team.name.as_str(),
        "slug": team.name.as_str(),
        "description": team.description,
        "privacy": "closed",
        "permission": "pull",
        "url": url,
        "html_url": format!("{base_url}/{}/teams/{}", org.name(), team.name),
        "members_url": format!("{url}/members{{/member}}"),
        "repositories_url": format!("{url}/repos"),
        "parent": null,
        "created_at": payloads::timestamp(team.created_at),
        "organization": payloads::account(&org.account, base_url),
    });
    if let Some((members, repos)) = counts {
        value["members_count"] = json!(members);
        value["repos_count"] = json!(repos);
    }
    value
}

fn summary_json(org: &Organization, summary: &TeamSummary, base_url: &str) -> Value {
    team_json(org, &summary.team, Some((summary.member_count, summary.repo_count)), base_url)
}

/// The full team object of single-team responses, with its counts.
async fn detail(state: &AppState, caller: &Caller, team: &TeamContext) -> ApiResult<Value> {
    let teams = state.services.teams.list(caller.user(), &team.org).await?;
    let counts = teams.iter().find(|summary| summary.team.id == team.team.id).map(|summary| (summary.member_count, summary.repo_count));
    Ok(team_json(&team.org, &team.team, counts, base(state)))
}

/// A repository granted to a team, with the team's permission.
fn team_repository_json(repo: &OwnedRepository, permission: Permission, fork: bool, base_url: &str) -> Value {
    let mut value = super::json::repository(repo, Some(permission.access()), fork, base_url);
    value["role_name"] = Value::String(permission.as_str().to_owned());
    value
}

/// GitHub's membership object.
fn membership_json(team: &TeamContext, login: &str, base_url: &str) -> Value {
    json!({ "url": format!("{}/memberships/{login}", team_url(&team.org, &team.team, base_url)), "role": "member", "state": "active" })
}

/// Validation messages say what they are about; attribute them to the closest field.
fn field_error(error: ServiceError) -> ApiError {
    let field = match &error {
        ServiceError::Invalid(message) if message.contains("description") => "description",
        _ => "name",
    };
    ApiError::service(error, field)
}

/// `GET /orgs/{org}/teams`
pub async fn list(State(state): State<AppState>, caller: Caller, pager: Pager, Path(path): Path<TeamPath>) -> ApiResult<Paged<Value>> {
    let org = organization(&state, &caller, &path).await?;
    let teams = state.services.teams.list(caller.user(), &org).await?;
    Ok(pager.page_of(teams.iter().map(|summary| summary_json(&org, summary, base(&state))).collect()))
}

/// The body of `POST` and `PATCH`.
#[derive(Debug, Deserialize)]
pub struct TeamBody {
    name: Option<String>,
    /// A string sets it, `null` clears it.
    #[serde(default, deserialize_with = "present")]
    description: Option<Value>,
}

/// `description` as given: `None` when absent.
fn description(value: Option<Value>) -> ApiResult<Option<String>> {
    match value {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(String::new())),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(ApiError::invalid("description", "description must be a string or null")),
    }
}

/// `POST /orgs/{org}/teams`: `201 Created`.
pub async fn create(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<TeamPath>,
    ApiJson(body): ApiJson<TeamBody>,
) -> ApiResult<Response> {
    let org = organization(&state, &caller, &path).await?;
    let user = caller.require(TokenScope::Admin)?;
    let name = body.name.ok_or_else(|| ApiError::invalid("name", "name is required"))?;
    let input = TeamInput { name, description: description(body.description)?.unwrap_or_default() };
    let teams = &state.services.teams;
    let team = teams.create(user, &org, input).await.map_err(field_error)?;
    let value = team_json(&org, &team, Some((0, 0)), base(&state));
    let location = team_url(&org, &team, base(&state));
    Ok((StatusCode::CREATED, [(header::LOCATION, location)], Json(value)).into_response())
}

/// `GET /orgs/{org}/teams/{team_slug}`
pub async fn show(State(state): State<AppState>, caller: Caller, Path(path): Path<TeamPath>) -> ApiResult<Json<Value>> {
    let team = open(&state, &caller, &path).await?;
    Ok(Json(detail(&state, &caller, &team).await?))
}

/// `PATCH /orgs/{org}/teams/{team_slug}`: absent fields keep their value.
pub async fn update(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<TeamPath>,
    ApiJson(body): ApiJson<TeamBody>,
) -> ApiResult<Json<Value>> {
    let team = open(&state, &caller, &path).await?;
    let user = caller.require(TokenScope::Admin)?;
    let input = TeamInput {
        name: body.name.unwrap_or_else(|| team.team.name.to_string()),
        description: description(body.description)?.unwrap_or_else(|| team.team.description.clone().unwrap_or_default()),
    };
    let teams = &state.services.teams;
    let name = teams.update(user, &team, input).await.map_err(field_error)?;
    let team = teams.open(caller.user(), &team.org, name.as_str()).await?;
    Ok(Json(detail(&state, &caller, &team).await?))
}

/// `DELETE /orgs/{org}/teams/{team_slug}`: `204`.
pub async fn delete(State(state): State<AppState>, caller: Caller, Path(path): Path<TeamPath>) -> ApiResult<StatusCode> {
    let team = open(&state, &caller, &path).await?;
    let user = caller.require(TokenScope::Admin)?;
    state.services.teams.delete(user, &team).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /orgs/{org}/teams/{team_slug}/members`
pub async fn members(State(state): State<AppState>, caller: Caller, pager: Pager, Path(path): Path<TeamPath>) -> ApiResult<Paged<Value>> {
    let team = open(&state, &caller, &path).await?;
    let members = state.services.teams.members(&team).await?;
    Ok(pager.page_of(members.iter().map(|account| payloads::account(account, base(&state))).collect()))
}

/// `GET /orgs/{org}/teams/{team_slug}/memberships/{username}`: 404 when not on the team.
pub async fn membership(State(state): State<AppState>, caller: Caller, Path(path): Path<TeamPath>) -> ApiResult<Json<Value>> {
    let team = open(&state, &caller, &path).await?;
    let members = state.services.teams.members(&team).await?;
    let member =
        members.iter().find(|account| account.name.as_str().eq_ignore_ascii_case(path.username.trim())).ok_or_else(ApiError::not_found)?;
    Ok(Json(membership_json(&team, member.name.as_str(), base(&state))))
}

/// `PUT /orgs/{org}/teams/{team_slug}/memberships/{username}`: `200` with the membership,
/// also when the user already was on the team.
pub async fn add_member(State(state): State<AppState>, caller: Caller, Path(path): Path<TeamPath>) -> ApiResult<Json<Value>> {
    let team = open(&state, &caller, &path).await?;
    let user = caller.require(TokenScope::Admin)?;
    state.services.teams.ensure_member(user, &team, &path.username).await.map_err(|error| ApiError::service(error, "username"))?;
    membership(State(state), caller, Path(path)).await
}

/// `DELETE /orgs/{org}/teams/{team_slug}/memberships/{username}`: `204`; owners remove
/// anyone, members themselves. Users who are not on the team are 404.
pub async fn remove_member(State(state): State<AppState>, caller: Caller, Path(path): Path<TeamPath>) -> ApiResult<StatusCode> {
    let team = open(&state, &caller, &path).await?;
    let user = caller.require(TokenScope::Admin)?;
    match state.services.teams.remove_member(user, &team, &path.username).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(ServiceError::Invalid(_)) => Err(ApiError::not_found()),
        Err(other) => Err(other.into()),
    }
}

/// The team's readable repositories with their permission and `fork` flags.
async fn granted(state: &AppState, caller: &Caller, team: &TeamContext) -> ApiResult<Vec<Value>> {
    let repos = state.services.teams.repositories(caller.user(), team).await?;
    let forks = state.services.forks.forks_among(&repos.iter().map(|(repo, _)| repo.repo.id).collect::<Vec<_>>()).await?;
    Ok(repos.iter().map(|(repo, permission)| team_repository_json(repo, *permission, forks.contains(&repo.repo.id), base(state))).collect())
}

/// `GET /orgs/{org}/teams/{team_slug}/repos`
pub async fn repositories(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(path): Path<TeamPath>,
) -> ApiResult<Paged<Value>> {
    let team = open(&state, &caller, &path).await?;
    Ok(pager.page_of(granted(&state, &caller, &team).await?))
}

/// `GET /orgs/{org}/teams/{team_slug}/repos/{owner}/{repo}`: the repository with the
/// team's permission; 404 when the team has no access (or the caller cannot read it).
pub async fn repository(State(state): State<AppState>, caller: Caller, Path(path): Path<TeamPath>) -> ApiResult<Json<Value>> {
    let team = open(&state, &caller, &path).await?;
    let full_name = format!("{}/{}", path.owner, path.repo);
    let found = granted(&state, &caller, &team)
        .await?
        .into_iter()
        .find(|repo| repo["full_name"].as_str().is_some_and(|name| name.eq_ignore_ascii_case(&full_name)));
    found.map(Json).ok_or_else(ApiError::not_found)
}

/// The optional body of `PUT …/repos/{owner}/{repo}`.
#[derive(Debug, Default, Deserialize)]
pub struct GrantBody {
    permission: Option<String>,
}

/// `PUT /orgs/{org}/teams/{team_slug}/repos/{owner}/{repo}` with an optional
/// `{"permission"}`: `204`.
pub async fn set_repository(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<TeamPath>,
    body: Bytes,
) -> ApiResult<StatusCode> {
    let team = open(&state, &caller, &path).await?;
    let user = caller.require(TokenScope::Admin)?;
    let body: GrantBody = if body.iter().all(u8::is_ascii_whitespace) {
        GrantBody::default()
    } else {
        serde_json::from_slice(&body).map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, format!("Problems parsing JSON: {error}")))?
    };
    let permission = requested_permission(body.permission.as_deref())?;
    let full_name = format!("{}/{}", path.owner, path.repo);
    state.services.teams.set_repository(user, &team, &full_name, permission).await.map_err(|error| ApiError::service(error, "repo"))?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /orgs/{org}/teams/{team_slug}/repos/{owner}/{repo}`: `204`; 404 when the team
/// has no access.
pub async fn remove_repository(State(state): State<AppState>, caller: Caller, Path(path): Path<TeamPath>) -> ApiResult<StatusCode> {
    let team = open(&state, &caller, &path).await?;
    let user = caller.require(TokenScope::Admin)?;
    let full_name = format!("{}/{}", path.owner, path.repo);
    match state.services.teams.remove_repository(user, &team, &full_name).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(ServiceError::Invalid(_)) => Err(ApiError::not_found()),
        Err(other) => Err(other.into()),
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{OrgRole, Visibility};
    use rbit_services::{CreateRepository, orgs::CreateOrganization};

    use super::{super::test_support::Fixture, *};

    #[tokio::test]
    #[expect(clippy::too_many_lines, reason = "one walk through the team endpoints")]
    async fn teams_members_and_repositories() {
        let fixture = Fixture::new(|_| {}).await;
        let olga = fixture.user("olga").await;
        let bob = fixture.user("bob").await;
        let eve = fixture.user("eve").await;
        let input = CreateOrganization { name: "acme".into(), ..CreateOrganization::default() };
        let acme = fixture.services.orgs.create(&olga, input).await.unwrap();
        fixture.services.orgs.add_member(&olga, &acme, "bob", OrgRole::Member).await.unwrap();
        for (name, visibility) in [("rocket", Visibility::Private), ("brochure", Visibility::Public)] {
            let input = CreateRepository { name: name.into(), description: String::new(), visibility };
            fixture.services.repositories.create_in(&olga, "acme", input).await.unwrap();
        }
        fixture.repo(&olga, "dotfiles", Visibility::Public).await;
        let olgas = fixture.token(&olga, TokenScope::Admin).await;
        let olgas_read = fixture.token(&olga, TokenScope::Read).await;
        let bobs = fixture.token(&bob, TokenScope::Admin).await;
        let eves = fixture.token(&eve, TokenScope::Admin).await;
        let api = "/api/v1/orgs/acme/teams";

        // Create, with validation.
        let (response, team) =
            fixture.send("POST", api, Some(&olgas), Some(json!({ "name": "core", "description": "The core team" }))).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{team}");
        assert_eq!(response.headers()[header::LOCATION], "https://git.example.com/api/v1/orgs/acme/teams/core");
        assert_eq!((team["slug"].as_str(), team["description"].as_str()), (Some("core"), Some("The core team")));
        assert_eq!(
            (team["html_url"].as_str(), team["organization"]["login"].as_str()),
            (Some("https://git.example.com/acme/teams/core"), Some("acme"))
        );
        assert_eq!(team["members_url"], "https://git.example.com/api/v1/orgs/acme/teams/core/members{/member}");
        for (body, field) in [
            (json!({ "name": "core" }), "name"),
            (json!({ "name": "a b" }), "name"),
            (json!({}), "name"),
            (json!({ "name": "x", "description": "d".repeat(400) }), "description"),
            (json!({ "name": "x", "description": 7 }), "description"),
        ] {
            let (response, error) = fixture.send("POST", api, Some(&olgas), Some(body.clone())).await;
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "{body}: {error}");
            assert_eq!(error["errors"][0]["field"], field, "{body}: {error}");
        }

        // Visibility and scopes: members read, owners manage, outsiders see nothing.
        let (response, list) = fixture.send("GET", api, Some(&bobs), None).await;
        assert_eq!((response.status(), list.as_array().map(Vec::len)), (StatusCode::OK, Some(1)));
        assert_eq!((list[0]["members_count"].as_i64(), list[0]["repos_count"].as_i64()), (Some(0), Some(0)));
        for path in [api.to_owned(), format!("{api}/core"), format!("{api}/core/members"), format!("{api}/core/repos")] {
            assert_eq!(fixture.send("GET", &path, Some(&eves), None).await.0.status(), StatusCode::NOT_FOUND, "{path}");
            assert_eq!(fixture.send("GET", &path, None, None).await.0.status(), StatusCode::NOT_FOUND, "anonymous {path}");
        }
        assert_eq!(fixture.send("POST", api, Some(&eves), Some(json!({ "name": "x" }))).await.0.status(), StatusCode::NOT_FOUND);
        assert_eq!(fixture.send("POST", api, Some(&bobs), Some(json!({ "name": "x" }))).await.0.status(), StatusCode::FORBIDDEN);
        assert_eq!(fixture.send("POST", api, Some(&olgas_read), Some(json!({ "name": "x" }))).await.0.status(), StatusCode::FORBIDDEN);
        assert_eq!(fixture.send("GET", &format!("{api}/nope"), Some(&olgas), None).await.0.status(), StatusCode::NOT_FOUND);
        assert_eq!(fixture.send("GET", "/api/v1/orgs/nope/teams", Some(&olgas), None).await.0.status(), StatusCode::NOT_FOUND);

        // Members: idempotent PUT, organisation members only, leave by yourself.
        let member = format!("{api}/core/memberships/bob");
        for _ in 0..2 {
            let (response, membership) = fixture.send("PUT", &member, Some(&olgas), None).await;
            assert_eq!(response.status(), StatusCode::OK, "{membership}");
            assert_eq!(membership, json!({ "url": format!("https://git.example.com{member}"), "role": "member", "state": "active" }));
        }
        let (response, error) = fixture.send("PUT", &format!("{api}/core/memberships/eve"), Some(&olgas), None).await;
        assert_eq!((response.status(), error["errors"][0]["field"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("username")));
        assert_eq!(fixture.send("PUT", &format!("{api}/core/memberships/olga"), Some(&bobs), None).await.0.status(), StatusCode::FORBIDDEN);
        let (_, members) = fixture.send("GET", &format!("{api}/core/members"), Some(&bobs), None).await;
        assert_eq!(members[0]["login"], "bob");
        assert_eq!(fixture.send("GET", &member, Some(&bobs), None).await.0.status(), StatusCode::OK);
        assert_eq!(fixture.send("GET", &format!("{api}/core/memberships/olga"), Some(&bobs), None).await.0.status(), StatusCode::NOT_FOUND);

        // Repositories: grants of the organisation's own repositories only.
        let rocket = format!("{api}/core/repos/acme/rocket");
        assert_eq!(
            fixture.send("PUT", &rocket, Some(&olgas), Some(json!({ "permission": "push" }))).await.0.status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            fixture.raw("PUT", &format!("{api}/core/repos/acme/brochure"), Some(&olgas), None, None).await.0.status(),
            StatusCode::NO_CONTENT
        );
        let (response, error) = fixture.send("PUT", &format!("{api}/core/repos/olga/dotfiles"), Some(&olgas), None).await;
        assert_eq!((response.status(), error["errors"][0]["field"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("repo")));
        let (response, error) = fixture.send("PUT", &rocket, Some(&olgas), Some(json!({ "permission": "triage" }))).await;
        assert_eq!((response.status(), error["errors"][0]["field"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("permission")));
        assert_eq!(fixture.send("PUT", &rocket, Some(&bobs), None).await.0.status(), StatusCode::FORBIDDEN);
        let (response, repos) = fixture.send("GET", &format!("{api}/core/repos"), Some(&bobs), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let rows: Vec<_> = repos.as_array().unwrap().iter().map(|repo| (repo["full_name"].clone(), repo["role_name"].clone())).collect();
        assert_eq!(rows, [(json!("acme/brochure"), json!("write")), (json!("acme/rocket"), json!("write"))]);
        assert_eq!(repos[1]["permissions"], json!({ "admin": false, "push": true, "pull": true }));
        let (response, repo) = fixture.send("GET", &rocket, Some(&bobs), None).await;
        assert_eq!((response.status(), repo["name"].as_str()), (StatusCode::OK, Some("rocket")));
        assert_eq!(
            fixture.send("GET", &format!("{api}/core/repos/olga/dotfiles"), Some(&bobs), None).await.0.status(),
            StatusCode::NOT_FOUND
        );
        // Bob can push to the private repository now.
        let ctx = fixture.services.repositories.open(Some(&bob), "acme", "rocket").await.unwrap();
        assert!(ctx.access.can_write());

        // Rename and describe; the counts follow.
        let (response, changed) = fixture.send("PATCH", &format!("{api}/core"), Some(&olgas), Some(json!({ "name": "platform" }))).await;
        assert_eq!(response.status(), StatusCode::OK, "{changed}");
        assert_eq!((changed["name"].as_str(), changed["description"].as_str()), (Some("platform"), Some("The core team")), "kept");
        assert_eq!((changed["members_count"].as_i64(), changed["repos_count"].as_i64()), (Some(1), Some(2)));
        let (_, changed) = fixture.send("PATCH", &format!("{api}/platform"), Some(&olgas), Some(json!({ "description": null }))).await;
        assert_eq!(changed["description"], Value::Null);
        assert_eq!(fixture.send("PATCH", &format!("{api}/platform"), Some(&bobs), Some(json!({}))).await.0.status(), StatusCode::FORBIDDEN);

        // Revoke, leave, delete.
        let rocket = format!("{api}/platform/repos/acme/rocket");
        assert_eq!(fixture.send("DELETE", &rocket, Some(&olgas), None).await.0.status(), StatusCode::NO_CONTENT);
        assert_eq!(fixture.send("DELETE", &rocket, Some(&olgas), None).await.0.status(), StatusCode::NOT_FOUND);
        let member = format!("{api}/platform/memberships/bob");
        assert_eq!(fixture.send("DELETE", &member, Some(&bobs), None).await.0.status(), StatusCode::NO_CONTENT);
        assert_eq!(fixture.send("DELETE", &member, Some(&olgas), None).await.0.status(), StatusCode::NOT_FOUND);
        assert_eq!(fixture.send("DELETE", &format!("{api}/platform"), Some(&bobs), None).await.0.status(), StatusCode::FORBIDDEN);
        assert_eq!(fixture.send("DELETE", &format!("{api}/platform"), Some(&olgas), None).await.0.status(), StatusCode::NO_CONTENT);
        assert_eq!(fixture.send("GET", &format!("{api}/platform"), Some(&olgas), None).await.0.status(), StatusCode::NOT_FOUND);
    }
}
