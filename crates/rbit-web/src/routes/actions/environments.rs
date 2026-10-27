//! A repository's deployment environments (Actions: environments):
//! `/{owner}/{repo}/settings/actions/environments` lists them and adds one, and each has a
//! page of its own for its protection rules, secrets and variables. Repository
//! administrators only; everyone else gets 404, as on every settings page.

use axum::{
    Form,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_core::{
    User,
    actions::{BranchPolicy, Environment},
};
use rbit_services::{
    ServiceError,
    actions::{environments::EnvironmentInput, secrets::ActionsTarget},
};
use serde::Deserialize;

use super::settings::{DoneQuery, EntryForm, actions, apply_secret, apply_variable, done, load_admin, reject, sections};
use crate::{
    AppState,
    error::WebResult,
    extract::{SignedIn, Viewer},
    routes::repo::RepoPage,
    views::{
        actions_settings::{EnvironmentPage, EnvironmentRow, EnvironmentsPage, Notice},
        components::FormError,
        layout::Page,
    },
};

fn environments_url(loaded: &RepoPage) -> String {
    format!("{}/settings/actions/environments", loaded.urls.home())
}

fn environment_url(loaded: &RepoPage, name: &str) -> String {
    format!("{}/{}", environments_url(loaded), utf8_percent_encode(name, NON_ALPHANUMERIC))
}

/// "2 reviewers · branches main, release/*", or "No protection".
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

async fn list_page(state: &AppState, loaded: &RepoPage, page: Page, error: FormError, new_name: String) -> WebResult<EnvironmentsPage> {
    let environments = actions(state)?.environments(&loaded.ctx).await?;
    let settings_url = format!("{}/settings", loaded.urls.home());
    Ok(EnvironmentsPage {
        page: page.titled(format!("Environments · Settings · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(state, "settings"),
        actions_url: format!("{settings_url}/actions"),
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

/// `GET /{owner}/{repo}/settings/actions/environments`
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
) -> WebResult<EnvironmentsPage> {
    let loaded = load_admin(&state, &viewer, &owner, &repo).await?;
    list_page(&state, &loaded, page, FormError::default(), String::new()).await
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct NewForm {
    name: String,
}

/// `POST /{owner}/{repo}/settings/actions/environments`: adds an unprotected environment.
pub async fn create(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<NewForm>,
) -> WebResult<Response> {
    let loaded = load_admin(&state, &viewer, &owner, &repo).await?;
    match actions(&state)?.put_environment(&user, &loaded.ctx, &form.name, &EnvironmentInput::default()).await {
        Ok(environment) => Ok(Redirect::to(&environment_url(&loaded, &environment.name)).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let page = list_page(&state, &loaded, page, FormError::from_message(&message), form.name).await?;
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

async fn environment_page(
    state: &AppState,
    loaded: &RepoPage,
    page: Page,
    environment: &Environment,
    notice: Option<Notice>,
) -> WebResult<EnvironmentPage> {
    let actions = actions(state)?;
    let target = ActionsTarget::Environment(&loaded.ctx, environment);
    let url = environment_url(loaded, &environment.name);
    let (secrets, variables) = sections(actions, target, format!("{url}/secrets"), format!("{url}/variables")).await?;
    let people = actions.environment_reviewers(environment).await?;
    let (policy, patterns) = match &environment.branch_policy {
        BranchPolicy::All => ("all", String::new()),
        BranchPolicy::Protected => ("protected", String::new()),
        BranchPolicy::Selected(patterns) => ("selected", patterns.join("\n")),
    };
    let settings_url = format!("{}/settings", loaded.urls.home());
    Ok(EnvironmentPage {
        page: page.titled(format!("{} · Environments · {}", environment.name, loaded.ctx.repo.full_name())),
        header: loaded.header(state, "settings"),
        actions_url: format!("{settings_url}/actions"),
        settings_url,
        environments_url: environments_url(loaded),
        name: environment.name.clone(),
        delete_url: format!("{url}/delete"),
        action_url: url,
        reviewers: people.iter().map(|account| account.name.to_string()).collect::<Vec<_>>().join("\n"),
        prevent_self_review: environment.prevent_self_review,
        policy: policy.to_owned(),
        patterns,
        error: FormError::default(),
        notice,
        secrets,
        variables,
    })
}

async fn load_environment(state: &AppState, viewer: &Viewer, owner: &str, repo: &str, name: &str) -> WebResult<(RepoPage, Environment)> {
    let loaded = load_admin(state, viewer, owner, repo).await?;
    let environment = actions(state)?.environment(&loaded.ctx, name).await?;
    Ok((loaded, environment))
}

/// `GET /{owner}/{repo}/settings/actions/environments/{environment}`
pub async fn show(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, name)): Path<(String, String, String)>,
    Query(query): Query<DoneQuery>,
) -> WebResult<EnvironmentPage> {
    let (loaded, environment) = load_environment(&state, &viewer, &owner, &repo, &name).await?;
    environment_page(&state, &loaded, page, &environment, query.notice()).await
}

/// `GET …/{environment}/{secrets,variables}`: the forms are on the environment's page.
pub async fn to_environment(Path((owner, repo, name)): Path<(String, String, String)>) -> Redirect {
    Redirect::to(&format!("/{owner}/{repo}/settings/actions/environments/{}", utf8_percent_encode(&name, NON_ALPHANUMERIC)))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RulesForm {
    reviewers: String,
    prevent_self_review: Option<String>,
    policy: String,
    patterns: String,
}

/// `POST /{owner}/{repo}/settings/actions/environments/{environment}`: its protection rules.
pub async fn save(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, name)): Path<(String, String, String)>,
    Form(form): Form<RulesForm>,
) -> WebResult<Response> {
    let (loaded, environment) = load_environment(&state, &viewer, &owner, &repo, &name).await?;
    let lines = |text: &str| text.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_owned).collect::<Vec<_>>();
    let branch_policy = match form.policy.as_str() {
        "protected" => BranchPolicy::Protected,
        "selected" => BranchPolicy::Selected(lines(&form.patterns)),
        _ => BranchPolicy::All,
    };
    let reviewers = lines(&form.reviewers.replace(',', "\n"));
    let input = EnvironmentInput { reviewers, prevent_self_review: form.prevent_self_review.is_some(), branch_policy };
    match actions(&state)?.put_environment(&user, &loaded.ctx, &environment.name, &input).await {
        Ok(_) => Ok(done(&environment_url(&loaded, &environment.name), Notice::Saved)),
        Err(ServiceError::Invalid(message)) => {
            let mut page = environment_page(&state, &loaded, page, &environment, None).await?;
            page.error = FormError::from_message(&message);
            page.reviewers = form.reviewers;
            page.prevent_self_review = form.prevent_self_review.is_some();
            page.policy = form.policy;
            page.patterns = form.patterns;
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// `POST …/{environment}/secrets`
pub async fn secret(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, name)): Path<(String, String, String)>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> WebResult<Response> {
    let mut form = EntryForm::parse(pairs);
    "secret".clone_into(&mut form.kind);
    entry(&state, &user, &viewer, page, (&owner, &repo, &name), &form).await
}

/// `POST …/{environment}/variables`
pub async fn variable(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, name)): Path<(String, String, String)>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> WebResult<Response> {
    let mut form = EntryForm::parse(pairs);
    "variable".clone_into(&mut form.kind);
    entry(&state, &user, &viewer, page, (&owner, &repo, &name), &form).await
}

async fn entry(
    state: &AppState,
    user: &User,
    viewer: &Viewer,
    page: Page,
    (owner, repo, name): (&str, &str, &str),
    form: &EntryForm,
) -> WebResult<Response> {
    let (loaded, environment) = load_environment(state, viewer, owner, repo, name).await?;
    let actions = actions(state)?;
    let target = ActionsTarget::Environment(&loaded.ctx, &environment);
    let result = if form.kind == "variable" {
        apply_variable(actions, user, target, form).await
    } else {
        apply_secret(actions, user, target, form).await
    };
    match result {
        Ok(notice) => Ok(done(&environment_url(&loaded, &environment.name), notice)),
        Err(ServiceError::Invalid(message)) => {
            let mut page = environment_page(state, &loaded, page, &environment, None).await?;
            (page.secrets, page.variables) = reject(page.secrets, page.variables, form, &message);
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// `POST …/{environment}/delete`
pub async fn delete(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    Path((owner, repo, name)): Path<(String, String, String)>,
) -> WebResult<Redirect> {
    let (loaded, environment) = load_environment(&state, &viewer, &owner, &repo, &name).await?;
    actions(&state)?.delete_environment(&user, &loaded.ctx, &environment.name).await?;
    Ok(Redirect::to(&environments_url(&loaded)))
}
