//! Administrator-only live plugin lifecycle controls using host page middleware.

use axum::{
    Form, Router,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use serde::Deserialize;

use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::SiteAdmin,
    views::{
        layout::Page,
        plugins::{LanguagePack, PluginsPage},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/plugins", get(index))
        .route("/admin/plugins/actions", axum::routing::post(set_actions))
        .route("/admin/plugins/i18n", axum::routing::post(set_i18n))
        .route("/admin/plugins/i18n/packs", axum::routing::post(install_pack).layer(DefaultBodyLimit::max(2 * 1024 * 1024 + 8 * 1024)))
        .route("/admin/plugins/i18n/packs/{id}/remove", axum::routing::post(remove_pack))
}

async fn page(state: &AppState, page: Page, actor: &rbit_core::User) -> WebResult<PluginsPage> {
    let status = state.services.actions_status().await.map(|status| (status.state, status.generation));
    let (state_label, generation, retry_disable, busy) = presentation(status);
    let mut view = PluginsPage::new(page, state.services.actions_available(), state.services.actions.is_some(), state_label, generation);
    view.retry_disable = retry_disable;
    view.busy = busy;
    view.i18n_available = state.services.i18n_available();
    view.i18n_active = state.services.i18n_enabled();
    let status = state.services.i18n_status().await.map(|status| (status.state, status.generation));
    view.i18n_problem = status
        .is_some_and(|(state, _)| matches!(state, rbit_plugin_api::PluginState::Failed | rbit_plugin_api::PluginState::RestartRequired));
    (view.i18n_status, view.i18n_generation, view.i18n_retry_disable, view.i18n_busy) = presentation(status);
    view.packs = state
        .services
        .language_packs(actor)
        .await?
        .into_iter()
        .map(|pack| LanguagePack {
            id: pack.id,
            locale: pack.locale,
            version: pack.version,
            message_count: pack.message_count,
            message_total: pack.message_total,
        })
        .collect();
    Ok(view)
}

fn presentation(status: Option<(rbit_plugin_api::PluginState, Option<rbit_plugin_api::Generation>)>) -> (String, Option<u64>, bool, bool) {
    use rbit_plugin_api::PluginState;
    let Some((state, generation)) = status else {
        return (crate::i18n::text("core", "plugin-state-unavailable").into_owned(), None, false, false);
    };
    let key = match state {
        PluginState::Installed | PluginState::Disabled => "plugin-state-disabled",
        PluginState::Waiting => "plugin-state-waiting",
        PluginState::Preparing => "plugin-state-enabling",
        PluginState::Active => "plugin-state-enabled",
        PluginState::Quiescing => "plugin-state-disabling",
        PluginState::Failed => "plugin-state-failed",
        PluginState::RestartRequired => "plugin-state-restart-required",
    };
    let retry = generation.is_some() && matches!(state, PluginState::Failed | PluginState::RestartRequired);
    let busy = matches!(state, PluginState::Preparing | PluginState::Quiescing);
    (crate::i18n::text("core", key).into_owned(), generation.map(rbit_plugin_api::Generation::get), retry, busy)
}

async fn index(State(state): State<AppState>, SiteAdmin(admin): SiteAdmin, shell: Page) -> WebResult<PluginsPage> {
    page(&state, shell, &admin).await
}

#[derive(Debug, Deserialize)]
struct Toggle {
    enabled: bool,
}

async fn set_actions(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    shell: Page,
    Form(form): Form<Toggle>,
) -> WebResult<Response> {
    match state.services.set_actions_enabled(&admin, form.enabled).await {
        Ok(()) => Ok(([("hx-redirect", "/admin/plugins"), (header::CACHE_CONTROL.as_str(), "no-store")], Redirect::to("/admin/plugins"))
            .into_response()),
        Err(rbit_services::ServiceError::Invalid(message)) => {
            let mut page = page(&state, shell, &admin).await?;
            page.error = Some(message);
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(error) => Err(WebError::from(error)),
    }
}

fn updated() -> Response {
    ([("hx-redirect", "/admin/plugins"), (header::CACHE_CONTROL.as_str(), "no-store")], Redirect::to("/admin/plugins")).into_response()
}

async fn set_i18n(State(state): State<AppState>, SiteAdmin(admin): SiteAdmin, Form(form): Form<Toggle>) -> WebResult<Response> {
    state.services.set_i18n_enabled(&admin, form.enabled).await?;
    Ok(updated())
}

async fn install_pack(State(state): State<AppState>, SiteAdmin(admin): SiteAdmin, mut multipart: Multipart) -> WebResult<Response> {
    // Allow bounded multipart framing; the service separately caps archive bytes at 2 MiB.
    let field = multipart
        .next_field()
        .await
        .map_err(|_| WebError::BadRequest(crate::i18n::text("core", "pack-invalid-upload").into_owned()))?
        .ok_or_else(|| WebError::BadRequest(crate::i18n::text("core", "pack-invalid-upload").into_owned()))?;
    if field.name() != Some("pack") {
        return Err(WebError::BadRequest(crate::i18n::text("core", "pack-invalid-upload").into_owned()));
    }
    let bytes = field.bytes().await.map_err(|_| WebError::BadRequest(crate::i18n::text("core", "pack-invalid-upload").into_owned()))?;
    if multipart
        .next_field()
        .await
        .map_err(|_| WebError::BadRequest(crate::i18n::text("core", "pack-invalid-upload").into_owned()))?
        .is_some()
    {
        return Err(WebError::BadRequest(crate::i18n::text("core", "pack-invalid-upload").into_owned()));
    }
    state.services.install_language_pack(&admin, &bytes).await?;
    Ok(updated())
}

async fn remove_pack(State(state): State<AppState>, SiteAdmin(admin): SiteAdmin, Path(id): Path<String>) -> WebResult<Response> {
    state.services.remove_language_pack(&admin, &id).await?;
    Ok(updated())
}
