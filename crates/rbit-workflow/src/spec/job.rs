//! Spec job contracts and original validation.
use super::limits::{JOB_SPEC_VERSION, MAX_NAME_BYTES, MAX_SECRET_BYTES, MAX_SECRETS, RESERVED_NAME_PREFIXES};
use super::policy::ActionsPolicy;
use super::templates::JobTemplate;
use crate::protocol::{TaskId, ValidationError, ensure, valid_task_token, validate_outputs};
use crate::secret::Secret;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// What the server sends a runner for one task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobSpec {
    /// [`crate::spec::JOB_SPEC_VERSION`].
    pub version: u32,
    pub task_id: TaskId,
    pub template: JobTemplate,
    pub contexts: Contexts,
    /// Repository then owner secrets, by name. Empty for untrusted runs.
    pub secrets: BTreeMap<String, Secret>,
    /// The task token: `secrets.GITHUB_TOKEN`, `secrets.RBIT_TOKEN` and `github.token`.
    /// It reaches a step's environment only where the step maps it.
    pub token: Secret,
    pub checkout: CheckoutSource,
    pub cache: CacheScopes,
    // Actions: settings API.
    /// The actions its repository and owner allow. Sent only when it restricts something:
    /// a runner that does not know the field refuses the whole spec rather than ignore it.
    #[serde(default, skip_serializing_if = "ActionsPolicy::is_open")]
    pub actions_policy: ActionsPolicy,
}

/// The contexts the server supplies. `matrix` and `strategy` come from the template; `env`,
/// `steps`, `job`, `runner` and `secrets` are the runner's own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contexts {
    /// The `github` context, `event` included. It never holds `token`: that is
    /// [`crate::spec::JobSpec::token`], which stays redacted.
    pub github: Map<String, Value>,
    /// `inputs` of a `workflow_dispatch` run; empty otherwise.
    pub inputs: Map<String, Value>,
    /// Repository then owner variables. Empty for untrusted runs.
    pub vars: BTreeMap<String, String>,
    /// One entry for every job in [`crate::spec::JobTemplate::needs`].
    pub needs: BTreeMap<String, Needed>,
}

/// One entry of the `needs` context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Needed {
    pub result: JobResult,
    /// Outputs, merged across the job's matrix instances.
    pub outputs: BTreeMap<String, String>,
}

/// `needs.<id>.result`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobResult {
    Success,
    Failure,
    Cancelled,
    Skipped,
}

/// Where the built-in checkout fetches from (§12.5.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckoutSource {
    /// The clone URL, `<server>/<owner>/<repo>`, without credentials.
    pub url: String,
    /// `owner/repo`.
    pub repository: String,
    /// The commit the run is for.
    pub sha: String,
    /// The full ref (`refs/heads/main`, `refs/pull/3/head`).
    #[serde(rename = "ref")]
    pub reference: String,
}

/// The runner-local cache scopes this task may use (§12.5.6), decided by the server from
/// the run's ref and trust.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheScopes {
    pub repository_id: i64,
    /// Scopes to restore from, in order: the run's own, then the default branch's.
    pub read: Vec<String>,
    /// The one scope saves go to (`pull/<n>` for an untrusted run).
    pub write: String,
}

impl JobSpec {
    pub fn validate(&self) -> Result<(), ValidationError> {
        ensure(self.version == JOB_SPEC_VERSION, || format!("job spec version {} is not supported", self.version))?;
        self.template.validate()?;
        ensure(self.secrets.len() <= MAX_SECRETS, || format!("a task has at most {MAX_SECRETS} secrets"))?;
        for (name, value) in &self.secrets {
            validate_secret_name("secret", name)?;
            ensure(value.len() <= MAX_SECRET_BYTES, || format!("secret `{name}` is larger than {MAX_SECRET_BYTES} bytes"))?;
        }
        for name in self.contexts.vars.keys() {
            validate_secret_name("variable", name)?;
        }
        ensure(valid_task_token(self.token.expose()), || "the task token is malformed".to_owned())?;
        ensure(!self.contexts.github.keys().any(|key| key.eq_ignore_ascii_case("token")), || {
            "the `github` context must not carry the token".to_owned()
        })?;
        // Actions: reusable workflows. Where a call is involved the context is keyed by the
        // names the workflow used (`need_names`), not by the jobs they stand for.
        let named = !self.template.calls.is_empty() || !self.template.need_names.is_empty();
        let expected: std::collections::BTreeSet<&String> =
            if named { self.template.need_names.keys().collect() } else { self.template.needs.iter().collect() };
        ensure(self.contexts.needs.keys().eq(expected), || "the `needs` context must have one entry for every needed job".to_owned())?;
        for needed in self.contexts.needs.values() {
            validate_outputs(&needed.outputs)?;
        }
        self.checkout.validate()?;
        self.cache.validate()
    }
}

/// `[A-Z_][A-Z0-9_]*`, at most [`crate::spec::MAX_NAME_BYTES`], with no [`crate::spec::RESERVED_NAME_PREFIXES`] entry:
/// the stored, upper-cased form of secret and variable names.
fn validate_secret_name(kind: &str, name: &str) -> Result<(), ValidationError> {
    let mut bytes = name.bytes();
    let well_formed = name.len() <= MAX_NAME_BYTES
        && bytes.next().is_some_and(|first| first.is_ascii_uppercase() || first == b'_')
        && bytes.all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
    ensure(well_formed, || format!("{kind} name `{name}` is malformed"))?;
    ensure(!RESERVED_NAME_PREFIXES.iter().any(|prefix| name.starts_with(prefix)), || format!("{kind} name `{name}` is reserved"))
}

impl CheckoutSource {
    pub fn validate(&self) -> Result<(), ValidationError> {
        ensure(self.url.starts_with("https://") || self.url.starts_with("http://"), || "the checkout URL must be HTTP(S)".to_owned())?;
        ensure(!self.url.contains('@'), || "the checkout URL must not carry credentials".to_owned())?;
        ensure(
            self.repository.split_once('/').is_some_and(|(owner, name)| !owner.is_empty() && !name.is_empty() && !name.contains('/')),
            || "the checkout repository must be `owner/name`".to_owned(),
        )?;
        ensure(matches!(self.sha.len(), 40 | 64) && self.sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), || {
            "the checkout SHA must be a full lowercase object id".to_owned()
        })?;
        ensure(self.reference.starts_with("refs/"), || "the checkout ref must be a full ref".to_owned())
    }
}

impl CacheScopes {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let valid = |scope: &String| {
            !scope.is_empty()
                && scope.len() <= 255
                && !scope.split('/').any(|part| part.is_empty() || part == "." || part == "..")
                && !scope.contains(['\\', '\0'])
        };
        ensure(valid(&self.write) && self.read.iter().all(valid), || "a cache scope is malformed".to_owned())
    }
}
