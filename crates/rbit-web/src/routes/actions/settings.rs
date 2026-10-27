//! Actions settings: a repository's (`/{owner}/{repo}/settings/actions`), an organisation's
//! (`/{org}/settings/actions`) and the signed-in user's own (`/settings/actions`).
//!
//! Only repository administrators, organisation owners and the user themselves get these
//! pages; everyone else gets 404, as on every settings page. The service checks the same
//! again for each change. Secret values are write-only: a value is read from the form,
//! handed to the service and dropped, and a rejected form is shown again without it.
//!
//! Each form redirects back to its page with a `?done=` notice. The owner pages have one
//! path for every form, so forms say what they are with `kind` (`secret` or `variable`) and
//! `action` (`put`, `delete`, or `repositories` for an owner secret's repositories).
//!
//! Workstream U (interface and secrets) owns this file.

use axum::{
    Form,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use rbit_core::{
    AccountId, RepoId, User,
    actions::{DefaultTokenPermission, SecretVisibility},
};
use rbit_services::{
    ActionsService, OrgContext, ServiceError,
    actions::{secrets::ActionsTarget, settings::OwnerRepository},
};
use serde::Deserialize;

use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    routes::repo::RepoPage,
    views::{
        actions_settings::{
            Notice, OrgActionsSettingsPage, PermissionOption, RepoActionsSettingsPage, SecretsSection, TrustedRow, UserActionsSettingsPage,
            VariablesSection,
        },
        components::FormError,
        layout::Page,
        org::{OrgHeader, OrgTab},
        settings::SettingsSection,
    },
};

pub(super) fn actions(state: &AppState) -> WebResult<&ActionsService> {
    state.services.actions.as_ref().ok_or(WebError::NotFound)
}

/// `?done=` after a form's redirect.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct DoneQuery {
    done: String,
}

impl DoneQuery {
    pub(super) fn notice(&self) -> Option<Notice> {
        Notice::parse(&self.done)
    }
}

pub(super) fn done(url: &str, notice: Notice) -> Response {
    Redirect::to(&format!("{url}?done={}", notice.key())).into_response()
}

// ------------------------------------------------------------------ forms

/// A secret or variable form. Read from the pairs, since an owner secret's repositories
/// repeat the `repository` field.
#[derive(Debug, Default)]
pub(super) struct EntryForm {
    pub(super) kind: String,
    action: String,
    name: String,
    value: String,
    visibility: String,
    repositories: Vec<RepoId>,
}

impl EntryForm {
    pub(super) fn parse(pairs: Vec<(String, String)>) -> Self {
        let mut form = Self::default();
        for (key, value) in pairs {
            match key.as_str() {
                "kind" => form.kind = value,
                "action" => form.action = value,
                "name" => form.name = value,
                // Browsers send line breaks in a textarea as CRLF; keys and certificates
                // are expected with LF.
                "value" => form.value = value.replace("\r\n", "\n"),
                "visibility" => form.visibility = value,
                "repository" => form.repositories.extend(value.parse().ok().map(RepoId)),
                _ => {}
            }
        }
        form
    }

    fn visibility(&self) -> SecretVisibility {
        SecretVisibility::parse(&self.visibility).unwrap_or_default()
    }
}

/// Applies a secret form to `target`.
pub(super) async fn apply_secret(
    actions: &ActionsService,
    user: &User,
    target: ActionsTarget<'_>,
    form: &EntryForm,
) -> Result<Notice, ServiceError> {
    match form.action.as_str() {
        "delete" => {
            actions.delete_secret(user, target, &form.name).await?;
            Ok(Notice::SecretDeleted)
        }
        "repositories" => {
            actions.set_secret_repositories(user, target, &form.name, &form.repositories).await?;
            Ok(Notice::SecretSaved)
        }
        _ => {
            let owner = matches!(target, ActionsTarget::Organization(_) | ActionsTarget::User(_));
            let visibility = if owner { form.visibility() } else { SecretVisibility::All };
            actions.put_secret(user, target, &form.name, &form.value, visibility).await?;
            if visibility == SecretVisibility::Selected {
                actions.set_secret_repositories(user, target, &form.name, &form.repositories).await?;
            }
            Ok(Notice::SecretSaved)
        }
    }
}

