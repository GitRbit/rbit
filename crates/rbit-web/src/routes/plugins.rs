//! Administrator-only live plugin lifecycle controls using host page middleware.

use axum::{
    Form, Router,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use serde::Deserialize;

use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::SiteAdmin,
    views::{layout::Page, plugins::PluginsPage},
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/admin/plugins", get(index)).route("/admin/plugins/actions", axum::routing::post(set_actions))
}

async fn page(state: &AppState, page: Page) -> PluginsPage {
    let status = state.services.actions_status().await;
    let (state_label, generation, retry_disable, busy) = status.map_or_else(
        || ("Unavailable in this build".to_owned(), None, false, false),
        |status| {
            use rbit_plugin_api::PluginState;
            let label = match status.state {
                PluginState::Installed | PluginState::Disabled => "Disabled",
                PluginState::Waiting => "Waiting for dependencies",
                PluginState::Preparing => "Enabling",
                PluginState::Active => "Enabled",
                PluginState::Quiescing => "Disabling",
                PluginState::Failed => "Failed",
                PluginState::RestartRequired => "Cleanup incomplete",
            };
            let retry = status.generation.is_some() && matches!(status.state, PluginState::Failed | PluginState::RestartRequired);
            let busy = matches!(status.state, PluginState::Preparing | PluginState::Quiescing);
            (label.to_owned(), status.generation.map(rbit_plugin_api::Generation::get), retry, busy)
        },
    );
    let mut view = PluginsPage::new(page, state.services.actions_available(), state.services.actions.is_some(), state_label, generation);
    view.retry_disable = retry_disable;
    view.busy = busy;
    view
}

async fn index(State(state): State<AppState>, _admin: SiteAdmin, shell: Page) -> PluginsPage {
    page(&state, shell).await
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
            let mut page = page(&state, shell).await;
            page.error = Some(message);
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(error) => Err(WebError::from(error)),
    }
}
