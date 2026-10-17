//! Organisation teams: `/{org}/teams`, `/{org}/teams/{team}` and their settings.
//!
//! Milestone 2 workstream A (access) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. `teams` is a reserved repository name, so these
//! never shadow `/{owner}/{repo}`, and `new` is a reserved team name. Team pages are for
//! organisation members and site administrators; everyone else gets a 404. Handlers stay
//! thin: every rule lives in `TeamService`.

use axum::{
    Form, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rbit_core::{Permission, User};
use rbit_services::{
    ServiceError,
    orgs::OrgContext,
    teams::{TeamContext, TeamInput},
};
use serde::Deserialize;

use super::orgs::org_header;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    views::{
        components::FormError,
        layout::Page,
        org::{OrgHeader, OrgTab},
        teams::{NewTeamPage, TeamForm, TeamPage, TeamPageForm, TeamRow, TeamSettingsPage, TeamsPage},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{owner}/teams", get(list))
        .route("/{owner}/teams/new", get(new_form).post(create))
        .route("/{owner}/teams/{team}", get(show))
        .route("/{owner}/teams/{team}/members", post(add_member))
        .route("/{owner}/teams/{team}/members/{member}/remove", post(remove_member))
        .route("/{owner}/teams/{team}/repositories", post(set_repository))
        .route("/{owner}/teams/{team}/repositories/{repo}/remove", post(remove_repository))
        .route("/{owner}/teams/{team}/settings", get(settings).post(save_settings))
        .route("/{owner}/teams/{team}/settings/delete", post(delete))
}

#[derive(Debug, Deserialize)]
pub struct TeamFormData {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
}

impl TeamFormData {
    fn input(&self) -> TeamInput {
        TeamInput { name: self.name.clone(), description: self.description.clone() }
    }

    fn form(self) -> TeamForm {
        TeamForm { name: self.name, description: self.description }
    }
}

#[derive(Debug, Deserialize)]
pub struct MemberFormData {
    #[serde(default)]
    username: String,
}

#[derive(Debug, Deserialize)]
pub struct RepositoryFormData {
    #[serde(default)]
    name: String,
    permission: Option<String>,
}

/// The organisation, if `viewer` may see its teams; 404 otherwise.
async fn organisation(state: &AppState, viewer: Option<&User>, owner: &str) -> WebResult<OrgContext> {
    let ctx = state.services.orgs.open(viewer, owner).await?;
    if state.services.teams.can_view(viewer, &ctx.org).await? { Ok(ctx) } else { Err(WebError::NotFound) }
}

/// The organisation and the team, for a viewer who may see them.
async fn team(state: &AppState, viewer: Option<&User>, owner: &str, name: &str) -> WebResult<(OrgContext, TeamContext)> {
    let org = organisation(state, viewer, owner).await?;
    let team = state.services.teams.open(viewer, &org.org, name).await?;
    Ok((org, team))
}

/// `/{org}/teams/{team}`.
fn team_url(team: &TeamContext) -> String {
    format!("/{}/teams/{}", team.org.name(), team.team.name)
}

async fn header(state: &AppState, viewer: Option<&User>, org: &OrgContext) -> WebResult<OrgHeader> {
    org_header(state, viewer, org, OrgTab::Teams).await
}

// ------------------------------------------------------------------ list and creation

/// `GET /{org}/teams`
pub async fn list(State(state): State<AppState>, viewer: Viewer, page: Page, Path(owner): Path<String>) -> WebResult<TeamsPage> {
    let org = organisation(&state, viewer.user(), &owner).await?;
    let teams = state.services.teams.list(viewer.user(), &org.org).await?;
    let name = org.org.name().to_string();
    Ok(TeamsPage {
        page: page.titled(format!("Teams · {}", org.org.label())),
        header: header(&state, viewer.user(), &org).await?,
        teams: teams.iter().map(|summary| TeamRow::new(summary, &name)).collect(),
        can_manage: org.can_manage,
        new_url: format!("/{name}/teams/new"),
    })
}

async fn new_team_page(state: &AppState, user: &User, page: Page, org: &OrgContext, form: TeamForm) -> WebResult<NewTeamPage> {
    Ok(NewTeamPage {
        page: page.titled(format!("New team · {}", org.org.label())),
        header: header(state, Some(user), org).await?,
        teams_url: format!("/{}/teams", org.org.name()),
        form,
        error: FormError::default(),
    })
}

/// `GET /{org}/teams/new`: owners only.
pub async fn new_form(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path(owner): Path<String>,
) -> WebResult<NewTeamPage> {
    let org = organisation(&state, Some(&user), &owner).await?;
    if !org.can_manage {
        return Err(WebError::NotFound);
    }
    new_team_page(&state, &user, page, &org, TeamForm::default()).await
}

/// `POST /{org}/teams/new`
pub async fn create(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path(owner): Path<String>,
    Form(form): Form<TeamFormData>,
) -> WebResult<Response> {
    let org = organisation(&state, Some(&user), &owner).await?;
    match state.services.teams.create(&user, &org.org, form.input()).await {
        Ok(team) => Ok(Redirect::to(&format!("/{}/teams/{}", org.org.name(), team.name)).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let mut page = new_team_page(&state, &user, page, &org, form.form()).await?;
            page.error = FormError::from_message(&message);
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

// ------------------------------------------------------------------ team page

async fn team_page(state: &AppState, viewer: Option<&User>, page: Page, org: &OrgContext, team: &TeamContext) -> WebResult<TeamPage> {
    let members = state.services.teams.members(team).await?;
    let repositories = state.services.teams.repositories(viewer, team).await?;
    let viewer_name = viewer.map(|user| user.name().to_string()).unwrap_or_default();
    Ok(TeamPage::new(page, header(state, viewer, org).await?, team, &members, &repositories, &viewer_name))
}

/// Redirects back to the team page after a change, or re-renders it with the message
/// above `form` and a 422.
#[expect(clippy::too_many_arguments, reason = "the parts of one re-rendered page")]
async fn after_change(
    state: &AppState,
    user: &User,
    page: Page,
    org: &OrgContext,
    team: &TeamContext,
    result: Result<(), ServiceError>,
    form: TeamPageForm,
    submitted: (&str, Permission),
) -> WebResult<Response> {
    match result {
        Ok(()) => Ok(Redirect::to(&team_url(team)).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let page = team_page(state, Some(user), page, org, team).await?.rejected(
                form,
                FormError::from_message(&message),
                submitted.0,
                submitted.1,
            );
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// `GET /{org}/teams/{team}`
pub async fn show(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, name)): Path<(String, String)>,
) -> WebResult<TeamPage> {
    let (org, team) = team(&state, viewer.user(), &owner, &name).await?;
    team_page(&state, viewer.user(), page, &org, &team).await
}

/// `POST /{org}/teams/{team}/members`
pub async fn add_member(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path((owner, name)): Path<(String, String)>,
    Form(form): Form<MemberFormData>,
) -> WebResult<Response> {
    let (org, team) = team(&state, Some(&user), &owner, &name).await?;
    let result = state.services.teams.add_member(&user, &team, &form.username).await;
    after_change(&state, &user, page, &org, &team, result, TeamPageForm::Members, (&form.username, Permission::Write)).await
}

/// `POST /{org}/teams/{team}/members/{member}/remove`: owners remove anyone, members
/// themselves (and are then sent to the team list).
pub async fn remove_member(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path((owner, name, member)): Path<(String, String, String)>,
) -> WebResult<Response> {
    let (org, team) = team(&state, Some(&user), &owner, &name).await?;
    let result = state.services.teams.remove_member(&user, &team, &member).await;
    if result.is_ok() && member.eq_ignore_ascii_case(user.name().as_str()) && !team.can_manage {
        return Ok(Redirect::to(&format!("/{}/teams", org.org.name())).into_response());
    }
    after_change(&state, &user, page, &org, &team, result, TeamPageForm::Members, ("", Permission::Write)).await
}

/// `POST /{org}/teams/{team}/repositories`: grant a repository, or change the permission.
pub async fn set_repository(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path((owner, name)): Path<(String, String)>,
    Form(form): Form<RepositoryFormData>,
) -> WebResult<Response> {
    let (org, team) = team(&state, Some(&user), &owner, &name).await?;
    let permission = form.permission.as_deref().and_then(Permission::parse);
    let result = match permission {
        Some(permission) => state.services.teams.set_repository(&user, &team, &form.name, permission).await,
        None => Err(ServiceError::Invalid("choose a permission".to_owned())),
    };
    let submitted = (form.name.as_str(), permission.unwrap_or(Permission::Write));
    after_change(&state, &user, page, &org, &team, result, TeamPageForm::Repositories, submitted).await
}

/// `POST /{org}/teams/{team}/repositories/{repo}/remove`
pub async fn remove_repository(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path((owner, name, repo)): Path<(String, String, String)>,
) -> WebResult<Response> {
    let (org, team) = team(&state, Some(&user), &owner, &name).await?;
    let result = state.services.teams.remove_repository(&user, &team, &repo).await;
    after_change(&state, &user, page, &org, &team, result, TeamPageForm::Repositories, ("", Permission::Write)).await
}

// ------------------------------------------------------------------ settings

/// The team, if `user` may manage it; 404 for everyone else.
async fn managed(state: &AppState, user: &User, owner: &str, name: &str) -> WebResult<(OrgContext, TeamContext)> {
    let (org, team) = team(state, Some(user), owner, name).await?;
    if team.can_manage { Ok((org, team)) } else { Err(WebError::NotFound) }
}

/// `GET /{org}/teams/{team}/settings`
pub async fn settings(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path((owner, name)): Path<(String, String)>,
) -> WebResult<TeamSettingsPage> {
    let (org, team) = managed(&state, &user, &owner, &name).await?;
    Ok(TeamSettingsPage::new(page, header(&state, Some(&user), &org).await?, &team))
}

/// `POST /{org}/teams/{team}/settings`: rename and describe.
pub async fn save_settings(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path((owner, name)): Path<(String, String)>,
    Form(form): Form<TeamFormData>,
) -> WebResult<Response> {
    let (org, team) = managed(&state, &user, &owner, &name).await?;
    match state.services.teams.update(&user, &team, form.input()).await {
        Ok(new_name) => Ok(Redirect::to(&format!("/{}/teams/{new_name}", org.org.name())).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let mut page = TeamSettingsPage::new(page, header(&state, Some(&user), &org).await?, &team);
            page.form = form.form();
            page.error = FormError::from_message(&message);
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// `POST /{org}/teams/{team}/settings/delete`
pub async fn delete(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, name)): Path<(String, String)>,
) -> WebResult<Redirect> {
    let (org, team) = managed(&state, &user, &owner, &name).await?;
    state.services.teams.delete(&user, &team).await?;
    Ok(Redirect::to(&format!("/{}/teams", org.org.name())))
}
