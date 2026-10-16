//! Site administration panel: `/admin/…`.
//!
//! Every handler takes [`SiteAdmin`], so non-administrators get the ordinary 404 page and
//! never learn the panel exists. The routes are merged into the page router, so they get
//! sessions, CSRF protection, security headers and error pages automatically.

use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rustit_core::{AccountId, InstanceSettings, OrgCreationPolicy, RegistrationMode, RepoId, User, Visibility};
use rustit_services::{ServiceError, SignUp, admin::ListQuery};
use serde::Deserialize;

use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::SiteAdmin,
    views::{
        admin::{
            AdminSection, DashboardPage, DeleteRepositoryPage, DeleteUserPage, NewUserPage, OrganizationsPage, RepositoriesPage,
            SettingsForm, SettingsPage, UsersPage, listing_url,
        },
        components::FormError,
        layout::Page,
    },
};

/// Routes for `/admin/…`, merged into the page router.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin", get(dashboard))
        .route("/admin/users", get(users))
        .route("/admin/users/new", get(new_user_form).post(create_user))
        .route("/admin/users/{id}/{action}", post(moderate_user))
        .route("/admin/users/{id}/delete", get(delete_user_form).post(delete_user))
        .route("/admin/organizations", get(organizations))
        .route("/admin/repositories", get(repositories))
        .route("/admin/repositories/{id}/delete", get(delete_repository_form).post(delete_repository))
        .route("/admin/settings", get(settings).post(update_settings))
}

/// `?q=…&page=…` on listings. Both are lenient: anything unparsable means "first page".
#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    q: Option<String>,
    page: Option<String>,
}

impl ListParams {
    fn query(&self) -> ListQuery {
        ListQuery { search: self.q.clone().unwrap_or_default(), page: self.page.as_deref().and_then(|p| p.parse().ok()).unwrap_or(1) }
    }
}

/// Numeric path ids; anything else is a 404.
fn parse_id(id: &str) -> Result<i64, WebError> {
    id.parse().map_err(|_| WebError::NotFound)
}

// ------------------------------------------------------------------ dashboard

/// `GET /admin`
pub async fn dashboard(State(state): State<AppState>, SiteAdmin(admin): SiteAdmin, page: Page) -> WebResult<DashboardPage> {
    let dashboard = state.services.admin.dashboard(&admin).await?;
    Ok(DashboardPage::new(page, &dashboard))
}

// ------------------------------------------------------------------ users

/// `GET /admin/users`
pub async fn users(State(state): State<AppState>, SiteAdmin(admin): SiteAdmin, page: Page, Query(params): Query<ListParams>) -> WebResult<UsersPage> {
    users_page(&state, &admin, page, &params.query()).await
}

async fn users_page(state: &AppState, admin: &User, page: Page, query: &ListQuery) -> WebResult<UsersPage> {
    let paged = state.services.admin.users(admin, query).await?;
    Ok(UsersPage::new(page, admin, query.search(), &paged))
}

/// Hidden fields that bring an action back to the listing page it was taken on.
#[derive(Debug, Deserialize)]
pub struct ModerationForm {
    #[serde(default)]
    q: String,
    page: Option<String>,
}

