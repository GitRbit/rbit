//! Spec runtime contracts and original validation.
use super::limits::Env;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Job-level `concurrency`, evaluated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConcurrencyGroup {
    pub group: String,
    pub cancel_in_progress: bool,
}

/// `defaults.run`, as written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunDefaults {
    pub shell: Option<String>,
    pub working_directory: Option<String>,
}

/// The job `container`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Container {
    /// Evaluated; it overrides the image of the runner's label.
    pub image: String,
    pub env: Env,
    pub credentials: Option<RegistryCredentials>,
    /// `options`, evaluated and checked: `docker create` arguments ([`crate::options`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// `volumes`, evaluated and checked: `name:/path[:ro]`, named volumes only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<String>,
}

/// One `services.<id>` container (container backend only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Service {
    /// The key under `services`, also its network alias.
    pub id: String,
    /// Evaluated.
    pub image: String,
    pub env: Env,
    pub credentials: Option<RegistryCredentials>,
    /// As [`crate::spec::Container::options`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// As [`crate::spec::Container::volumes`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<String>,
}

/// `credentials` of a container or service, as templates (normally
/// `${{ secrets.REGISTRY_PASSWORD }}`). The runner evaluates them and logs in with
/// `--password-stdin`. `Debug` hides the password in case it was written out literally.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryCredentials {
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for RegistryCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegistryCredentials").field("username", &self.username).field("password", &"***").finish()
    }
}

/// One step. Everything here is a template the runner evaluates at step time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepTemplate {
    /// `id`, for the `steps` context.
    pub id: Option<String>,
    pub name: Option<String>,
    /// `if`; `None` means `success()`.
    #[serde(rename = "if")]
    pub condition: Option<String>,
    pub action: StepAction,
    pub shell: Option<String>,
    pub working_directory: Option<String>,
    pub env: Env,
    /// `continue-on-error` as written (`true`, or an expression).
    pub continue_on_error: Option<String>,
    /// `timeout-minutes` as written (a number, or an expression).
    pub timeout_minutes: Option<String>,
}

/// What a step does: exactly one of `run` and `uses`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StepAction {
    /// A `run:` script, interpolated as text and written to a file.
    Run(String),
    /// A built-in action.
    Uses(Builtin),
    // Actions: third-party actions.
    /// An action the runner fetches and runs.
    Action(ActionStep),
}

// Actions: third-party actions.
/// A `uses:` that is not built in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionStep {
    /// `uses:` as written, for display.
    pub uses: String,
    pub source: ActionSource,
    /// `with:`, as templates; the runner checks them against the action's inputs.
    pub with: BTreeMap<String, String>,
}

/// Where an action comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionSource {
    /// `owner/repo[/path]@reference`, from the runner's action source.
    Repository { owner: String, repo: String, path: String, reference: String },
    /// `./path`: a directory of the workspace.
    Local { path: String },
    /// `docker://image`.
    Docker { image: String },
}

/// A resolved `uses:` (§12.2 built-ins).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Builtin {
    /// `uses:` as written (`actions/checkout@v4`), for display.
    pub uses: String,
    pub kind: BuiltinKind,
    /// The major version of the rbit built-in it resolved to.
    pub version: u32,
    /// `with:`, as templates; the keys were checked against the built-in.
    pub with: BTreeMap<String, String>,
}

/// The built-in actions `rbit-runner` carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinKind {
    /// `rbit/checkout`.
    Checkout,
    /// `rbit/cache`: restore now, save after the job.
    Cache,
    /// `actions/cache/restore`.
    CacheRestore,
    /// `actions/cache/save`.
    CacheSave,
    /// `rbit/upload-artifact`.
    UploadArtifact,
    /// `rbit/download-artifact`.
    DownloadArtifact,
}
