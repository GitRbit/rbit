//! Organisations: `/organizations/new`, the `/{org}` profile (via [`profile`]),
//! `/{org}/members` and `/{org}/settings` (profile, base permission, deletion). Team
//! pages are in `routes::teams`.
//!
//! Its routes are merged into the page router, so they get sessions, CSRF protection,
//! security headers and error pages automatically. Handlers stay thin: every rule
//! (who may manage, the last owner, deletion) lives in `OrgService`.

use axum::{
    Form, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rbit_core::{BasePermission, OrgRole, User};
use rbit_services::{
    ServiceError,
    orgs::{CreateOrganization, OrgContext, UpdateOrganization},
};
use serde::Deserialize;

use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    views::{
        components::{FormError, RepoCard},
        layout::Page,
        org::{
            AddMemberForm, BasePermissionOption, MemberRow, MembersPage, NewOrgPage, OrgHeader, OrgProfileForm, OrgProfilePage,
            OrgSettingsPage, OrgTab,
        },
    },
};

/// Routes for organisation pages, merged into the page router. `members` and `settings`
/// are reserved repository names, so these never shadow `/{owner}/{repo}`.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/organizations/new", get(new_form).post(create))
        .route("/{owner}/members", get(members).post(add_member))
        .route("/{owner}/members/leave", post(leave))
        .route("/{owner}/members/{member}/role", post(change_role))
        .route("/{owner}/members/{member}/remove", post(remove_member))
        .route("/{owner}/settings", get(settings).post(save_settings))
        .route("/{owner}/settings/delete", post(delete))
        .route("/{owner}/settings/permission", post(save_permission))
}

/// Maps validation failures to `Some(message)` (re-render with 422) and passes every
/// other error on (404, 403, 500 pages).
fn validation_message(error: ServiceError) -> WebResult<String> {
    match error {
        ServiceError::Invalid(message) => Ok(message),
        conflict @ ServiceError::Conflict(_) => Ok(conflict.to_string()),
        other => Err(other.into()),
    }
}

/// The header with repository and member counts as `viewer` sees them.
async fn header(state: &AppState, viewer: Option<&User>, ctx: &OrgContext, tab: OrgTab) -> WebResult<(OrgHeader, Vec<RepoCard>)> {
    let repos = state.services.repositories.list_for_owner(viewer, &ctx.org.account).await?;
    let members = state.services.orgs.members(&ctx.org).await?;
    let header = OrgHeader::new(ctx, tab).with_counts(Some(repos.len()), Some(members.len()));
    Ok((header, super::listing::repo_cards(state, &repos).await?))
}

/// Just the header (for pages that do not list repositories, such as the team pages).
pub(super) async fn org_header(state: &AppState, viewer: Option<&User>, ctx: &OrgContext, tab: OrgTab) -> WebResult<OrgHeader> {
    let repos = state.services.repositories.list_for_owner(viewer, &ctx.org.account).await?;
    let members = state.services.orgs.members(&ctx.org).await?;
    Ok(OrgHeader::new(ctx, tab).with_counts(Some(repos.len()), Some(members.len())))
}

// ------------------------------------------------------------------ creation

#[derive(Debug, Deserialize)]
pub struct NewOrgForm {
    #[serde(default)]
    name: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    description: String,
}

/// `GET /organizations/new`
pub async fn new_form(State(state): State<AppState>, SignedIn(user): SignedIn, page: Page) -> WebResult<NewOrgPage> {
    if !state.services.orgs.can_create(&user) {
        return Err(WebError::Forbidden);
    }
    Ok(NewOrgPage {
        page: page.titled("New organization"),
        error: FormError::default(),
        name: String::new(),
        display_name: String::new(),
        description: String::new(),
    })
}

