//! A repository's deployment environments (Actions: environments):
//! `/{owner}/{repo}/settings/actions/environments` lists them and adds one, and each has a
//! page of its own for its protection rules, secrets and variables. Repository
//! administrators only; everyone else gets 404, as on every settings page.

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_core::actions::{BranchPolicy, Environment};
use rbit_services::actions::secrets::ActionsTarget;

use super::settings::{actions, load_admin, sections};
use crate::{
    AppState,
    error::WebResult,
    extract::Viewer,
    routes::repo::RepoPage,
    views::{
        actions_settings::{EnvironmentPage, EnvironmentRow, EnvironmentsPage, Notice},
        components::FormError,
        layout::Page,
    },
};

fn environments_url(loaded: &RepoPage) -> String {
    format!("{}/-/plugins/rbit.actions/settings/environments", loaded.urls.home())
}

fn environment_url(loaded: &RepoPage, name: &str) -> String {
    format!(
        "{}/-/plugins/rbit.actions/settings/environment?environment={}",
        loaded.urls.home(),
        utf8_percent_encode(name, NON_ALPHANUMERIC)
    )
}

fn protection(environment: &Environment) -> String {
    let mut parts = Vec::new();
    match environment.reviewers.len() {
        0 => {}
        1 => parts.push("1 reviewer".to_owned()),
        count => parts.push(format!("{count} reviewers")),
    }
    match &environment.branch_policy {
        BranchPolicy::All => {}
        BranchPolicy::Protected => parts.push("protected branches".to_owned()),
        BranchPolicy::Selected(patterns) => parts.push(format!("branches {}", patterns.join(", "))),
    }
    if parts.is_empty() { "No protection".to_owned() } else { parts.join(" · ") }
}

pub(crate) async fn list_page(
    state: &AppState,
    loaded: &RepoPage,
    page: Page,
    error: FormError,
    new_name: String,
) -> WebResult<EnvironmentsPage> {
    let environments = actions(state)?.environments(&loaded.ctx).await?;
    let settings_url = format!("{}/settings", loaded.urls.home());
    Ok(EnvironmentsPage {
        page: page.titled(format!("Environments · Settings · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(state, "settings"),
        actions_url: format!("{}/-/plugins/rbit.actions/settings", loaded.urls.home()),
        settings_url,
        action_url: environments_url(loaded),
        environments: environments
            .iter()
            .map(|environment| EnvironmentRow {
                name: environment.name.clone(),
                url: environment_url(loaded, &environment.name),
                protection: protection(environment),
            })
            .collect(),
        error,
        new_name,
    })
}

pub(crate) async fn environment_page(
    state: &AppState,
    loaded: &RepoPage,
    mut page: Page,
    environment: &Environment,
    notice: Option<Notice>,
) -> WebResult<EnvironmentPage> {
    let actions = actions(state)?;
    let target = ActionsTarget::Environment(&loaded.ctx, environment);
    let base = format!("{}/-/plugins/rbit.actions/settings/environment", loaded.urls.home());
    let (secrets, variables) = sections(&actions, target, format!("{base}/secrets"), format!("{base}/variables")).await?;
    let people = actions.environment_reviewers(environment).await?;
    let (policy, patterns) = match &environment.branch_policy {
        BranchPolicy::All => ("all", String::new()),
        BranchPolicy::Protected => ("protected", String::new()),
        BranchPolicy::Selected(patterns) => ("selected", patterns.join("\n")),
    };
    let settings_url = format!("{}/settings", loaded.urls.home());
    page.path = base.clone();
    let secrets = secrets.bounded(&page);
    let variables = variables.bounded(&page);
    Ok(EnvironmentPage {
        page: page.titled(format!("{} · Environments · {}", environment.name, loaded.ctx.repo.full_name())),
        header: loaded.header(state, "settings"),
        actions_url: format!("{}/-/plugins/rbit.actions/settings", loaded.urls.home()),
        settings_url,
        environments_url: environments_url(loaded),
        name: environment.name.clone(),
        delete_url: format!("{base}/delete"),
        action_url: base,
        reviewers: people.iter().map(|account| account.name.to_string()).collect::<Vec<_>>().join("\n"),
        prevent_self_review: environment.prevent_self_review,
        wait_timer: environment.wait_timer.to_string(),
        policy: policy.to_owned(),
        patterns,
        error: FormError::default(),
        notice,
        secrets,
        variables,
    })
}

pub(crate) async fn load_environment(
    state: &AppState,
    viewer: &Viewer,
    owner: &str,
    repo: &str,
    name: &str,
) -> WebResult<(RepoPage, Environment)> {
    let loaded = load_admin(state, viewer, owner, repo).await?;
    let environment = actions(state)?.environment(&loaded.ctx, name).await?;
    Ok((loaded, environment))
}
