//! Protocol runners contracts and original validation.
use super::limits::{
    MAX_CAPABILITY_BYTES, MAX_CAPACITY, MAX_HEARTBEAT_INTERVAL, MAX_LONG_POLL, MAX_REGISTRATION_TOKEN_BYTES, MAX_RUNNER_NAME_CHARS,
};
use super::tokens::valid_runner_credential;
use super::validation::{ValidationError, ensure, unique, validate_labels};
use crate::secret::Secret;
use serde::{Deserialize, Serialize};

/// A runner, as the protocol names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunnerId(pub i64);

/// How a runner runs the jobs of a label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    /// Docker or Podman, through their CLI (§12.5.2).
    Container,
    /// Child processes of the runner's user, with no isolation (§12.5.3).
    Host,
}

impl Backend {
    /// Whether jobs on this backend are isolated from the runner's host. The labels of a
    /// backend that is not are the runner's host labels: the server stores them apart, shows
    /// them as running without isolation and never gives them an untrusted run's jobs.
    pub const fn isolates_jobs(self) -> bool {
        matches!(self, Self::Container)
    }
}

/// A `runs-on` label a runner serves, and the one backend its jobs run on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredLabel {
    pub name: String,
    pub backend: Backend,
}

/// What a runner can do, sent at registration and at every start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// The backends that work on this host (`rbit-runner check`).
    pub backends: Vec<Backend>,
    /// `std::env::consts::OS` (`linux`, `macos`, `windows`).
    pub os: String,
    /// `std::env::consts::ARCH` (`x86_64`, `aarch64`).
    pub arch: String,
    /// Tasks it runs at once.
    pub capacity: u32,
    /// The `rbit-runner` version.
    pub version: String,
    /// Runs one task, then unregisters.
    pub ephemeral: bool,
}

impl Capabilities {
    pub fn validate(&self) -> Result<(), ValidationError> {
        ensure(!self.backends.is_empty(), || "a runner must have at least one backend".to_owned())?;
        ensure(unique(self.backends.iter()), || "backends must not repeat".to_owned())?;
        for (field, value) in [("os", &self.os), ("arch", &self.arch), ("version", &self.version)] {
            ensure(
                !value.is_empty()
                    && value.len() <= MAX_CAPABILITY_BYTES
                    && value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'+')),
                || format!("`{field}` must be 1 to {MAX_CAPABILITY_BYTES} letters, digits, '.', '_', '-' or '+'"),
            )?;
        }
        ensure((1..=MAX_CAPACITY).contains(&self.capacity), || format!("`capacity` must be between 1 and {MAX_CAPACITY}"))
    }
}

/// Checks a runner's name, labels and capabilities together: each label is well formed,
/// named once, and runs on a backend the runner has.
fn validate_runner(name: &str, labels: &[DeclaredLabel], capabilities: &Capabilities) -> Result<(), ValidationError> {
    let chars = name.chars().count();
    ensure((1..=MAX_RUNNER_NAME_CHARS).contains(&chars) && !name.chars().any(char::is_control), || {
        format!("the runner name must be 1 to {MAX_RUNNER_NAME_CHARS} characters without control characters")
    })?;
    validate_labels(labels.iter().map(|label| label.name.as_str()))?;
    ensure(unique(labels.iter().map(|label| &label.name)), || "labels must not repeat".to_owned())?;
    capabilities.validate()?;
    match labels.iter().find(|label| !capabilities.backends.contains(&label.backend)) {
        Some(label) => Err(ValidationError::new(format!("label `{}` uses a backend the runner does not have", label.name))),
        None => Ok(()),
    }
}

/// The intervals a runner keeps to, chosen by the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intervals {
    /// How long the server holds a poll open; at most [`crate::protocol::MAX_LONG_POLL`].
    pub long_poll_seconds: u32,
    /// How often a running task reports its state; at most [`crate::protocol::MAX_HEARTBEAT_INTERVAL`].
    pub heartbeat_seconds: u32,
}

impl Intervals {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let long_poll = u64::from(self.long_poll_seconds);
        let heartbeat = u64::from(self.heartbeat_seconds);
        ensure((1..=MAX_LONG_POLL.as_secs()).contains(&long_poll), || {
            format!("`long_poll_seconds` must be between 1 and {}", MAX_LONG_POLL.as_secs())
        })?;
        ensure((1..=MAX_HEARTBEAT_INTERVAL.as_secs()).contains(&heartbeat), || {
            format!("`heartbeat_seconds` must be between 1 and {}", MAX_HEARTBEAT_INTERVAL.as_secs())
        })
    }
}

/// `POST register`: exchanges a registration token for a runner credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub token: Secret,
    pub name: String,
    pub labels: Vec<DeclaredLabel>,
    pub capabilities: Capabilities,
}

impl RegisterRequest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        ensure(!self.token.is_empty() && self.token.len() <= MAX_REGISTRATION_TOKEN_BYTES, || {
            format!("the registration token must be 1 to {MAX_REGISTRATION_TOKEN_BYTES} bytes")
        })?;
        validate_runner(&self.name, &self.labels, &self.capabilities)
    }
}

/// The answer to [`crate::protocol::RegisterRequest`]. The credential is shown this once; the server keeps
/// only its digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub runner_id: RunnerId,
    pub credential: Secret,
    pub intervals: Intervals,
}

impl RegisterResponse {
    pub fn validate(&self) -> Result<(), ValidationError> {
        ensure(valid_runner_credential(self.credential.expose()), || "the runner credential is malformed".to_owned())?;
        self.intervals.validate()
    }
}

/// `POST declare`: the runner's labels and capabilities, sent at every start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclareRequest {
    pub name: String,
    pub labels: Vec<DeclaredLabel>,
    pub capabilities: Capabilities,
}

impl DeclareRequest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_runner(&self.name, &self.labels, &self.capabilities)
    }
}

/// The answer to [`crate::protocol::DeclareRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclareResponse {
    pub runner_id: RunnerId,
    pub intervals: Intervals,
    // Actions: shared cache.
    /// The server keeps `actions/cache` entries for all runners ([`crate::protocol::CACHES_PATH`]); without
    /// it (an older server) a runner keeps its own.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shared_cache: bool,
}

/// `POST unregister`: an ephemeral runner after its task, or an uninstall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UnregisterRequest {}