/// `POST /organizations/new`
pub async fn create(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Form(form): Form<NewOrgForm>,
) -> WebResult<Response> {
    let input =
        CreateOrganization { name: form.name.clone(), display_name: form.display_name.clone(), description: form.description.clone() };
    match state.services.orgs.create(&user, input).await {
        Ok(org) => Ok(Redirect::to(&format!("/{}", org.name())).into_response()),
        Err(error) => {
            let message = validation_message(error)?;
            let page = NewOrgPage {
                page: page.titled("New organization"),
                error: FormError::from_message(&message),
                name: form.name,
                display_name: form.display_name,
                description: form.description,
            };
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
    }
}

// ------------------------------------------------------------------ profile

/// `/{org}`: called by the account profile route once it knows the name is an organisation.
pub async fn profile(
    state: &AppState,
    viewer: &Viewer,
    page: Page,
    name: &str,
    year: Option<i32>,
    calendar_request: Option<&str>,
) -> WebResult<Response> {
    let user = viewer.user();
    let ctx = state.services.orgs.open(user, name).await?;
    let calendar = crate::views::contributions::ContributionCalendar::new(year, "Commits across this organization's default branches.", "");
    let (start, end) = calendar.bounds();
    let contributions = if calendar_request.is_some() {
        calendar.populate(&state.services.contributions.account(user, &ctx.org.account, start, end).await?)
    } else {
        calendar.deferred()
    };
    if calendar_request == Some("fragment") {
        return Ok(crate::views::contributions::ContributionFragment { contributions }.into_response());
    }
    let (header, repos) = header(state, user, &ctx, OrgTab::Repositories).await?;
    Ok(OrgProfilePage {
        contributions,
        page: page.titled(ctx.org.label().to_owned()),
        can_create_repo: ctx.is_member() || user.is_some_and(|u| u.is_admin),
        new_repo_url: format!("/new?owner={}", ctx.org.name()),
        header,
        repos,
    }
    .into_response())
}

// ------------------------------------------------------------------ members

#[derive(Debug, Deserialize)]
pub struct AddMemberFormData {
    #[serde(default)]
    username: String,
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RoleFormData {
    role: Option<String>,
}

/// Renders the members page; `error` and `form` carry a failed submission.
async fn members_page(
    state: &AppState,
    viewer: Option<&User>,
    page: Page,
    ctx: &OrgContext,
    error: FormError,
    form: AddMemberForm,
) -> WebResult<MembersPage> {
    let (header, _) = header(state, viewer, ctx, OrgTab::Members).await?;
    let members = state.services.orgs.members(&ctx.org).await?;
    let base = format!("/{}/members", ctx.org.name());
    Ok(MembersPage {
        page: page.titled(format!("Members · {}", ctx.org.label())),
        header,
        members: members.iter().map(|member| MemberRow::new(&ctx.org, member, viewer)).collect(),
        error,
        form,
        can_manage: ctx.can_manage,
        is_member: ctx.is_member(),
        leave_url: format!("{base}/leave"),
        add_url: base,
    })
}

/// Re-renders the members page with `message` and a 422, or redirects back on success.
async fn after_member_change(
    state: &AppState,
    user: &User,
    page: Page,
    ctx: &OrgContext,
    result: Result<(), ServiceError>,
    form: AddMemberForm,
) -> WebResult<Response> {
    match result {
        Ok(()) => Ok(Redirect::to(&format!("/{}/members", ctx.org.name())).into_response()),
        Err(error) => {
            let message = validation_message(error)?;
            // Reload: the change may have been partly visible to a concurrent request.
            let ctx = state.services.orgs.open(Some(user), ctx.org.name().as_str()).await?;
            let page = members_page(state, Some(user), page, &ctx, FormError::from_message(&message), form).await?;
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
    }
}

/// `GET /{org}/members`: public, like the member list on the profile.
pub async fn members(State(state): State<AppState>, viewer: Viewer, page: Page, Path(owner): Path<String>) -> WebResult<MembersPage> {
    let ctx = state.services.orgs.open(viewer.user(), &owner).await?;
    members_page(&state, viewer.user(), page, &ctx, FormError::default(), AddMemberForm::default()).await
}

/// `POST /{org}/members`: add a member.
pub async fn add_member(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path(owner): Path<String>,
    Form(form): Form<AddMemberFormData>,
) -> WebResult<Response> {
    let ctx = state.services.orgs.open(Some(&user), &owner).await?;
    let role = form.role.as_deref().and_then(OrgRole::parse);
    let result = match role {
        Some(role) => state.services.orgs.add_member(&user, &ctx.org, &form.username, role).await,
        None => Err(ServiceError::Invalid("choose a role".to_owned())),
    };
    let submitted = AddMemberForm { username: form.username, owner: role.is_some_and(OrgRole::is_owner) };
    after_member_change(&state, &user, page, &ctx, result, submitted).await
}

/// `POST /{org}/members/{member}/role`
pub async fn change_role(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path((owner, member)): Path<(String, String)>,
    Form(form): Form<RoleFormData>,
) -> WebResult<Response> {
    let ctx = state.services.orgs.open(Some(&user), &owner).await?;
    let result = match form.role.as_deref().and_then(OrgRole::parse) {
        Some(role) => state.services.orgs.change_role(&user, &ctx.org, &member, role).await,
        None => Err(ServiceError::Invalid("choose a role".to_owned())),
    };
    after_member_change(&state, &user, page, &ctx, result, AddMemberForm::default()).await
}

/// `POST /{org}/members/{member}/remove`
pub async fn remove_member(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path((owner, member)): Path<(String, String)>,
) -> WebResult<Response> {
    let ctx = state.services.orgs.open(Some(&user), &owner).await?;
    let result = state.services.orgs.remove_member(&user, &ctx.org, &member).await;
    after_member_change(&state, &user, page, &ctx, result, AddMemberForm::default()).await
}

/// `POST /{org}/members/leave`: the viewer leaves; back to the dashboard.
pub async fn leave(State(state): State<AppState>, SignedIn(user): SignedIn, page: Page, Path(owner): Path<String>) -> WebResult<Response> {
    let ctx = state.services.orgs.open(Some(&user), &owner).await?;
    match state.services.orgs.leave(&user, &ctx.org).await {
        Ok(()) => Ok(Redirect::to("/").into_response()),
        Err(error) => after_member_change(&state, &user, page, &ctx, Err(error), AddMemberForm::default()).await,
    }
}

// ------------------------------------------------------------------ settings

#[derive(Debug, Deserialize)]
pub struct ProfileFormData {
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    website: String,
}

#[derive(Debug, Deserialize)]
pub struct DeleteFormData {
    #[serde(default)]
    confirm: String,
}

/// The organisation, if `user` may manage it. Everyone else gets a 404, so settings
/// pages reveal nothing to non-owners.
async fn managed(state: &AppState, user: &User, owner: &str) -> WebResult<OrgContext> {
    let ctx = state.services.orgs.open(Some(user), owner).await?;
    if ctx.can_manage { Ok(ctx) } else { Err(WebError::NotFound) }
}

async fn settings_page(state: &AppState, user: &User, page: Page, ctx: &OrgContext, form: OrgProfileForm) -> WebResult<OrgSettingsPage> {
    let (header, repos) = header(state, Some(user), ctx, OrgTab::Settings).await?;
    let base = format!("/{}/settings", ctx.org.name());
    let base_permission = state.services.orgs.base_permission(&ctx.org).await?;
    let mut cards = crate::views::org::org_settings_cards(&base);
    // Actions: interface and secrets (U). Only when the instance has Actions.
    if state.services.actions.is_some() {
        cards.extend(crate::views::actions_settings::org_settings_cards(&base));
    }
    Ok(OrgSettingsPage {
        page: page.titled(format!("Settings · {}", ctx.org.label())),
        header,
        form,
        error: FormError::default(),
        delete_error: FormError::default(),
        repo_count: repos.len(),
        delete_url: format!("{base}/delete"),
        cards,
        permission_url: format!("{base}/permission"),
        base_permissions: BasePermissionOption::list(base_permission),
        permission_error: FormError::default(),
        save_url: base,
    })
}

#[derive(Debug, Deserialize)]
pub struct BasePermissionFormData {
    permission: Option<String>,
}

/// `POST /{org}/settings/permission`: change what every member gets.
pub async fn save_permission(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path(owner): Path<String>,
    Form(form): Form<BasePermissionFormData>,
) -> WebResult<Response> {
    let ctx = managed(&state, &user, &owner).await?;
    let Some(permission) = form.permission.as_deref().and_then(BasePermission::parse) else {
        let mut page = settings_page(&state, &user, page, &ctx, OrgProfileForm::from_org(&ctx.org)).await?;
        page.permission_error = FormError::new("Choose a permission.");
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response());
    };
    state.services.orgs.set_base_permission(&user, &ctx.org, permission).await?;
    Ok(Redirect::to(&format!("/{}/settings", ctx.org.name())).into_response())
}

/// `GET /{org}/settings`
pub async fn settings(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path(owner): Path<String>,
) -> WebResult<OrgSettingsPage> {
    let ctx = managed(&state, &user, &owner).await?;
    settings_page(&state, &user, page, &ctx, OrgProfileForm::from_org(&ctx.org)).await
}

/// `POST /{org}/settings`: save the profile.
pub async fn save_settings(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path(owner): Path<String>,
    Form(form): Form<ProfileFormData>,
) -> WebResult<Response> {
    let ctx = managed(&state, &user, &owner).await?;
    let input = UpdateOrganization {
        display_name: form.display_name.clone(),
        description: form.description.clone(),
        website: form.website.clone(),
    };
    match state.services.orgs.update_profile(&user, &ctx.org, input).await {
        Ok(()) => Ok(Redirect::to(&format!("/{}/settings", ctx.org.name())).into_response()),
        Err(error) => {
            let message = validation_message(error)?;
            let submitted = OrgProfileForm { display_name: form.display_name, description: form.description, website: form.website };
            let mut page = settings_page(&state, &user, page, &ctx, submitted).await?;
            page.error = FormError::from_message(&message);
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
    }
}

/// `POST /{org}/settings/delete`
pub async fn delete(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path(owner): Path<String>,
    Form(form): Form<DeleteFormData>,
) -> WebResult<Response> {
    let ctx = managed(&state, &user, &owner).await?;
    match state.services.orgs.delete(&user, &ctx.org, &form.confirm).await {
        Ok(()) => Ok(Redirect::to("/").into_response()),
        Err(error) => {
            let message = validation_message(error)?;
            let mut page = settings_page(&state, &user, page, &ctx, OrgProfileForm::from_org(&ctx.org)).await?;
            page.delete_error = FormError::from_message(&message);
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::routing::get;

    use super::*;

    /// Route conflicts panic when the router is built; make sure ours coexist with the
    /// account and repository routes they share a prefix with.
    #[test]
    fn routes_coexist_with_account_and_repository_routes() {
        let _router: Router<AppState> = Router::new()
            .route("/{owner}", get(|| async {}))
            .route("/{owner}/{repo}", get(|| async {}))
            .route("/{owner}/{repo}/commit/{revision}", get(|| async {}))
            .route("/{owner}/{repo}/tree/{*rest}", get(|| async {}))
            .merge(super::super::settings::routes())
            .merge(super::super::admin::routes())
            .merge(super::super::repo_settings::routes())
            .merge(super::super::stars::routes())
            .merge(super::super::ssh_keys::routes())
            .merge(super::super::collaborators::routes())
            .merge(super::super::teams::routes())
            .merge(super::super::transfer::routes())
            .merge(routes());
    }
}