/// Applies a variable form to `target`.
pub(super) async fn apply_variable(
    actions: &ActionsService,
    user: &User,
    target: ActionsTarget<'_>,
    form: &EntryForm,
) -> Result<Notice, ServiceError> {
    if form.action == "delete" {
        actions.delete_variable(user, target, &form.name).await?;
        Ok(Notice::VariableDeleted)
    } else {
        actions.put_variable(user, target, &form.name, &form.value).await?;
        Ok(Notice::VariableSaved)
    }
}

/// The secrets and variables sections of `target`, posting to `secrets_url` and
/// `variables_url`.
pub(super) async fn sections(
    actions: &ActionsService,
    target: ActionsTarget<'_>,
    secrets_url: String,
    variables_url: String,
) -> WebResult<(SecretsSection, VariablesSection)> {
    let owner_repositories: Option<Vec<OwnerRepository>> = match target {
        ActionsTarget::Repository(_) | ActionsTarget::Environment(..) => None,
        ActionsTarget::Organization(_) | ActionsTarget::User(_) => Some(actions.owner_repositories(target).await?),
    };
    let secrets = actions.list_secrets(target).await?;
    let problem = actions.secrets_problem().await.map(|problem| problem.to_string());
    let variables = actions.list_variables(target).await?;
    Ok((
        SecretsSection::new(secrets_url, &secrets, owner_repositories.as_deref(), problem),
        VariablesSection::new(variables_url, &variables),
    ))
}

/// Puts a rejected entry form's message (and, for a new one, what was submitted, never a
/// secret's value) back on its section.
pub(super) fn reject(
    secrets: SecretsSection,
    variables: VariablesSection,
    form: &EntryForm,
    message: &str,
) -> (SecretsSection, VariablesSection) {
    let error = FormError::from_message(message);
    if form.kind == "variable" {
        (secrets, variables.rejected(error, &form.name, &form.value))
    } else {
        let selected = form.visibility() == SecretVisibility::Selected;
        (secrets.rejected(error, &form.name, selected, &form.repositories), variables)
    }
}

// ------------------------------------------------------------------ repository

/// Loads the repository for a viewer who may administer it; 404 for everyone else.
pub(super) async fn load_admin(state: &AppState, viewer: &Viewer, owner: &str, repo: &str) -> WebResult<RepoPage> {
    actions(state)?;
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    if !loaded.ctx.access.can_admin() {
        return Err(WebError::NotFound);
    }
    Ok(loaded)
}

