//! Runners and registration tokens of a repository, an organisation, the signed-in user's
//! account and the instance (administrators).
//!
//! Each scope has a page that lists its runners and tokens (`GET`), takes the token forms
//! (`POST`, `action=create` or `action=revoke`) and a `POST …/runners/{runner}` for disable,
//! enable and remove. A created token is rendered on the answer itself — never redirected
//! through a URL — so it is shown once. The service checks who may manage each scope again;
//! everyone else gets 404, like the settings pages.
//!
//! Workstream U (interface and secrets) owns this file.

use axum::{
    Form,
    extract::{Path, State},
    response::Redirect,
};
use rbit_core::{
    Timestamp, User,
    actions::{RegistrationTokenId, RunnerId, RunnerLabel},
};
use rbit_services::{
    ActionsService, ServiceError,
    actions::runners::{ManagedScope, RegistrationOptions},
};
use serde::Deserialize;

use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, SiteAdmin, Viewer},
    routes::repo::RepoPage,
    views::{
        actions_runners::{
            AdminRunnersPage, IssuedToken, LIFETIMES, OrgRunnersPage, RepoRunnersPage, RunnerRow, RunnersSection, TokenRow, UserRunnersPage,
        },
        admin::AdminSection,
        components::FormError,
        layout::Page,
        org::{OrgHeader, OrgTab},
        settings::SettingsSection,
    },
};

/// Most registration tokens listed (newest first).
const LISTED_TOKENS: usize = 20;

fn actions(state: &AppState) -> WebResult<ActionsService> {
    state.services.actions.snapshot().ok_or(WebError::NotFound)
}

/// Forbidden reads like a page that does not exist.
fn hidden(error: ServiceError) -> WebError {
    match error {
        ServiceError::Forbidden => WebError::NotFound,
        other => other.into(),
    }
}

/// The token forms.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct TokenForm {
    action: String,
    lifetime: Option<u64>,
    labels: String,
    reusable: Option<String>,
    token: Option<i64>,
}

/// The runner buttons.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RunnerForm {
    action: String,
}

/// What a token form did: a token to show once, or an error to show.
#[derive(Debug, Default)]
struct Outcome {
    issued: Option<IssuedToken>,
    error: FormError,
}

/// The section of `scope`; on a repository also the runners it inherits, read only.
async fn section(
    state: &AppState,
    user: &User,
    scope: ManagedScope<'_>,
    base_url: &str,
    subject: String,
    outcome: Outcome,
) -> WebResult<RunnersSection> {
    let actions = actions(state)?;
    let now = Timestamp::now();
    let own = scope.scope();
    let runners = match scope {
        // Every runner that can serve the repository; the page itself needs an
        // administrator, and the tokens below are checked again by the service.
        ManagedScope::Repository(repo) => actions.runners_for_repository(repo).await?,
        _ => actions.runners(user, scope).await.map_err(hidden)?,
    };
    let mut tokens = actions.registration_tokens(user, scope).await.map_err(hidden)?;
    tokens.sort_by_key(|token| std::cmp::Reverse((token.is_usable(now), token.created_at)));
    tokens.truncate(LISTED_TOKENS);
    Ok(RunnersSection {
        subject,
        runners: runners.iter().map(|runner| RunnerRow::new(runner, runner.scope == own, base_url, now)).collect(),
        tokens: tokens.iter().map(|token| TokenRow::new(token, now)).collect(),
        issued: outcome.issued,
        error: outcome.error,
        action_url: base_url.to_owned(),
        lifetimes: &LIFETIMES,
    })
}

/// Applies a token form for `user` in `scope`.
async fn apply(state: &AppState, user: &User, scope: ManagedScope<'_>, form: &TokenForm) -> WebResult<Outcome> {
    let actions = actions(state)?;
    match form.action.as_str() {
        "create" => {
            let labels: Result<Vec<RunnerLabel>, _> =
                form.labels.split(',').map(str::trim).filter(|label| !label.is_empty()).map(str::parse).collect();
            let labels = match labels {
                Ok(labels) => labels,
                Err(error) => return Ok(Outcome { issued: None, error: FormError::from_message(&format!("{error}")) }),
            };
            let options = RegistrationOptions {
                lifetime: std::time::Duration::from_secs(form.lifetime.unwrap_or(3600)),
                uses: if form.reusable.is_some() { None } else { Some(1) },
                labels: (!labels.is_empty()).then_some(labels),
            };
            match actions.create_registration_token(user, scope, options).await {
                Ok(issued) => {
                    let pinned = form.labels.trim();
                    let shown = IssuedToken::new(issued.secret.expose().to_owned(), state.config.server.base_url(), Some(pinned));
                    Ok(Outcome { issued: Some(shown), error: FormError::default() })
                }
                Err(ServiceError::Invalid(message)) => Ok(Outcome { issued: None, error: FormError::from_message(&message) }),
                Err(error) => Err(hidden(error)),
            }
        }
        "revoke" => {
            let token = form.token.map(RegistrationTokenId).ok_or_else(|| WebError::BadRequest("which token?".into()))?;
            actions.revoke_registration_token(user, scope, token).await.map_err(hidden)?;
            Ok(Outcome::default())
        }
        _ => Err(WebError::BadRequest("unknown action".into())),
    }
}

