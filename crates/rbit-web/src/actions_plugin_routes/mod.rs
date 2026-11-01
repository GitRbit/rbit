//! Compiled built-in Actions documents and forms, owned by its active plugin scope.
mod environments;
mod jobs;
mod run_pages;
mod runners;
mod runs;
mod settings;
mod summary;

use crate::{AppState, error::WebError, extract::Viewer, state::WeakAppState, views::layout::Page};
use askama::Template;
use rbit_core::User;
use rbit_messages::LocaleSnapshot;
use rbit_services::{
    ActionsRouteBundle, ServiceError,
    plugin_pages::{CompiledPage, FormActions, MutationContext, MutationError, PageContext, PageError},
};
use rbit_ui::template::Localized;
use std::sync::Arc;

pub(crate) fn bundle(weak: WeakAppState) -> Result<ActionsRouteBundle, ServiceError> {
    let mut bundle = ActionsRouteBundle::default();
    for routes in [
        runs::bundle(weak.clone())?,
        jobs::bundle(weak.clone())?,
        summary::bundle(weak.clone())?,
        settings::bundle(&weak)?,
        environments::bundle(weak.clone())?,
        runners::bundle(weak)?,
    ] {
        bundle.pages.extend(routes.pages);
        bundle.mutations.extend(routes.mutations);
    }
    Ok(bundle)
}

fn declaration(error: impl std::fmt::Display) -> ServiceError {
    ServiceError::Invalid(format!("invalid built-in Actions route: {error}"))
}

async fn actor(state: &AppState, id: i64) -> Result<User, PageError> {
    state
        .services
        .context()
        .database
        .accounts()
        .find_active_user(rbit_core::AccountId(id))
        .await
        .map_err(|_| PageError::Unavailable)?
        .ok_or(PageError::Forbidden)
}

async fn principal(state: &AppState, context: &PageContext) -> Result<Viewer, PageError> {
    Ok(Viewer(match &context.actor {
        Some(a) => Some(Arc::new(actor(state, a.account_id).await?)),
        None => None,
    }))
}

fn page(state: &AppState, viewer: &Viewer, locale: LocaleSnapshot, forms: FormActions, path: &str) -> Page {
    let mut page = Page::new(state, viewer.user(), path);
    page.locale = locale;
    if let Some(appearance) = forms.appearance() {
        page.theme = crate::themes::Theme::parse(appearance.theme()).unwrap_or_default();
        page.has_unread_notifications = viewer.user().is_some() && appearance.has_unread_notifications();
    }
    page.plugin_forms = forms;
    page
}

async fn request(
    weak: &WeakAppState,
    context: &PageContext,
    forms: FormActions,
) -> Result<(AppState, Viewer, Page, String, String), PageError> {
    let state = weak.upgrade().ok_or(PageError::Unavailable)?;
    let viewer = principal(&state, context).await?;
    let repo = context.repository.as_ref().ok_or(PageError::Forbidden)?;
    let page = page(&state, &viewer, context.locale.clone(), forms, &format!("/{}/{}/-/plugins/rbit.actions", repo.owner, repo.name));
    Ok((state, viewer, page, repo.owner.clone(), repo.name.clone()))
}

async fn mutation(weak: &WeakAppState, context: &MutationContext) -> Result<(AppState, User, Viewer, Page, String, String), MutationError> {
    let state = weak.upgrade().ok_or(MutationError::Unavailable)?;
    let user = actor(&state, context.actor.account_id).await.map_err(|_| MutationError::Forbidden)?;
    let viewer = Viewer(Some(Arc::new(user.clone())));
    let repo = context.repository.as_ref().ok_or(MutationError::Forbidden)?;
    let page = page(
        &state,
        &viewer,
        context.locale.clone(),
        context.forms().clone(),
        &format!("/{}/{}/-/plugins/rbit.actions", repo.owner, repo.name),
    );
    Ok((state, user, viewer, page, repo.owner.clone(), repo.name.clone()))
}

#[allow(clippy::needless_pass_by_value, reason = "map_err adapter consumes and drops native errors without exposing their contents")]
fn page_error(error: WebError) -> PageError {
    match error {
        WebError::NotFound | WebError::Forbidden => PageError::Forbidden,
        WebError::BadRequest(_) => PageError::Invalid("invalid Actions request"),
        _ => PageError::Unavailable,
    }
}
#[allow(clippy::needless_pass_by_value, reason = "map_err adapter consumes and drops native errors without exposing their contents")]
fn mutation_error(error: WebError) -> MutationError {
    match error {
        WebError::NotFound | WebError::Forbidden => MutationError::Forbidden,
        WebError::BadRequest(_) => MutationError::InvalidForm,
        _ => MutationError::Unavailable,
    }
}

fn document<T: Template>(template: T, locale: LocaleSnapshot) -> Result<CompiledPage<Localized<T>>, PageError> {
    CompiledPage::new("Actions", Localized::new(template, locale))
}

fn repo_url(owner: &str, repo: &str, path: &str) -> String {
    format!("/{owner}/{repo}/-/plugins/rbit.actions/{path}")
}
pub(crate) fn global_url(path: &str) -> String {
    format!("/-/plugins/rbit.actions/{path}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn native_actions_routes_blueprints_do_not_keep_a_dropped_host_alive() {
        let directory = tempfile::tempdir().unwrap();
        let mut config = rbit_core::Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.updates.check_for_updates = false;
        config.actions.enabled = true;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = rbit_storage::Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = rbit_storage::KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = rbit_services::Services::new(config.clone(), &database, &kv).await.unwrap();
        let state = AppState::new(config, services.clone());
        crate::prepare_plugin_routes(&state).await.unwrap();
        crate::prepare_plugin_routes(&state).await.unwrap();
        let weak = state.downgrade();
        let router = crate::router(state.clone());
        drop(state);
        assert!(weak.upgrade().is_some());
        drop(router);
        assert!(weak.upgrade().is_none(), "active Actions route factories must capture only weak host references");
        services.shutdown().await;
    }
}
