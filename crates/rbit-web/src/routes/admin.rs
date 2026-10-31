//! Site administration panel: `/admin/…`.
//!
//! Every handler takes [`SiteAdmin`], so non-administrators get the ordinary 404 page and
//! never learn the panel exists. The routes are merged into the page router, so they get
//! sessions, CSRF protection, security headers and error pages automatically.

use std::collections::BTreeMap;

use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rbit_core::{AccountId, InstanceSettings, OrgCreationPolicy, RegistrationMode, RepoId, User, Visibility};
use rbit_services::{
    ServiceError, SignUp,
    admin::{ListQuery, UserRecord},
};
use serde::Deserialize;

use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::SiteAdmin,
    views::{
        admin::{
            AdminSection, ConfigurationPage, DashboardPage, DeleteRepositoryPage, DeleteUserPage, NewUserPage, OrganizationsPage,
            RepositoriesPage, SetPasswordPage, SettingsForm, SettingsPage, UsersNotice, UsersPage, listing_url,
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
        .route("/admin/users/{id}/password", get(set_password_form).post(set_password))
        .route("/admin/users/{id}/password-reset", post(send_password_reset))
        .route("/admin/organizations", get(organizations))
        .route("/admin/repositories", get(repositories))
        .route("/admin/repositories/{id}/delete", get(delete_repository_form).post(delete_repository))
        .route("/admin/settings", get(settings).post(update_settings))
        .route("/admin/configuration", get(configuration).post(save_configuration))
        .route("/admin/configuration/reset", post(reset_configuration))
}

/// `?q=…&page=…` on listings. Both are lenient: anything unparsable means "first page".
#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    q: Option<String>,
    page: Option<String>,
    /// A confirmation to show (users list only).
    #[serde(default)]
    notice: Option<String>,
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
pub async fn users(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Query(params): Query<ListParams>,
) -> WebResult<UsersPage> {
    let notice = params.notice.as_deref().and_then(UsersNotice::parse);
    Ok(users_page(&state, &admin, page, &params.query()).await?.with_notice(notice))
}

async fn users_page(state: &AppState, admin: &User, page: Page, query: &ListQuery) -> WebResult<UsersPage> {
    let paged = state.services.admin.users(admin, query).await?;
    let recovery = &state.services.recovery;
    let ids: Vec<AccountId> = paged.items.iter().map(|record| record.user.id()).collect();
    let verified = recovery.verified_among(admin, &ids).await?;
    Ok(UsersPage::new(page, admin, query.search(), &paged).with_email(&verified, recovery.email_available()))
}

/// `POST /admin/users/{id}/password-reset`: emails the user a reset link.
pub async fn send_password_reset(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Path(id): Path<String>,
    Form(form): Form<ModerationForm>,
) -> WebResult<Response> {
    let id = AccountId(parse_id(&id)?);
    let query = ListParams { q: Some(form.q), page: form.page, notice: None }.query();
    match state.services.recovery.admin_send_password_reset(&admin, id).await {
        Ok(_) => {
            let listing = listing_url(AdminSection::Users.url(), query.search(), query.page());
            Ok(Redirect::to(&UsersNotice::ResetSent.url(&listing)).into_response())
        }
        Err(ServiceError::Invalid(message)) => {
            let page = users_page(&state, &admin, page, &query).await?.with_error(FormError::from_message(&message));
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

#[derive(Debug, Deserialize)]
pub struct SetPasswordForm {
    #[serde(default)]
    new_password: String,
    #[serde(default)]
    confirm_password: String,
}

/// `GET /admin/users/{id}/password`
pub async fn set_password_form(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Path(id): Path<String>,
) -> WebResult<SetPasswordPage> {
    let record = state.services.admin.user(&admin, AccountId(parse_id(&id)?)).await?;
    Ok(set_password_page(&state, &admin, page, &record))
}

fn set_password_page(state: &AppState, admin: &User, page: Page, record: &UserRecord) -> SetPasswordPage {
    SetPasswordPage::new(page, admin, record, state.config.security.min_password_length, state.services.recovery.email_available())
}

/// `POST /admin/users/{id}/password`
pub async fn set_password(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Path(id): Path<String>,
    Form(form): Form<SetPasswordForm>,
) -> WebResult<Response> {
    let record = state.services.admin.user(&admin, AccountId(parse_id(&id)?)).await?;
    let result = if form.new_password == form.confirm_password {
        state.services.recovery.admin_set_password(&admin, record.user.id(), form.new_password).await
    } else {
        Err(ServiceError::Invalid("the new passwords do not match".to_owned()))
    };
    match result {
        Ok(()) => {
            let listing = listing_url(AdminSection::Users.url(), record.user.name().as_str(), 1);
            Ok(Redirect::to(&UsersNotice::PasswordSet.url(&listing)).into_response())
        }
        Err(ServiceError::Invalid(message)) => {
            let page = set_password_page(&state, &admin, page, &record).with_error(FormError::from_message(&message));
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
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

    let query = ListParams { q: Some(form.q), page: form.page, notice: None }.query();
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
        page: page.titled(crate::i18n::text("core", "title-new-user-administration")),
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
pub async fn create_user(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Form(form): Form<NewUserForm>,
) -> WebResult<Response> {
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
pub async fn delete_user_form(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Path(id): Path<String>,
) -> WebResult<DeleteUserPage> {
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
pub async fn settings(
    State(state): State<AppState>,
    SiteAdmin(_admin): SiteAdmin,
    page: Page,
    Query(params): Query<SavedParams>,
) -> SettingsPage {
    let form = SettingsForm::from(&state.services.instance.current());
    SettingsPage::new(page, form, FormError::default(), params.saved.is_some())
}

/// Radio values are validated strictly: an unknown value is an error, never a silent default.
#[derive(Debug, Default, Deserialize)]
pub struct SettingsSubmission {
    #[serde(default)]
    primary_color: String,
    #[serde(default)]
    primary_text_color: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    welcome_title: String,
    #[serde(default)]
    welcome_message: String,
    #[serde(default)]
    logo_url: String,
    #[serde(default)]
    favicon_url: String,
    #[serde(default)]
    app_icon_url: String,
    #[serde(default)]
    footer_text: String,
    #[serde(default)]
    support_url: String,
    #[serde(default)]
    terms_url: String,
    #[serde(default)]
    privacy_url: String,
    show_powered_by: Option<String>,
    show_version: Option<String>,
    show_features: Option<String>,
    #[serde(default)]
    site_name: String,
    #[serde(default)]
    registration: String,
    #[serde(default)]
    org_creation: String,
    #[serde(default)]
    default_visibility: String,
    /// Missing (an older form) keeps the current hour.
    digest_hour: Option<String>,
}

impl SettingsSubmission {
    fn branding(&self) -> rbit_core::config::BrandingConfig {
        rbit_core::config::BrandingConfig {
            primary_color: self.primary_color.trim().to_owned(),
            primary_text_color: self.primary_text_color.trim().to_owned(),
            description: self.description.trim().to_owned(),
            welcome_title: self.welcome_title.trim().to_owned(),
            welcome_message: self.welcome_message.trim().to_owned(),
            logo_url: self.logo_url.trim().to_owned(),
            favicon_url: self.favicon_url.trim().to_owned(),
            app_icon_url: self.app_icon_url.trim().to_owned(),
            footer_text: self.footer_text.trim().to_owned(),
            support_url: self.support_url.trim().to_owned(),
            terms_url: self.terms_url.trim().to_owned(),
            privacy_url: self.privacy_url.trim().to_owned(),
            show_powered_by: self.show_powered_by.is_some(),
            show_version: self.show_version.is_some(),
            show_features: self.show_features.is_some(),
        }
    }

    /// The submitted settings; `current_digest_hour` fills in a missing digest hour.
    fn parse(&self, current_digest_hour: u8) -> Result<InstanceSettings, ServiceError> {
        let invalid = |what: &str| ServiceError::Invalid(format!("choose one of the offered {what} options"));
        Ok(InstanceSettings {
            site_name: self.site_name.clone(),
            branding: self.branding(),
            registration: RegistrationMode::parse(&self.registration).ok_or_else(|| invalid("registration"))?,
            org_creation: OrgCreationPolicy::parse(&self.org_creation).ok_or_else(|| invalid("organization"))?,
            default_visibility: Visibility::parse(&self.default_visibility).ok_or_else(|| invalid("visibility"))?,
            digest_hour: match &self.digest_hour {
                None => current_digest_hour,
                Some(_) => self.digest_hour().ok_or_else(|| invalid("digest time"))?,
            },
        })
    }

    fn digest_hour(&self) -> Option<u8> {
        self.digest_hour.as_deref()?.trim().parse::<u8>().ok().filter(|hour| *hour <= rbit_core::config::MAX_DIGEST_HOUR)
    }

    /// The form as submitted, for re-rendering after an error.
    fn form(&self) -> SettingsForm {
        SettingsForm {
            site_name: self.site_name.clone(),
            branding: self.branding(),
            registration_open: self.registration != RegistrationMode::Closed.as_str(),
            admins_only_orgs: self.org_creation == OrgCreationPolicy::AdminsOnly.as_str(),
            private_by_default: self.default_visibility == Visibility::Private.as_str(),
            digest_hour: self.digest_hour(),
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
    let result = match form.parse(state.services.instance.current().digest_hour) {
        Ok(settings) => state.services.admin.update_settings(&admin, settings).await,
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

#[derive(Debug, Default, Deserialize)]
pub struct ConfigurationParams {
    section: Option<String>,
    saved: Option<String>,
}

pub async fn configuration(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Query(params): Query<ConfigurationParams>,
) -> WebResult<ConfigurationPage> {
    let document = serde_json::from_str(&state.services.admin.configuration(&admin).await?).map_err(WebError::internal)?;
    configuration_page(page, &document, params.section.as_deref().unwrap_or("server"), FormError::default(), params.saved.is_some())
}

fn configuration_page(
    page: Page,
    document: &serde_json::Value,
    section: &str,
    error: FormError,
    saved: bool,
) -> WebResult<ConfigurationPage> {
    Ok(ConfigurationPage {
        page: page.titled(crate::i18n::text("core", "title-configuration-administration")),
        admin_nav: AdminSection::Configuration.nav(),
        form: crate::views::admin::configuration::ConfigurationForm::new(document, section)?,
        error,
        saved,
    })
}

pub async fn save_configuration(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Form(inputs): Form<BTreeMap<String, String>>,
) -> WebResult<Response> {
    let section = inputs.get("section").map_or("server", String::as_str);
    let mut document = serde_json::from_str(&state.services.admin.configuration(&admin).await?).map_err(WebError::internal)?;
    let result = match crate::views::admin::configuration::apply_submission(&mut document, section, &inputs) {
        Ok(true) => {
            let document = serde_json::to_string(&document).map_err(WebError::internal)?;
            state.services.admin.stage_configuration(&admin, &document).await
        }
        Ok(false) => return Ok(configuration_page(page, &document, section, FormError::default(), false)?.into_response()),
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => Ok(Redirect::to(&format!("/admin/configuration?section={section}&saved=1")).into_response()),
        Err(ServiceError::Invalid(message)) => Ok((StatusCode::UNPROCESSABLE_ENTITY, {
            let mut rendered = configuration_page(page, &document, section, FormError::from_message(&message), false)?;
            rendered.form = rendered.form.with_inputs(&inputs);
            rendered
        })
            .into_response()),
        Err(error) => Err(error.into()),
    }
}

pub async fn reset_configuration(State(state): State<AppState>, SiteAdmin(admin): SiteAdmin) -> WebResult<Redirect> {
    state.services.admin.reset_configuration(&admin).await?;
    Ok(Redirect::to("/admin/configuration?saved=1"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_params_are_lenient() {
        let params = ListParams { q: Some(" alice ".to_owned()), page: Some("x".to_owned()), notice: None };
        assert_eq!(params.query(), ListQuery { search: " alice ".to_owned(), page: 1 });
        assert_eq!(ListParams { q: None, page: Some("3".to_owned()), notice: None }.query().page(), 3);
        assert_eq!(ListParams::default().query().page(), 1);
    }

    #[test]
    fn settings_submission_rejects_unknown_choices() {
        let submission = |registration: &str| SettingsSubmission {
            site_name: "Forge".to_owned(),
            registration: registration.to_owned(),
            org_creation: "admins_only".to_owned(),
            default_visibility: "private".to_owned(),
            digest_hour: Some("6".to_owned()),
            ..SettingsSubmission::default()
        };
        let parsed = submission("closed").parse(8).unwrap();
        assert_eq!(parsed.registration, RegistrationMode::Closed);
        assert_eq!(parsed.org_creation, OrgCreationPolicy::AdminsOnly);
        assert_eq!(parsed.default_visibility, Visibility::Private);
        assert_eq!(parsed.digest_hour, 6);
        assert!(matches!(submission("maybe").parse(8), Err(ServiceError::Invalid(_))));
        for hour in ["24", "", "noon"] {
            let late = SettingsSubmission { digest_hour: Some(hour.to_owned()), ..submission("closed") };
            assert!(matches!(late.parse(8), Err(ServiceError::Invalid(_))), "{hour:?}");
        }
        let older_form = SettingsSubmission { digest_hour: None, ..submission("closed") };
        assert_eq!(older_form.parse(8).unwrap().digest_hour, 8, "a form without the field keeps the current hour");

        let form = submission("closed").form();
        assert!(!form.registration_open && form.admins_only_orgs && form.private_by_default);
    }
}