/// Applies a runner button for `user` in `scope`.
async fn apply_runner(state: &AppState, user: &User, scope: ManagedScope<'_>, runner: &str, form: &RunnerForm) -> WebResult<()> {
    let actions = actions(state)?;
    let id = RunnerId(runner.parse().map_err(|_| WebError::NotFound)?);
    match form.action.as_str() {
        "disable" => actions.set_runner_disabled(user, scope, id, true).await.map(drop),
        "enable" => actions.set_runner_disabled(user, scope, id, false).await.map(drop),
        "remove" => actions.remove_runner(user, scope, id).await,
        _ => return Err(WebError::BadRequest("unknown action".into())),
    }
    .map_err(hidden)
}

// ------------------------------------------------------------------ repository

async fn load_admin(state: &AppState, viewer: &Viewer, owner: &str, repo: &str) -> WebResult<RepoPage> {
    actions(state)?;
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    if loaded.ctx.access.can_admin() { Ok(loaded) } else { Err(WebError::NotFound) }
}

async fn repo_page(state: &AppState, user: &User, loaded: &RepoPage, page: Page, outcome: Outcome) -> WebResult<RepoRunnersPage> {
    let actions_url = format!("{}/settings/actions", loaded.urls.home());
    let base = format!("{actions_url}/runners");
    let section = section(state, user, ManagedScope::Repository(&loaded.ctx), &base, "this repository".to_owned(), outcome).await?;
    Ok(RepoRunnersPage {
        page: page.titled(format!("Runners · Settings · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(state, "settings"),
        actions_url,
        section,
    })
}

/// `GET /{owner}/{repo}/settings/actions/runners`
pub async fn repository(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
) -> WebResult<RepoRunnersPage> {
    let loaded = load_admin(&state, &viewer, &owner, &repo).await?;
    repo_page(&state, &user, &loaded, page, Outcome::default()).await
}

/// `POST /{owner}/{repo}/settings/actions/runners`
pub async fn repository_tokens(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<TokenForm>,
) -> WebResult<RepoRunnersPage> {
    let loaded = load_admin(&state, &viewer, &owner, &repo).await?;
    let outcome = apply(&state, &user, ManagedScope::Repository(&loaded.ctx), &form).await?;
    repo_page(&state, &user, &loaded, page, outcome).await
}

/// `POST /{owner}/{repo}/settings/actions/runners/{runner}`
pub async fn repository_runner(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    Path((owner, repo, runner)): Path<(String, String, String)>,
    Form(form): Form<RunnerForm>,
) -> WebResult<Redirect> {
    let loaded = load_admin(&state, &viewer, &owner, &repo).await?;
    apply_runner(&state, &user, ManagedScope::Repository(&loaded.ctx), &runner, &form).await?;
    Ok(Redirect::to(&format!("{}/settings/actions/runners", loaded.urls.home())))
}

// ------------------------------------------------------------------ organisation

async fn org_page(state: &AppState, user: &User, owner: &str, page: Page, outcome: Outcome) -> WebResult<OrgRunnersPage> {
    actions(state)?;
    let ctx = state.services.orgs.open(Some(user), owner).await?;
    if !ctx.can_manage {
        return Err(WebError::NotFound);
    }
    let actions_url = format!("/{}/settings/actions", ctx.org.name());
    let base = format!("{actions_url}/runners");
    let scope = ManagedScope::Owner(&ctx.org.account);
    let section = section(state, user, scope, &base, ctx.org.name().to_string(), outcome).await?;
    Ok(OrgRunnersPage {
        page: page.titled(format!("Runners · Settings · {}", ctx.org.label())),
        header: OrgHeader::new(&ctx, OrgTab::Settings),
        actions_url,
        section,
    })
}

/// `GET /{owner}/settings/actions/runners`
pub async fn organization(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path(owner): Path<String>,
) -> WebResult<OrgRunnersPage> {
    org_page(&state, &user, &owner, page, Outcome::default()).await
}

/// `POST /{owner}/settings/actions/runners`
pub async fn organization_tokens(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path(owner): Path<String>,
    Form(form): Form<TokenForm>,
) -> WebResult<OrgRunnersPage> {
    actions(&state)?;
    let ctx = state.services.orgs.open(Some(&user), &owner).await?;
    if !ctx.can_manage {
        return Err(WebError::NotFound);
    }
    let outcome = apply(&state, &user, ManagedScope::Owner(&ctx.org.account), &form).await?;
    org_page(&state, &user, &owner, page, outcome).await
}

/// `POST /{owner}/settings/actions/runners/{runner}`
pub async fn organization_runner(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, runner)): Path<(String, String)>,
    Form(form): Form<RunnerForm>,
) -> WebResult<Redirect> {
    actions(&state)?;
    let ctx = state.services.orgs.open(Some(&user), &owner).await?;
    if !ctx.can_manage {
        return Err(WebError::NotFound);
    }
    apply_runner(&state, &user, ManagedScope::Owner(&ctx.org.account), &runner, &form).await?;
    Ok(Redirect::to(&format!("/{}/settings/actions/runners", ctx.org.name())))
}

