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

use rbit_core::{RepoId, User, actions::SecretVisibility};
use rbit_services::{
    ActionsService, OrgContext, ServiceError,
    actions::{secrets::ActionsTarget, settings::OwnerRepository},
};

use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::Viewer,
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

pub(crate) fn actions(state: &AppState) -> WebResult<ActionsService> {
    state.services.actions.snapshot().ok_or(WebError::NotFound)
}

// ------------------------------------------------------------------ forms

#[derive(Debug, Default)]
pub(crate) struct EntryForm {
    pub(crate) kind: String,
    action: String,
    name: String,
    value: String,
    visibility: String,
    repositories: Vec<RepoId>,
}

impl EntryForm {
    pub(crate) fn parse(pairs: Vec<(String, String)>) -> Self {
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

pub(crate) async fn apply_secret(
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

pub(crate) async fn apply_variable(
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

pub(crate) async fn sections(
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

pub(crate) fn reject(
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

pub(crate) async fn load_admin(state: &AppState, viewer: &Viewer, owner: &str, repo: &str) -> WebResult<RepoPage> {
    actions(state)?;
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    if !loaded.ctx.access.can_admin() {
        return Err(WebError::NotFound);
    }
    Ok(loaded)
}

pub(crate) async fn repo_page(
    state: &AppState,
    loaded: &RepoPage,
    mut page: Page,
    notice: Option<Notice>,
) -> WebResult<RepoActionsSettingsPage> {
    let actions = actions(state)?;
    let target = ActionsTarget::Repository(&loaded.ctx);
    let settings = actions.repo_settings(&loaded.ctx).await?;
    let base = format!("{}/settings", loaded.urls.home());
    let action_url = format!("{}/-/plugins/rbit.actions/settings", loaded.urls.home());
    let (secrets, variables) = sections(&actions, target, format!("{action_url}/secrets"), format!("{action_url}/variables")).await?;
    page.path = action_url.clone();
    let secrets = secrets.bounded(&page);
    let variables = variables.bounded(&page);
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

// ------------------------------------------------------------------ organisation

pub(crate) async fn managed_org(state: &AppState, user: &User, owner: &str) -> WebResult<OrgContext> {
    actions(state)?;
    let ctx = state.services.orgs.open(Some(user), owner).await?;
    if ctx.can_manage { Ok(ctx) } else { Err(WebError::NotFound) }
}

pub(crate) async fn org_page(
    state: &AppState,
    ctx: &OrgContext,
    mut page: Page,
    notice: Option<Notice>,
) -> WebResult<OrgActionsSettingsPage> {
    let url = crate::actions_plugin_routes::global_url("org/settings");
    let (secrets, variables) = sections(&actions(state)?, ActionsTarget::Organization(ctx), url.clone(), url).await?;
    page.path = crate::actions_plugin_routes::global_url("org/settings");
    let secrets = secrets.bounded(&page);
    let variables = variables.bounded(&page);
    Ok(OrgActionsSettingsPage {
        page: page.titled(format!("Actions · Settings · {}", ctx.org.label())),
        header: OrgHeader::new(ctx, OrgTab::Settings),
        notice,
        secrets,
        variables,
        runners_url: format!("{}?owner={}", crate::actions_plugin_routes::global_url("org/runners"), ctx.org.name()),
    })
}

pub(crate) async fn apply(
    actions: &ActionsService,
    user: &User,
    target: ActionsTarget<'_>,
    form: &EntryForm,
) -> Result<Notice, ServiceError> {
    match form.kind.as_str() {
        "secret" => apply_secret(actions, user, target, form).await,
        "variable" => apply_variable(actions, user, target, form).await,
        _ => Err(ServiceError::Invalid("the form is incomplete; reload the page and try again".to_owned())),
    }
}

// ------------------------------------------------------------------ personal

const PERSONAL_URL: &str = "/-/plugins/rbit.actions/settings";

pub(crate) async fn user_page(state: &AppState, user: &User, mut page: Page, notice: Option<Notice>) -> WebResult<UserActionsSettingsPage> {
    let (secrets, variables) =
        sections(&actions(state)?, ActionsTarget::User(user), PERSONAL_URL.to_owned(), PERSONAL_URL.to_owned()).await?;
    page.path = PERSONAL_URL.into();
    let secrets = secrets.bounded(&page);
    let variables = variables.bounded(&page);
    let title = page.locale.text("core", "title-actions");
    Ok(UserActionsSettingsPage {
        page: page.titled(title),
        settings_nav: SettingsSection::Actions.nav(),
        notice,
        secrets,
        variables,
        runners_url: "/-/plugins/rbit.actions/settings/runners".to_owned(),
    })
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