/// `POST /admin/users/{id}/{promote|demote|suspend|unsuspend}`
pub async fn moderate_user(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Path((id, action)): Path<(String, String)>,
    Form(form): Form<ModerationForm>,
) -> WebResult<Response> {
    let id = AccountId(parse_id(&id)?);
    let service = &state.services.admin;
    let result = match action.as_str() {
        "promote" => service.set_admin(&admin, id, true).await,
        "demote" => service.set_admin(&admin, id, false).await,
        "suspend" => service.set_suspended(&admin, id, true).await,
        "unsuspend" => service.set_suspended(&admin, id, false).await,
        _ => return Err(WebError::NotFound),
    };

    let query = ListParams { q: Some(form.q), page: form.page }.query();
    match result {
        Ok(_) => Ok(Redirect::to(&listing_url(AdminSection::Users.url(), query.search(), query.page())).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let page = users_page(&state, &admin, page, &query).await?.with_error(FormError::from_message(&message));
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

#[derive(Debug, Deserialize)]
pub struct NewUserForm {
    #[serde(default)]
    username: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    password: String,
    is_admin: Option<String>,
}

/// `GET /admin/users/new`
pub async fn new_user_form(State(state): State<AppState>, SiteAdmin(_admin): SiteAdmin, page: Page) -> NewUserPage {
    new_user_page(&state, page, FormError::default(), String::new(), String::new(), false)
}

fn new_user_page(state: &AppState, page: Page, error: FormError, username: String, email: String, is_admin: bool) -> NewUserPage {
    NewUserPage {
        page: page.titled("New user · Administration"),
        admin_nav: AdminSection::Users.nav(),
        error,
        username,
        email,
        is_admin,
        min_password_length: state.config.security.min_password_length,
        registration_open: state.services.instance.current().registration == RegistrationMode::Open,
    }
}

/// `POST /admin/users/new`
pub async fn create_user(State(state): State<AppState>, SiteAdmin(admin): SiteAdmin, page: Page, Form(form): Form<NewUserForm>) -> WebResult<Response> {
    let is_admin = form.is_admin.is_some();
    let input = SignUp { username: form.username.clone(), email: form.email.clone(), password: form.password };
    match state.services.admin.create_user(&admin, input, is_admin).await {
        Ok(user) => Ok(Redirect::to(&listing_url(AdminSection::Users.url(), user.name().as_str(), 1)).into_response()),
        Err(error @ (ServiceError::Invalid(_) | ServiceError::Conflict(_))) => {
            let page = new_user_page(&state, page, FormError::from_message(&error.to_string()), form.username, form.email, is_admin);
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

#[derive(Debug, Deserialize)]
pub struct ConfirmForm {
    #[serde(default)]
    confirm: String,
}

/// `GET /admin/users/{id}/delete`: asks for the username to be typed.
pub async fn delete_user_form(State(state): State<AppState>, SiteAdmin(admin): SiteAdmin, page: Page, Path(id): Path<String>) -> WebResult<DeleteUserPage> {
    let record = state.services.admin.user(&admin, AccountId(parse_id(&id)?)).await?;
    Ok(DeleteUserPage::new(page, &admin, &record, FormError::default()))
}

/// `POST /admin/users/{id}/delete`
pub async fn delete_user(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Path(id): Path<String>,
    Form(form): Form<ConfirmForm>,
) -> WebResult<Response> {
    let id = AccountId(parse_id(&id)?);
    match state.services.admin.delete_user(&admin, id, &form.confirm).await {
        Ok(_) => Ok(Redirect::to(AdminSection::Users.url()).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let record = state.services.admin.user(&admin, id).await?;
            let page = DeleteUserPage::new(page, &admin, &record, FormError::from_message(&message));
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

// ------------------------------------------------------------------ organisations

/// `GET /admin/organizations`
pub async fn organizations(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Query(params): Query<ListParams>,
) -> WebResult<OrganizationsPage> {
    let query = params.query();
    let paged = state.services.admin.organizations(&admin, &query).await?;
    Ok(OrganizationsPage::new(page, query.search(), &paged))
}

// ------------------------------------------------------------------ repositories

/// `GET /admin/repositories`
pub async fn repositories(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Query(params): Query<ListParams>,
) -> WebResult<RepositoriesPage> {
    let query = params.query();
    let paged = state.services.admin.repositories(&admin, &query).await?;
    Ok(RepositoriesPage::new(page, query.search(), &paged))
}

/// `GET /admin/repositories/{id}/delete`: asks for `owner/name` to be typed.
pub async fn delete_repository_form(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Path(id): Path<String>,
) -> WebResult<DeleteRepositoryPage> {
    let repo = state.services.admin.repository(&admin, RepoId(parse_id(&id)?)).await?;
    Ok(DeleteRepositoryPage::new(page, &repo, FormError::default()))
}

/// `POST /admin/repositories/{id}/delete`
pub async fn delete_repository(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Path(id): Path<String>,
    Form(form): Form<ConfirmForm>,
) -> WebResult<Response> {
    let id = RepoId(parse_id(&id)?);
    match state.services.admin.delete_repository(&admin, id, &form.confirm).await {
        Ok(_) => Ok(Redirect::to(AdminSection::Repositories.url()).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let repo = state.services.admin.repository(&admin, id).await?;
            let page = DeleteRepositoryPage::new(page, &repo, FormError::from_message(&message));
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

// ------------------------------------------------------------------ instance settings

#[derive(Debug, Deserialize)]
pub struct SavedParams {
    saved: Option<String>,
}

/// `GET /admin/settings`
pub async fn settings(State(state): State<AppState>, SiteAdmin(_admin): SiteAdmin, page: Page, Query(params): Query<SavedParams>) -> SettingsPage {
    let form = SettingsForm::from(&state.services.instance.current());
    SettingsPage::new(page, form, FormError::default(), params.saved.is_some())
}

/// Radio values are validated strictly: an unknown value is an error, never a silent default.
#[derive(Debug, Deserialize)]
pub struct SettingsSubmission {
    #[serde(default)]
    site_name: String,
    #[serde(default)]
    registration: String,
    #[serde(default)]
    org_creation: String,
    #[serde(default)]
    default_visibility: String,
}

impl SettingsSubmission {
    fn parse(&self) -> Result<InstanceSettings, ServiceError> {
        let invalid = |what: &str| ServiceError::Invalid(format!("choose one of the offered {what} options"));
        Ok(InstanceSettings {
            site_name: self.site_name.clone(),
            registration: RegistrationMode::parse(&self.registration).ok_or_else(|| invalid("registration"))?,
            org_creation: OrgCreationPolicy::parse(&self.org_creation).ok_or_else(|| invalid("organization"))?,
            default_visibility: Visibility::parse(&self.default_visibility).ok_or_else(|| invalid("visibility"))?,
        })
    }

    /// The form as submitted, for re-rendering after an error.
    fn form(&self) -> SettingsForm {
        SettingsForm {
            site_name: self.site_name.clone(),
            registration_open: self.registration != RegistrationMode::Closed.as_str(),
            admins_only_orgs: self.org_creation == OrgCreationPolicy::AdminsOnly.as_str(),
            private_by_default: self.default_visibility == Visibility::Private.as_str(),
        }
    }
}

/// `POST /admin/settings`
pub async fn update_settings(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Form(form): Form<SettingsSubmission>,
) -> WebResult<Response> {
    let result = match form.parse() {
        Ok(settings) => state.services.instance.update(&admin, settings).await,
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => Ok(Redirect::to("/admin/settings?saved=1").into_response()),
        Err(ServiceError::Invalid(message)) => {
            let page = SettingsPage::new(page, form.form(), FormError::from_message(&message), false);
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_params_are_lenient() {
        let params = ListParams { q: Some(" alice ".to_owned()), page: Some("x".to_owned()) };
        assert_eq!(params.query(), ListQuery { search: " alice ".to_owned(), page: 1 });
        assert_eq!(ListParams { q: None, page: Some("3".to_owned()) }.query().page(), 3);
        assert_eq!(ListParams::default().query().page(), 1);
    }

    #[test]
    fn settings_submission_rejects_unknown_choices() {
        let submission = |registration: &str| SettingsSubmission {
            site_name: "Forge".to_owned(),
            registration: registration.to_owned(),
            org_creation: "admins_only".to_owned(),
            default_visibility: "private".to_owned(),
        };
        let parsed = submission("closed").parse().unwrap();
        assert_eq!(parsed.registration, RegistrationMode::Closed);
        assert_eq!(parsed.org_creation, OrgCreationPolicy::AdminsOnly);
        assert_eq!(parsed.default_visibility, Visibility::Private);
        assert!(matches!(submission("maybe").parse(), Err(ServiceError::Invalid(_))));

        let form = submission("closed").form();
        assert!(!form.registration_open && form.admins_only_orgs && form.private_by_default);
    }
}
