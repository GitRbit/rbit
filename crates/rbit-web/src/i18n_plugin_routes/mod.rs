//! Built-in language UI uses the same scoped compiled routes as native extensions.

use crate::AppState;

#[cfg(feature = "i18n")]
mod language;
#[cfg(feature = "i18n")]
mod management;

/// Bind native route blueprints before serving. Clones of this host share one
/// initialization; a different host cannot replace an existing host's callbacks.
pub async fn prepare_plugin_routes(state: &AppState) -> Result<(), rbit_services::ServiceError> {
    state
        .plugin_routes_ready
        .get_or_try_init(|| async {
            #[cfg(feature = "i18n")]
            let (active, management) = (language::bundle(state.downgrade())?, management::bundle(state.downgrade())?);
            #[cfg(not(feature = "i18n"))]
            let (active, management) = (rbit_services::I18nRouteBundle::default(), rbit_services::I18nRouteBundle::default());
            state.services.bind_i18n_routes(active, management).await
        })
        .await
        .copied()
}

#[cfg(feature = "i18n")]
async fn actor(state: &AppState, id: i64) -> Result<rbit_core::User, rbit_services::plugin_pages::MutationError> {
    state
        .services
        .context()
        .database
        .accounts()
        .find_active_user(rbit_core::AccountId(id))
        .await
        .map_err(|_| rbit_services::plugin_pages::MutationError::Unavailable)?
        .ok_or(rbit_services::plugin_pages::MutationError::Forbidden)
}

#[cfg(feature = "i18n")]
fn title(messages: &rbit_messages::LocaleSnapshot, key: &str) -> String {
    let text = messages.text("core", key);
    let mut title = String::new();
    for character in text.chars() {
        let character = if character.is_control() { ' ' } else { character };
        if title.len() + character.len_utf8() > 160 {
            break;
        }
        title.push(character);
    }
    let title = title.trim();
    if title.is_empty() { "Languages".to_owned() } else { title.to_owned() }
}

#[cfg(feature = "i18n")]
fn declaration(error: impl std::fmt::Display) -> rbit_services::ServiceError {
    rbit_services::ServiceError::Invalid(format!("invalid built-in language route: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn native_i18n_routes_blueprints_do_not_keep_a_dropped_host_alive() {
        let directory = tempfile::tempdir().unwrap();
        let mut config = rbit_core::Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.updates.check_for_updates = false;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = rbit_storage::Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = rbit_storage::KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = rbit_services::Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        let state = AppState::new(config, services.clone());
        prepare_plugin_routes(&state).await.unwrap();
        prepare_plugin_routes(&state).await.unwrap();
        let weak = state.downgrade();
        let router = crate::router(state.clone());
        drop(state);
        assert!(weak.upgrade().is_some(), "the live router still owns its host");
        drop(router);
        assert!(weak.upgrade().is_none(), "retained service blueprints must contain only weak host references");
        services.shutdown().await;
    }
}