/// The repository's settings page with its stored values.
async fn repo_page(state: &AppState, loaded: &RepoPage, page: Page, notice: Option<Notice>) -> WebResult<RepoActionsSettingsPage> {
    let actions = actions(state)?;
    let target = ActionsTarget::Repository(&loaded.ctx);
    let settings = actions.repo_settings(&loaded.ctx).await?;
    let base = format!("{}/settings", loaded.urls.home());
    let action_url = format!("{base}/actions");
    let (secrets, variables) = sections(actions, target, format!("{action_url}/secrets"), format!("{action_url}/variables")).await?;
    Ok(RepoActionsSettingsPage {
        page: page.titled(format!("Actions · Settings · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(state, "settings"),
        settings_url: base,
        enabled: settings.enabled,
        permissions: PermissionOption::list(settings.token_permission),
        general_error: FormError::default(),
        notice,
        secrets,
        variables,
        trusted: settings.trusted.iter().map(TrustedRow::from).collect(),
        trusted_url: format!("{action_url}/trusted"),
        runners_url: format!("{action_url}/runners"),
        environments_url: format!("{action_url}/environments"),
        action_url,
    })
}

/// `GET /{owner}/{repo}/settings/actions`
pub async fn repository(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<DoneQuery>,
) -> WebResult<RepoActionsSettingsPage> {
    let loaded = load_admin(&state, &viewer, &owner, &repo).await?;
    repo_page(&state, &loaded, page, query.notice()).await
}

/// `GET /{owner}/{repo}/settings/actions/{secrets,variables}`: the forms are on the page.
pub async fn to_repository(Path((owner, repo)): Path<(String, String)>) -> Redirect {
    Redirect::to(&format!("/{owner}/{repo}/settings/actions"))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct GeneralForm {
    enabled: Option<String>,
    token_permission: String,
}

/// `POST /{owner}/{repo}/settings/actions`: the switch and the default token permission.
pub async fn save_repository(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<GeneralForm>,
) -> WebResult<Response> {
    let loaded = load_admin(&state, &viewer, &owner, &repo).await?;
    let actions = actions(&state)?;
    let Some(permission) = DefaultTokenPermission::parse(&form.token_permission) else {
        let mut settings = repo_page(&state, &loaded, page, None).await?;
        settings.general_error = FormError::new("Choose read or read and write.");
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, settings).into_response());
    };
    actions.set_repo_enabled(&user, &loaded.ctx, form.enabled.is_some()).await?;
    actions.set_token_permission(&user, &loaded.ctx, permission).await?;
    Ok(done(&format!("{}/settings/actions", loaded.urls.home()), Notice::Saved))
}

/// `POST /{owner}/{repo}/settings/actions/secrets`
pub async fn repository_secret(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> WebResult<Response> {
    let mut form = EntryForm::parse(pairs);
    "secret".clone_into(&mut form.kind);
    repository_entry(&state, &user, &viewer, page, (&owner, &repo), &form).await
}

/// `POST /{owner}/{repo}/settings/actions/variables`
pub async fn repository_variable(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> WebResult<Response> {
    let mut form = EntryForm::parse(pairs);
    "variable".clone_into(&mut form.kind);
    repository_entry(&state, &user, &viewer, page, (&owner, &repo), &form).await
}

async fn repository_entry(
    state: &AppState,
    user: &User,
    viewer: &Viewer,
    page: Page,
    (owner, repo): (&str, &str),
    form: &EntryForm,
) -> WebResult<Response> {
    let loaded = load_admin(state, viewer, owner, repo).await?;
    let actions = actions(state)?;
    let target = ActionsTarget::Repository(&loaded.ctx);
    let result = if form.kind == "variable" {
        apply_variable(actions, user, target, form).await
    } else {
        apply_secret(actions, user, target, form).await
    };
    match result {
        Ok(notice) => Ok(done(&format!("{}/settings/actions", loaded.urls.home()), notice)),
        Err(ServiceError::Invalid(message)) => {
            let mut settings = repo_page(state, &loaded, page, None).await?;
            (settings.secrets, settings.variables) = reject(settings.secrets, settings.variables, form, &message);
            Ok((StatusCode::UNPROCESSABLE_ENTITY, settings).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct TrustedForm {
    user: String,
}

/// `POST /{owner}/{repo}/settings/actions/trusted`: stops trusting an author.
pub async fn untrust(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<TrustedForm>,
) -> WebResult<Response> {
    let loaded = load_admin(&state, &viewer, &owner, &repo).await?;
    let author = AccountId(form.user.parse().map_err(|_| WebError::NotFound)?);
    actions(&state)?.untrust_user(&user, &loaded.ctx, author).await?;
    Ok(done(&format!("{}/settings/actions", loaded.urls.home()), Notice::Untrusted))
}

// ------------------------------------------------------------------ organisation

/// The organisation, if `user` may manage it; 404 for everyone else.
async fn managed_org(state: &AppState, user: &User, owner: &str) -> WebResult<OrgContext> {
    actions(state)?;
    let ctx = state.services.orgs.open(Some(user), owner).await?;
    if ctx.can_manage { Ok(ctx) } else { Err(WebError::NotFound) }
}

async fn org_page(state: &AppState, ctx: &OrgContext, page: Page, notice: Option<Notice>) -> WebResult<OrgActionsSettingsPage> {
    let url = format!("/{}/settings/actions", ctx.org.name());
    let (secrets, variables) = sections(actions(state)?, ActionsTarget::Organization(ctx), url.clone(), url).await?;
    Ok(OrgActionsSettingsPage {
        page: page.titled(format!("Actions · Settings · {}", ctx.org.label())),
        header: OrgHeader::new(ctx, OrgTab::Settings),
        notice,
        secrets,
        variables,
        runners_url: format!("/{}/settings/actions/runners", ctx.org.name()),
    })
}

/// `GET /{org}/settings/actions`
pub async fn organization(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path(owner): Path<String>,
    Query(query): Query<DoneQuery>,
) -> WebResult<OrgActionsSettingsPage> {
    let ctx = managed_org(&state, &user, &owner).await?;
    org_page(&state, &ctx, page, query.notice()).await
}

/// `POST /{org}/settings/actions`: every secret and variable form of the page.
pub async fn save_organization(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path(owner): Path<String>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> WebResult<Response> {
    let ctx = managed_org(&state, &user, &owner).await?;
    let form = EntryForm::parse(pairs);
    match apply(actions(&state)?, &user, ActionsTarget::Organization(&ctx), &form).await {
        Ok(notice) => Ok(done(&format!("/{}/settings/actions", ctx.org.name()), notice)),
        Err(ServiceError::Invalid(message)) => {
            let mut settings = org_page(&state, &ctx, page, None).await?;
            (settings.secrets, settings.variables) = reject(settings.secrets, settings.variables, &form, &message);
            Ok((StatusCode::UNPROCESSABLE_ENTITY, settings).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// Applies an owner page's form, by its `kind`.
async fn apply(actions: &ActionsService, user: &User, target: ActionsTarget<'_>, form: &EntryForm) -> Result<Notice, ServiceError> {
    match form.kind.as_str() {
        "secret" => apply_secret(actions, user, target, form).await,
        "variable" => apply_variable(actions, user, target, form).await,
        _ => Err(ServiceError::Invalid("the form is incomplete; reload the page and try again".to_owned())),
    }
}

// ------------------------------------------------------------------ personal

const PERSONAL_URL: &str = SettingsSection::Actions.url();

async fn user_page(state: &AppState, user: &User, page: Page, notice: Option<Notice>) -> WebResult<UserActionsSettingsPage> {
    let (secrets, variables) =
        sections(actions(state)?, ActionsTarget::User(user), PERSONAL_URL.to_owned(), PERSONAL_URL.to_owned()).await?;
    Ok(UserActionsSettingsPage {
        page: page.titled("Actions"),
        settings_nav: SettingsSection::Actions.nav(),
        notice,
        secrets,
        variables,
        runners_url: "/settings/actions/runners".to_owned(),
    })
}

/// `GET /settings/actions`: the signed-in user's own secrets and variables.
pub async fn personal(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Query(query): Query<DoneQuery>,
) -> WebResult<UserActionsSettingsPage> {
    user_page(&state, &user, page, query.notice()).await
}

/// `POST /settings/actions`: every secret and variable form of the page.
pub async fn save_personal(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Form(pairs): Form<Vec<(String, String)>>,
) -> WebResult<Response> {
    let form = EntryForm::parse(pairs);
    match apply(actions(&state)?, &user, ActionsTarget::User(&user), &form).await {
        Ok(notice) => Ok(done(PERSONAL_URL, notice)),
        Err(ServiceError::Invalid(message)) => {
            let mut settings = user_page(&state, &user, page, None).await?;
            (settings.secrets, settings.variables) = reject(settings.secrets, settings.variables, &form, &message);
            Ok((StatusCode::UNPROCESSABLE_ENTITY, settings).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect()
    }

    #[test]
    fn entry_forms_collect_repositories_and_normalise_line_breaks() {
        let form = EntryForm::parse(pairs(&[
            ("kind", "secret"),
            ("name", "KEY"),
            ("value", "line one\r\nline two\r\n"),
            ("visibility", "selected"),
            ("repository", "3"),
            ("repository", "nope"),
            ("repository", "7"),
        ]));
        assert_eq!(form.value, "line one\nline two\n");
        assert_eq!(form.repositories, [RepoId(3), RepoId(7)]);
        assert_eq!(form.visibility(), SecretVisibility::Selected);
        assert_eq!(EntryForm::parse(pairs(&[("visibility", "everyone")])).visibility(), SecretVisibility::All);
    }
}