// ------------------------------------------------------------------ the user's own account

async fn user_page(state: &AppState, user: &User, page: Page, outcome: Outcome) -> WebResult<UserRunnersPage> {
    let section =
        section(state, user, ManagedScope::Owner(&user.account), "/settings/actions/runners", "your repositories".to_owned(), outcome)
            .await?;
    Ok(UserRunnersPage {
        page: page.titled("Runners"),
        settings_nav: SettingsSection::Actions.nav(),
        actions_url: "/settings/actions".to_owned(),
        section,
    })
}

/// `GET /settings/actions/runners`
pub async fn personal(State(state): State<AppState>, SignedIn(user): SignedIn, page: Page) -> WebResult<UserRunnersPage> {
    actions(&state)?;
    user_page(&state, &user, page, Outcome::default()).await
}

/// `POST /settings/actions/runners`
pub async fn personal_tokens(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Form(form): Form<TokenForm>,
) -> WebResult<UserRunnersPage> {
    let outcome = apply(&state, &user, ManagedScope::Owner(&user.account), &form).await?;
    user_page(&state, &user, page, outcome).await
}

/// `POST /settings/actions/runners/{runner}`
pub async fn personal_runner(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path(runner): Path<String>,
    Form(form): Form<RunnerForm>,
) -> WebResult<Redirect> {
    apply_runner(&state, &user, ManagedScope::Owner(&user.account), &runner, &form).await?;
    Ok(Redirect::to("/settings/actions/runners"))
}

// ------------------------------------------------------------------ the instance

async fn admin_page(state: &AppState, admin: &User, page: Page, outcome: Outcome) -> WebResult<AdminRunnersPage> {
    let section =
        section(state, admin, ManagedScope::Instance, "/admin/runners", "every owner allowed instance runners".to_owned(), outcome).await?;
    Ok(AdminRunnersPage { page: page.titled("Runners · Administration"), admin_nav: AdminSection::Runners.nav(), section })
}

/// `GET /admin/runners`
pub async fn instance(State(state): State<AppState>, SiteAdmin(admin): SiteAdmin, page: Page) -> WebResult<AdminRunnersPage> {
    actions(&state)?;
    admin_page(&state, &admin, page, Outcome::default()).await
}

/// `POST /admin/runners`
pub async fn instance_tokens(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Form(form): Form<TokenForm>,
) -> WebResult<AdminRunnersPage> {
    let outcome = apply(&state, &admin, ManagedScope::Instance, &form).await?;
    admin_page(&state, &admin, page, outcome).await
}

/// `POST /admin/runners/{runner}`
pub async fn instance_runner(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    Path(runner): Path<String>,
    Form(form): Form<RunnerForm>,
) -> WebResult<Redirect> {
    apply_runner(&state, &admin, ManagedScope::Instance, &runner, &form).await?;
    Ok(Redirect::to("/admin/runners"))
}
