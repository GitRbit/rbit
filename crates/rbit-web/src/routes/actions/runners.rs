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

use axum::extract::{Path, State};
use rbit_core::{
    Timestamp, User,
    actions::{RegistrationTokenId, RunnerId, RunnerLabel},
};
use rbit_services::{
    ActionsService, ServiceError,
    actions::runners::{ManagedScope, RegistrationOptions},
};

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
#[derive(Debug, Default)]
pub struct TokenForm {
    pub(crate) action: String,
    pub(crate) lifetime: Option<u64>,
    pub(crate) labels: String,
    pub(crate) reusable: Option<String>,
    pub(crate) token: Option<i64>,
}

/// The runner buttons.
#[derive(Debug, Default)]
pub struct RunnerForm {
    pub(crate) action: String,
}

/// What a token form did: a token to show once, or an error to show.
#[derive(Debug, Default)]
pub(crate) struct Outcome {
    pub(crate) issued: Option<IssuedToken>,
    pub(crate) error: FormError,
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
    let runner_url = format!("{}runner", base_url.strip_suffix("runners").unwrap_or(base_url));
    Ok(RunnersSection {
        subject,
        runners: runners.iter().map(|runner| RunnerRow::new(runner, runner.scope == own, &runner_url, now)).collect(),
        tokens: tokens.iter().map(|token| TokenRow::new(token, now)).collect(),
        issued: outcome.issued,
        error: outcome.error,
        action_url: base_url.to_owned(),
        lifetimes: &LIFETIMES,
    })
}

/// Applies a token form for `user` in `scope`.
pub(crate) async fn apply(state: &AppState, user: &User, scope: ManagedScope<'_>, form: &TokenForm) -> WebResult<Outcome> {
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
pub(crate) async fn apply_runner(state: &AppState, user: &User, scope: ManagedScope<'_>, runner: &str, form: &RunnerForm) -> WebResult<()> {
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

pub(crate) async fn load_admin(state: &AppState, viewer: &Viewer, owner: &str, repo: &str) -> WebResult<RepoPage> {
    actions(state)?;
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    if loaded.ctx.access.can_admin() { Ok(loaded) } else { Err(WebError::NotFound) }
}

pub(crate) async fn repo_page(
    state: &AppState,
    user: &User,
    loaded: &RepoPage,
    page: Page,
    outcome: Outcome,
) -> WebResult<RepoRunnersPage> {
    let actions_url = format!("{}/-/plugins/rbit.actions/settings", loaded.urls.home());
    let base = format!("{actions_url}/runners");
    let section = section(state, user, ManagedScope::Repository(&loaded.ctx), &base, "this repository".to_owned(), outcome).await?;
    Ok(RepoRunnersPage {
        page: page.titled(format!("Runners · Settings · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(state, "settings"),
        actions_url,
        section,
    })
}

/// The repository-scoped native runners document.
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

// ------------------------------------------------------------------ organisation

pub(crate) async fn org_page(state: &AppState, user: &User, owner: &str, page: Page, outcome: Outcome) -> WebResult<OrgRunnersPage> {
    actions(state)?;
    let ctx = state.services.orgs.open(Some(user), owner).await?;
    if !ctx.can_manage {
        return Err(WebError::NotFound);
    }
    let owner = ctx.org.name().to_string();
    let actions_url = format!(
        "/-/plugins/rbit.actions/org/settings?owner={}",
        percent_encoding::utf8_percent_encode(&owner, percent_encoding::NON_ALPHANUMERIC)
    );
    let base = "/-/plugins/rbit.actions/org/runners".to_owned();
    let mut page = page;
    page.plugin_fields = vec![("owner".to_owned(), owner)];
    let scope = ManagedScope::Owner(&ctx.org.account);
    let section = section(state, user, scope, &base, ctx.org.name().to_string(), outcome).await?;
    Ok(OrgRunnersPage {
        page: page.titled(format!("Runners · Settings · {}", ctx.org.label())),
        header: OrgHeader::new(&ctx, OrgTab::Settings),
        actions_url,
        section,
    })
}

/// The native organisation runners document; its host callback resolves the owner.
pub async fn organization(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path(owner): Path<String>,
) -> WebResult<OrgRunnersPage> {
    org_page(&state, &user, &owner, page, Outcome::default()).await
}

// ------------------------------------------------------------------ the user's own account

pub(crate) async fn user_page(state: &AppState, user: &User, page: Page, outcome: Outcome) -> WebResult<UserRunnersPage> {
    let section = section(
        state,
        user,
        ManagedScope::Owner(&user.account),
        "/-/plugins/rbit.actions/settings/runners",
        "your repositories".to_owned(),
        outcome,
    )
    .await?;
    let title = page.locale.text("core", "title-runners");
    Ok(UserRunnersPage {
        page: page.titled(title),
        settings_nav: SettingsSection::Actions.nav(),
        actions_url: "/-/plugins/rbit.actions/settings".to_owned(),
        section,
    })
}

/// The signed-in account native runners document.
pub async fn personal(State(state): State<AppState>, SignedIn(user): SignedIn, page: Page) -> WebResult<UserRunnersPage> {
    actions(&state)?;
    user_page(&state, &user, page, Outcome::default()).await
}

// ------------------------------------------------------------------ the instance

pub(crate) async fn admin_page(state: &AppState, admin: &User, page: Page, outcome: Outcome) -> WebResult<AdminRunnersPage> {
    let section = section(
        state,
        admin,
        ManagedScope::Instance,
        "/-/plugins/rbit.actions/admin/runners",
        "every owner allowed instance runners".to_owned(),
        outcome,
    )
    .await?;
    let title = page.locale.text("core", "title-runners-administration");
    Ok(AdminRunnersPage { page: page.titled(title), admin_nav: AdminSection::Runners.nav(), section })
}

/// The instance-administrator native runners document.
pub async fn instance(State(state): State<AppState>, SiteAdmin(admin): SiteAdmin, page: Page) -> WebResult<AdminRunnersPage> {
    actions(&state)?;
    admin_page(&state, &admin, page, Outcome::default()).await
}
