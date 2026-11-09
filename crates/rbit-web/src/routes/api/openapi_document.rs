//! The served `OpenAPI` document: `openapi.json` with each area's fragment merged in once,
//! with or without the Actions fragments, and the `GET /openapi.json` handler.

use std::sync::LazyLock;

use axum::{extract::State, http::header, response::IntoResponse};
use serde_json::Value;

use super::auth::Caller;
use crate::AppState;

/// The hand-written `OpenAPI` 3.1 description of [`super::table`].
const OPENAPI: &str = include_str!("openapi.json");

/// The `OpenAPI` fragments of the Actions endpoints, merged only when Actions is enabled.
// Actions: integration (I).
pub(super) const ACTIONS_FRAGMENTS: [(&str, &str); 3] = [
    ("actions", include_str!("openapi/actions.json")),
    ("environments", include_str!("openapi/environments.json")),
    // Actions: settings API.
    ("actions-permissions", include_str!("openapi/actions-permissions.json")),
];

/// The `OpenAPI` fragments of wave B's and milestone 3's areas, merged into [`OPENAPI`]. Each is shaped
/// `{"paths": {…}, "components": {"schemas": {…}}}`.
pub(super) const FRAGMENTS: [(&str, &str); 9] = [
    ("issues", include_str!("openapi/issues.json")),
    ("notifications", include_str!("openapi/notifications.json")),
    ("pulls", include_str!("openapi/pulls.json")),
    ("protection", include_str!("openapi/protection.json")),
    // Milestone 3: REST API continuation (A).
    ("collaborators", include_str!("openapi/collaborators.json")),
    ("teams", include_str!("openapi/teams.json")),
    ("releases", include_str!("openapi/releases.json")),
    ("forks", include_str!("openapi/forks.json")),
    // Milestone 3: commit statuses (S).
    ("statuses", include_str!("openapi/statuses.json")),
];

/// The served document: `openapi.json` with every fragment's paths and schemas merged in.
/// Built once per Actions setting; a test checks each documents exactly [`super::table`] and that
/// no fragment repeats a path or schema name.
pub(super) static DOCUMENT: LazyLock<String> = LazyLock::new(|| merged_document(false).0.to_string());
pub(super) static DOCUMENT_WITH_ACTIONS: LazyLock<String> = LazyLock::new(|| merged_document(true).0.to_string());

/// The merged document (with the Actions fragments when `actions`) and every path or schema
/// name defined more than once (the first definition wins).
pub(super) fn merged_document(actions: bool) -> (Value, Vec<String>) {
    let mut document: Value = serde_json::from_str(OPENAPI).expect("openapi.json is valid JSON");
    let mut duplicates = Vec::new();
    let actions_fragments: &[(&str, &str)] = if actions { &ACTIONS_FRAGMENTS } else { &[] };
    for (area, fragment) in FRAGMENTS.iter().chain(actions_fragments).copied() {
        let fragment: Value = serde_json::from_str(fragment).unwrap_or_else(|error| panic!("openapi/{area}.json is invalid: {error}"));
        for (section, pointer) in [("paths", "/paths"), ("schemas", "/components/schemas")] {
            let Some(entries) = fragment.pointer(pointer).and_then(Value::as_object) else { continue };
            let Some(target) = document.pointer_mut(pointer).and_then(Value::as_object_mut) else { continue };
            for (name, value) in entries {
                if target.contains_key(name) {
                    duplicates.push(format!("{section} {name} (openapi/{area}.json)"));
                } else {
                    target.insert(name.clone(), value.clone());
                }
            }
        }
    }
    (document, duplicates)
}

/// `GET /openapi.json`
pub(super) async fn openapi(State(state): State<AppState>, _caller: Caller) -> impl IntoResponse {
    let document = if state.config.actions.enabled { DOCUMENT_WITH_ACTIONS.as_str() } else { DOCUMENT.as_str() };
    ([(header::CONTENT_TYPE, "application/json")], document)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use rbit_core::Config;
    use serde_json::Value;

    use super::{
        super::{PREFIX, table},
        *,
    };

    /// `{*branch}` in axum is `{branch}` in `OpenAPI`.
    fn openapi_path(path: &str) -> String {
        path.replace("{*", "{")
    }

    #[test]
    fn openapi_documents_exactly_the_routes() {
        // Actions: foundation (F). Both with Actions disabled (the default) and enabled.
        for actions in [false, true] {
            let mut config = Config::default();
            config.actions.enabled = actions;
            documents_exactly_the_routes(&config);
        }
        let disabled = table(&Config::default());
        assert!(disabled.iter().all(|route| !route.path.contains("/actions")), "no Actions route while Actions is disabled");
    }

    fn documents_exactly_the_routes(config: &Config) {
        let (document, duplicates) = merged_document(config.actions.enabled);
        assert!(duplicates.is_empty(), "defined more than once: {duplicates:?}");
        let served = if config.actions.enabled { &*DOCUMENT_WITH_ACTIONS } else { &*DOCUMENT };
        assert_eq!(serde_json::from_str::<Value>(served).unwrap(), document, "the served document is the merged one");
        for (area, fragment) in FRAGMENTS.iter().chain(&ACTIONS_FRAGMENTS).copied() {
            let fragment: Value = serde_json::from_str(fragment).unwrap();
            assert!(fragment["paths"].is_object() && fragment["components"]["schemas"].is_object(), "openapi/{area}.json shape");
        }
        assert_eq!(document["openapi"].as_str().map(|v| v.starts_with("3.1")), Some(true), "OpenAPI 3.1");
        assert_eq!(document["servers"][0]["url"], PREFIX);

        let methods = ["get", "post", "put", "patch", "delete", "head", "options", "trace"];
        let mut documented = BTreeSet::new();
        for (path, item) in document["paths"].as_object().expect("paths") {
            for method in methods {
                if let Some(operation) = item.get(method) {
                    assert!(operation["operationId"].is_string(), "{method} {path} has an operationId");
                    assert!(operation["responses"].is_object(), "{method} {path} documents its responses");
                    documented.insert((method.to_owned(), path.clone()));
                }
            }
        }
        let routed: BTreeSet<_> = table(config).iter().map(|route| (route.method.as_str().to_owned(), openapi_path(&route.path))).collect();
        let missing: Vec<_> = routed.difference(&documented).collect();
        let extra: Vec<_> = documented.difference(&routed).collect();
        assert!(missing.is_empty() && extra.is_empty(), "undocumented: {missing:?}; documented but not routed: {extra:?}");
        assert_eq!(routed.len(), table(config).len(), "no route is listed twice");
    }
}
