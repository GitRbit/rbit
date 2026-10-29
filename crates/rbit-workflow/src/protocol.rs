//! Runner protocol v1 (§12.6 of `docs/design/actions.md`): the request and response bodies
//! `rbit-runner` and the server exchange under [`BASE_PATH`], the headers, the version
//! constants, the body caps and the error model. Types only — the server's
//! `actions_runner` route and the runner's client do the talking, and both validate what
//! they receive with the `validate` methods here.
//!
//! Compatibility within v1 follows one rule: **information may be ignored, instructions
//! may not.**
//!
//! * Request and response envelopes ignore fields they do not know, so either side can add
//!   an optional field without a new protocol version (a newer runner registering with an
//!   older server, say).
//! * What a runner is told to run — [`crate::spec::JobSpec`] and everything in it — denies
//!   unknown fields and carries its own version: a runner that skipped a field it did not
//!   understand would run a different job than the workflow describes.
//! * Enumerations a decision rests on ([`Backend`], [`StepStatus`], [`TaskResult`]) are
//!   closed. Error codes are open ([`ErrorCode::Unknown`]), because a runner only needs to
//!   know that a call failed and how to back off.
//!
//! Every call carries [`PROTOCOL_HEADER`]`: 1` and a `User-Agent` of [`USER_AGENT_PREFIX`]
//! and the runner's version; runner calls authenticate with `Authorization: Bearer` and a
//! runner credential ([`valid_runner_credential`]). Times are Unix seconds.

use std::{collections::BTreeMap, fmt, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{secret::Secret, spec::JobSpec};

/// The protocol this build speaks.
pub const PROTOCOL_VERSION: u32 = 1;
/// The versions a server built from this crate accepts; listed in an
/// [`ErrorCode::UnsupportedProtocol`] answer.
pub const SUPPORTED_PROTOCOL_VERSIONS: [u32; 1] = [PROTOCOL_VERSION];

/// Where every endpoint lives; [`Endpoint::path`] is relative to it.
pub const BASE_PATH: &str = "/api/actions/runner/v1";
/// The header carrying the protocol version of a call.
pub const PROTOCOL_HEADER: &str = "Rbit-Runner-Protocol";
/// The header making a poll retry-safe (§12.6 "Idempotency-Key").
pub const IDEMPOTENCY_KEY_HEADER: &str = "Idempotency-Key";
/// The header carrying the SHA-256 of an uploaded artifact, as lowercase hex.
pub const CONTENT_SHA256_HEADER: &str = "Rbit-Content-Sha256";
/// The runner's `User-Agent` is this followed by its version.
pub const USER_AGENT_PREFIX: &str = "rbit-runner/";

/// The prefix of a runner credential (`rbrn_` and 43 base64url characters).
pub const RUNNER_CREDENTIAL_PREFIX: &str = "rbrn_";
/// The prefix of a task token (`rbat_` and 43 base64url characters); the same value as
/// `rbit_core::actions::TASK_TOKEN_PREFIX`, which this crate does not depend on.
pub const TASK_TOKEN_PREFIX: &str = "rbat_";
/// Base64url characters after either prefix: 32 random bytes.
pub const TOKEN_BODY_CHARS: usize = 43;

/// The longest a poll is held open before it answers `204`.
pub const MAX_LONG_POLL: Duration = Duration::from_secs(25);
/// The longest a runner may wait between two state calls of a running task.
pub const MAX_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
/// Most decompressed bytes per compressed byte of a gzip body.
pub const MAX_GZIP_RATIO: u64 = 100;

/// Longest runner name, in characters.
pub const MAX_RUNNER_NAME_CHARS: usize = 64;
/// Most labels one runner may declare (`rbit_core::actions::MAX_RUNNER_LABELS`).
pub const MAX_RUNNER_LABELS: usize = 64;
/// Longest label, in characters (`rbit_core::actions::MAX_LABEL_CHARS`).
pub const MAX_LABEL_CHARS: usize = 64;
/// Most tasks one runner may run at once.
pub const MAX_CAPACITY: u32 = 64;
/// Longest registration token accepted, in bytes.
pub const MAX_REGISTRATION_TOKEN_BYTES: usize = 256;
/// Longest `os`, `arch` or `version` a runner declares, in bytes.
pub const MAX_CAPABILITY_BYTES: usize = 64;
/// Longest [`IDEMPOTENCY_KEY_HEADER`] value, in bytes.
pub const MAX_IDEMPOTENCY_KEY_BYTES: usize = 64;
/// Longest log line; a runner splits longer lines before sending them.
pub const MAX_LOG_LINE_BYTES: usize = 64 * 1024;
/// Longest job output name, in bytes.
pub const MAX_OUTPUT_KEY_BYTES: usize = 255;
/// Most bytes of job outputs (names and values) per job.
pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
// Actions: step summaries.
/// Most bytes of one step's summary (`GITHUB_STEP_SUMMARY`) in a state call. The runner
/// counts the text as JSON encodes it and cuts a longer summary with a note.
pub const MAX_STEP_SUMMARY_BYTES: usize = 64 * 1024;
/// Most bytes of all of a task's step summaries together, counted the same way.
pub const MAX_TASK_SUMMARY_BYTES: usize = 256 * 1024;

const KIB: usize = 1024;
const MIB: usize = 1024 * KIB;

/// The endpoints of protocol v1, apart from artifacts (see [`ARTIFACTS_PATH`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Endpoint {
    /// `POST register`: [`RegisterRequest`] → [`RegisterResponse`].
    Register,
    /// `POST declare`: [`DeclareRequest`] → [`DeclareResponse`].
    Declare,
    /// `POST poll`: [`PollRequest`] → [`PollResponse`], or `204` when the poll ends empty.
    Poll,
    /// `POST tasks/{id}/state`: [`TaskStateRequest`] → [`TaskStateResponse`].
    TaskState,
    /// `POST tasks/{id}/logs?from=<line>` ([`LogsQuery`]): `text/plain` lines →
    /// [`LogsResponse`], or `409` with [`ErrorBody::next_line`] on a gap.
    TaskLogs,
    /// `POST unregister`: [`UnregisterRequest`] → `204`.
    Unregister,
}

impl Endpoint {
    pub const ALL: [Self; 6] = [Self::Register, Self::Declare, Self::Poll, Self::TaskState, Self::TaskLogs, Self::Unregister];

    /// The route below [`BASE_PATH`], with `{id}` for the task id.
    pub const fn path(self) -> &'static str {
        match self {
            Self::Register => "register",
            Self::Declare => "declare",
            Self::Poll => "poll",
            Self::TaskState => "tasks/{id}/state",
            Self::TaskLogs => "tasks/{id}/logs",
            Self::Unregister => "unregister",
        }
    }

    /// The largest request body, after decompression.
    pub const fn body_cap(self) -> usize {
        match self {
            Self::Register | Self::Declare => 64 * KIB,
            Self::Poll => 4 * KIB,
            // Outputs (up to `MAX_OUTPUT_BYTES` before JSON escapes them), step states and
            // summaries (`MAX_TASK_SUMMARY_BYTES`), with room to spare: a final call that is
            // too large could never report how its task ended.
            Self::TaskState => 4 * MIB,
            Self::TaskLogs => 8 * MIB,
            Self::Unregister => KIB,
        }
    }
}

/// A runner, as the protocol names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunnerId(pub i64);

/// A task: one attempt of one job on one runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(pub i64);

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Why a message was refused. The message names the field and is safe to return to the
/// other side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub message: String,
}

impl ValidationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ValidationError {}

/// `Err` with `message` unless `condition` holds.
pub(crate) fn ensure(condition: bool, message: impl FnOnce() -> String) -> Result<(), ValidationError> {
    if condition { Ok(()) } else { Err(ValidationError::new(message())) }
}

/// How a runner runs the jobs of a label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    /// Docker or Podman, through their CLI (§12.5.2).
    Container,
    /// Child processes of the runner's user, with no isolation (§12.5.3).
    Host,
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

/// `runs-on` labels: 1 to [`MAX_RUNNER_LABELS`] of 1 to [`MAX_LABEL_CHARS`] `[A-Za-z0-9._-]`.
pub(crate) fn validate_labels<'a>(labels: impl ExactSizeIterator<Item = &'a str>) -> Result<(), ValidationError> {
    ensure((1..=MAX_RUNNER_LABELS).contains(&labels.len()), || format!("there must be 1 to {MAX_RUNNER_LABELS} labels"))?;
    for label in labels {
        ensure(
            !label.is_empty()
                && label.len() <= MAX_LABEL_CHARS
                && label.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')),
            || format!("label `{label}` must be 1 to {MAX_LABEL_CHARS} letters, digits, '.', '_' or '-'"),
        )?;
    }
    Ok(())
}

pub(crate) fn unique<T: Ord>(items: impl Iterator<Item = T>) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    items.into_iter().all(|item| seen.insert(item))
}

/// Whether `value` has the shape of a runner credential.
pub fn valid_runner_credential(value: &str) -> bool {
    valid_token(value, RUNNER_CREDENTIAL_PREFIX)
}

/// Whether `value` has the shape of a task token.
pub fn valid_task_token(value: &str) -> bool {
    valid_token(value, TASK_TOKEN_PREFIX)
}

fn valid_token(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|body| body.len() == TOKEN_BODY_CHARS && body.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
}

/// Whether `value` is a usable [`IDEMPOTENCY_KEY_HEADER`]: 1 to
/// [`MAX_IDEMPOTENCY_KEY_BYTES`] of `[A-Za-z0-9_-]` (a UUID fits).
pub fn valid_idempotency_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_IDEMPOTENCY_KEY_BYTES
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The intervals a runner keeps to, chosen by the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intervals {
    /// How long the server holds a poll open; at most [`MAX_LONG_POLL`].
    pub long_poll_seconds: u32,
    /// How often a running task reports its state; at most [`MAX_HEARTBEAT_INTERVAL`].
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

/// The answer to [`RegisterRequest`]. The credential is shown this once; the server keeps
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

/// The answer to [`DeclareRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclareResponse {
    pub runner_id: RunnerId,
    pub intervals: Intervals,
    // Actions: shared cache.
    /// The server keeps `actions/cache` entries for all runners ([`CACHES_PATH`]); without
    /// it (an older server) a runner keeps its own.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shared_cache: bool,
}

/// `POST poll`, sent with an [`IDEMPOTENCY_KEY_HEADER`] and only while a slot is free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollRequest {
    pub free_slots: u32,
}

impl PollRequest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        ensure((1..=MAX_CAPACITY).contains(&self.free_slots), || format!("`free_slots` must be between 1 and {MAX_CAPACITY}"))
    }
}

/// A claimed task. A poll that ends without one answers `204` and no body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollResponse {
    pub task: JobSpec,
}

/// Where a step is in its life cycle; the wire names of `rbit_core::actions::Status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Waiting,
    Running,
    Success,
    Failure,
    Cancelled,
    Skipped,
}

impl StepStatus {
    pub const fn is_done(self) -> bool {
        matches!(self, Self::Success | Self::Failure | Self::Cancelled | Self::Skipped)
    }
}

/// One step of a task as the runner last saw it. Its log is the task log's lines
/// `log_index .. log_index + log_length`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepState {
    /// The step's position in [`crate::spec::JobTemplate::steps`], from 0.
    pub index: u32,
    pub status: StepStatus,
    pub started_at: Option<i64>,
    pub stopped_at: Option<i64>,
    pub log_index: u64,
    pub log_length: u64,
}

/// How a task ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskResult {
    Success,
    Failure,
    Cancelled,
}

/// `POST tasks/{id}/state`: the heartbeat, step states and outputs, and the result once the
/// task has ended. Steps are keyed by index and outputs are kept when first seen, so a
/// resent call changes nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStateRequest {
    pub steps: Vec<StepState>,
    /// Job outputs evaluated so far, masked by the runner.
    pub outputs: BTreeMap<String, String>,
    /// Set on the final call only.
    pub result: Option<TaskResult>,
    // Actions: step summaries.
    /// What steps wrote to `GITHUB_STEP_SUMMARY` (Markdown, masked by the runner), by step
    /// index, once each step has ended. The first summary seen for a step is kept, so a
    /// resent call changes nothing. Left out when empty, as older runners send it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub summaries: BTreeMap<u32, String>,
    // Actions: third-party actions.
    /// Steps the runner added after the job's own, by index: the `post` scripts of the
    /// actions it ran (`Post Run actions/setup-node@v4`). The server creates them once; an
    /// index the template already has keeps its name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub post_steps: BTreeMap<u32, String>,
    // Actions: environments.
    /// The job's `environment.url`, as the runner evaluated it after the steps (masked).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_url: Option<String>,
}

/// Longest environment URL a runner may report.
pub const MAX_ENVIRONMENT_URL_BYTES: usize = 2048;

/// Longest name of a step the runner adds.
pub const MAX_POST_STEP_NAME_BYTES: usize = 256;

impl TaskStateRequest {
    pub fn is_final(&self) -> bool {
        self.result.is_some()
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        // Every step of the template may add a post step.
        let max_steps = 2 * crate::spec::MAX_STEPS;
        ensure(self.steps.len() <= max_steps, || format!("a task has at most {max_steps} steps"))?;
        for (index, name) in &self.post_steps {
            ensure(usize::try_from(*index).is_ok_and(|index| index < max_steps), || format!("post step {index} is out of range"))?;
            ensure(!name.is_empty() && name.len() <= MAX_POST_STEP_NAME_BYTES && !name.chars().any(char::is_control), || {
                format!("post step {index} needs a name of 1 to {MAX_POST_STEP_NAME_BYTES} bytes")
            })?;
        }
        ensure(unique(self.steps.iter().map(|step| step.index)), || "step indexes must not repeat".to_owned())?;
        for step in &self.steps {
            let index = step.index;
            ensure(usize::try_from(index).is_ok_and(|index| index < max_steps), || format!("step {index} is out of range"))?;
            ensure(step.log_index.checked_add(step.log_length).is_some(), || format!("step {index} has an impossible log range"))?;
            if let (Some(started), Some(stopped)) = (step.started_at, step.stopped_at) {
                ensure(stopped >= started, || format!("step {index} stopped before it started"))?;
            }
            ensure(!self.is_final() || step.status != StepStatus::Running, || format!("step {index} is still running"))?;
        }
        // Actions: step summaries.
        let mut total = 0usize;
        for (index, summary) in &self.summaries {
            ensure(usize::try_from(*index).is_ok_and(|index| index < max_steps), || {
                format!("the summary of step {index} is out of range")
            })?;
            ensure(summary.len() <= MAX_STEP_SUMMARY_BYTES, || {
                format!("the summary of step {index} is over {MAX_STEP_SUMMARY_BYTES} bytes")
            })?;
            total = total.saturating_add(summary.len());
        }
        ensure(total <= MAX_TASK_SUMMARY_BYTES, || format!("a task's step summaries may be at most {MAX_TASK_SUMMARY_BYTES} bytes"))?;
        if let Some(url) = &self.environment_url {
            ensure(url.len() <= MAX_ENVIRONMENT_URL_BYTES && !url.chars().any(char::is_control), || {
                format!("the environment URL must be at most {MAX_ENVIRONMENT_URL_BYTES} bytes, on one line")
            })?;
        }
        validate_outputs(&self.outputs)
    }
}

/// Output names are `[A-Za-z_][A-Za-z0-9_-]*` of at most [`MAX_OUTPUT_KEY_BYTES`]; names and
/// values together are at most [`MAX_OUTPUT_BYTES`].
pub(crate) fn validate_outputs<V: AsRef<str>>(outputs: &BTreeMap<String, V>) -> Result<(), ValidationError> {
    let mut total = 0usize;
    for (name, value) in outputs {
        ensure(name.len() <= MAX_OUTPUT_KEY_BYTES && valid_identifier(name), || {
            format!("output name `{name}` must start with a letter or '_' and contain only letters, digits, '_' and '-'")
        })?;
        total = total.saturating_add(name.len()).saturating_add(value.as_ref().len());
    }
    ensure(total <= MAX_OUTPUT_BYTES, || format!("outputs may be at most {MAX_OUTPUT_BYTES} bytes"))
}

/// `[A-Za-z_][A-Za-z0-9_-]*`, the shape of output names.
fn valid_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The answer to every state call: whether the job was cancelled. Cancellation reaches a
/// runner this way; by then its task token is already dead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStateResponse {
    pub cancel: bool,
}

/// The query of `POST tasks/{id}/logs`: the task-log line the body's first line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogsQuery {
    pub from: u64,
}

/// The answer to a log append: the server's line count after it, which is where the next
/// append starts. Lines before it that were sent again are dropped, so a resend is harmless.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogsResponse {
    pub next_line: u64,
}

/// `POST unregister`: an ephemeral runner after its task, or an uninstall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UnregisterRequest {}

/// The `code` of an [`ErrorBody`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The body is not a valid message ([`ValidationError`], bad JSON, a bad query).
    BadRequest,
    /// No or an unknown credential or token.
    Unauthenticated,
    /// Authenticated, but not allowed (another runner's task, a pinned label).
    Forbidden,
    /// The task or runner is gone; a runner cancels the task locally.
    NotFound,
    /// The call contradicts the stored state (a log gap: see [`ErrorBody::next_line`]).
    Conflict,
    /// The body exceeds its cap or the gzip ratio.
    TooLarge,
    /// The protocol version is not served; see [`ErrorBody::supported_versions`].
    UnsupportedProtocol,
    /// Too many calls; the `Retry-After` header says when to try again.
    Throttled,
    /// The server cannot take the call now (too many polls, shutting down).
    Unavailable,
    /// A code from a newer server. Treated like [`ErrorCode::Unavailable`].
    #[serde(other)]
    Unknown,
}

impl ErrorCode {
    /// The HTTP status the code is sent with.
    pub const fn status(self) -> u16 {
        match self {
            Self::BadRequest => 400,
            Self::Unauthenticated => 401,
            Self::Forbidden => 403,
            Self::NotFound => 404,
            Self::Conflict => 409,
            Self::TooLarge => 413,
            Self::UnsupportedProtocol => 426,
            Self::Throttled => 429,
            Self::Unavailable | Self::Unknown => 503,
        }
    }
}

// Actions: artifacts (A).
/// Where a task's artifacts are, below [`BASE_PATH`] (`{id}` is the task id):
/// `PUT …/{name}` uploads one (a tar.gz, with [`CONTENT_SHA256_HEADER`] and
/// [`ArtifactQuery`]), `GET` lists the run's ([`ArtifactList`]), `GET …/{name}` downloads one.
/// The upload's cap is the server's `[actions] max_artifact_bytes`, not a protocol constant.
pub const ARTIFACTS_PATH: &str = "tasks/{id}/artifacts";

/// The shape of an artifact name: 1 to 128 of `[A-Za-z0-9._-]`, but not `.` or `..`
/// (§12.7). A name is a path segment of [`ARTIFACTS_PATH`] and the directory a runner
/// unpacks it into among several, so it must not walk up a directory.
pub fn valid_artifact_name(name: &str) -> bool {
    (1..=128).contains(&name.len())
        && name != "."
        && name != ".."
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// The query of an artifact upload.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactQuery {
    /// `retention-days`; it can only lower the server's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<u32>,
    /// Replace an artifact of the same name in this run attempt.
    #[serde(default)]
    pub overwrite: bool,
}

// Actions: workflow_run.
/// The query of an artifact list or download: another run of the task's repository
/// (`actions/download-artifact`'s `run-id`) instead of the task's own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRunQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<i64>,
}

// Actions: shared cache.
/// Where a task's cache entries are, below [`BASE_PATH`] (`{id}` is the task id), when the
/// server declares [`DeclareResponse::shared_cache`]: `POST …/lookup` finds the entry to
/// restore ([`CacheLookupRequest`], answered with [`CacheLookupResponse`]), `GET …/{entry}`
/// downloads it (a tar.gz, with [`CONTENT_SHA256_HEADER`]), and `PUT` saves one (with
/// [`CONTENT_SHA256_HEADER`] and [`CacheSaveQuery`]; `201` and its [`CacheEntryInfo`], or
/// `409` when the key is saved already). The server picks the scopes from the task's run.
pub const CACHES_PATH: &str = "tasks/{id}/caches";

/// `POST tasks/{id}/caches/lookup`: the key, the `restore-keys` prefixes in order, and the
/// version (64 lowercase hex digits, from the cached paths and the runner's platform).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheLookupRequest {
    pub key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restore_keys: Vec<String>,
    pub version: String,
}

/// The answer to [`CacheLookupRequest`]: the entry to restore, if any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheLookupResponse {
    pub entry: Option<CacheEntryInfo>,
}

/// The query of a cache save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheSaveQuery {
    pub key: String,
    pub version: String,
    /// Replace an entry of the key that the job could not restore.
    #[serde(default)]
    pub replace: bool,
}

/// A cache entry, as lookups and saves answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheEntryInfo {
    pub id: i64,
    pub key: String,
    /// The scope it was saved in (`heads/main`).
    pub scope: String,
    /// Bytes of the tar.gz.
    pub size: u64,
    /// Its SHA-256, as 64 lowercase hex digits.
    pub sha256: String,
}

/// One artifact of a run, as uploads answer and lists show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactInfo {
    pub name: String,
    /// Bytes of the tar.gz.
    pub size: u64,
    /// Its SHA-256, as 64 lowercase hex digits.
    pub sha256: String,
}

/// The answer to `GET tasks/{id}/artifacts`: the newest attempt's artifact of each name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactList {
    pub artifacts: Vec<ArtifactInfo>,
}

/// The body of every error answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    /// With [`ErrorCode::UnsupportedProtocol`]: the versions the server speaks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_versions: Vec<u32>,
    /// With [`ErrorCode::Conflict`] on a log append: the line to resend from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_line: Option<u64>,
}

impl ErrorBody {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), supported_versions: Vec::new(), next_line: None }
    }

    pub fn unsupported_protocol() -> Self {
        Self {
            supported_versions: SUPPORTED_PROTOCOL_VERSIONS.to_vec(),
            ..Self::new(ErrorCode::UnsupportedProtocol, "this server does not speak the runner's protocol version")
        }
    }

    pub fn log_gap(next_line: u64) -> Self {
        Self { next_line: Some(next_line), ..Self::new(ErrorCode::Conflict, format!("the log continues at line {next_line}")) }
    }
}

impl From<ValidationError> for ErrorBody {
    fn from(error: ValidationError) -> Self {
        Self::new(ErrorCode::BadRequest, error.message)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use serde::{Serialize, de::DeserializeOwned};

    use super::*;

    /// A change that makes a message invalid, and part of the message it is refused with.
    pub(crate) type Refusal<T> = (fn(&mut T), &'static str);

    /// Serialises to exactly `expected`, and `expected` deserialises back to the value.
    #[track_caller]
    pub(crate) fn golden<T: Serialize + DeserializeOwned + PartialEq + fmt::Debug>(value: &T, expected: &str) {
        assert_eq!(serde_json::to_string(value).unwrap(), expected);
        assert_eq!(&serde_json::from_str::<T>(expected).unwrap(), value);
    }

    fn capabilities() -> Capabilities {
        Capabilities {
            backends: vec![Backend::Container, Backend::Host],
            os: "linux".into(),
            arch: "x86_64".into(),
            capacity: 2,
            version: "0.2.0".into(),
            ephemeral: false,
        }
    }

    fn labels() -> Vec<DeclaredLabel> {
        vec![
            DeclaredLabel { name: "ubuntu-latest".into(), backend: Backend::Container },
            DeclaredLabel { name: "self-hosted".into(), backend: Backend::Host },
        ]
    }

    const CAPABILITIES: &str =
        r#"{"backends":["container","host"],"os":"linux","arch":"x86_64","capacity":2,"version":"0.2.0","ephemeral":false}"#;
    const LABELS: &str = r#"[{"name":"ubuntu-latest","backend":"container"},{"name":"self-hosted","backend":"host"}]"#;
    const CREDENTIAL: &str = "rbrn_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    #[test]
    fn golden_registration() {
        let request = RegisterRequest {
            token: Secret::new("registration-token"),
            name: "build-1".into(),
            labels: labels(),
            capabilities: capabilities(),
        };
        request.validate().unwrap();
        golden(&request, &format!(r#"{{"token":"registration-token","name":"build-1","labels":{LABELS},"capabilities":{CAPABILITIES}}}"#));

        let response = RegisterResponse {
            runner_id: RunnerId(7),
            credential: Secret::new(CREDENTIAL),
            intervals: Intervals { long_poll_seconds: 25, heartbeat_seconds: 10 },
        };
        response.validate().unwrap();
        golden(
            &response,
            &format!(r#"{{"runner_id":7,"credential":"{CREDENTIAL}","intervals":{{"long_poll_seconds":25,"heartbeat_seconds":10}}}}"#),
        );

        let declare = DeclareRequest { name: "build-1".into(), labels: labels(), capabilities: capabilities() };
        declare.validate().unwrap();
        golden(&declare, &format!(r#"{{"name":"build-1","labels":{LABELS},"capabilities":{CAPABILITIES}}}"#));
        golden(
            &DeclareResponse {
                runner_id: RunnerId(7),
                intervals: Intervals { long_poll_seconds: 25, heartbeat_seconds: 10 },
                shared_cache: false,
            },
            r#"{"runner_id":7,"intervals":{"long_poll_seconds":25,"heartbeat_seconds":10}}"#,
        );
        // Actions: shared cache. Older runners read the answer without it.
        golden(
            &DeclareResponse {
                runner_id: RunnerId(7),
                intervals: Intervals { long_poll_seconds: 25, heartbeat_seconds: 10 },
                shared_cache: true,
            },
            r#"{"runner_id":7,"intervals":{"long_poll_seconds":25,"heartbeat_seconds":10},"shared_cache":true}"#,
        );
        golden(&UnregisterRequest {}, "{}");
    }

    #[test]
    fn golden_poll_state_and_logs() {
        let poll = PollRequest { free_slots: 1 };
        poll.validate().unwrap();
        golden(&poll, r#"{"free_slots":1}"#);

        let state = TaskStateRequest {
            steps: vec![
                StepState {
                    index: 0,
                    status: StepStatus::Success,
                    started_at: Some(100),
                    stopped_at: Some(102),
                    log_index: 0,
                    log_length: 12,
                },
                StepState { index: 1, status: StepStatus::Running, started_at: Some(102), stopped_at: None, log_index: 12, log_length: 3 },
            ],
            outputs: BTreeMap::from([("version".to_owned(), "1.2.3".to_owned())]),
            result: None,
            summaries: BTreeMap::new(),
            post_steps: BTreeMap::new(),
            environment_url: None,
        };
        state.validate().unwrap();
        assert!(!state.is_final());
        golden(
            &state,
            concat!(
                r#"{"steps":[{"index":0,"status":"success","started_at":100,"stopped_at":102,"log_index":0,"log_length":12},"#,
                r#"{"index":1,"status":"running","started_at":102,"stopped_at":null,"log_index":12,"log_length":3}],"#,
                r#""outputs":{"version":"1.2.3"},"result":null}"#
            ),
        );
        let done = TaskStateRequest {
            steps: Vec::new(),
            outputs: BTreeMap::new(),
            result: Some(TaskResult::Cancelled),
            summaries: BTreeMap::new(),
            post_steps: BTreeMap::new(),
            environment_url: None,
        };
        assert!(done.is_final());
        golden(&done, r#"{"steps":[],"outputs":{},"result":"cancelled"}"#);
        // Actions: step summaries. Older servers read the call the same way; newer ones keep them.
        let summarised = TaskStateRequest { summaries: BTreeMap::from([(0, "# Done\n".to_owned())]), ..done };
        golden(&summarised, r##"{"steps":[],"outputs":{},"result":"cancelled","summaries":{"0":"# Done\n"}}"##);
        let old: TaskStateRequest = serde_json::from_str(r#"{"steps":[],"outputs":{},"result":null}"#).unwrap();
        assert!(old.summaries.is_empty());
        golden(&TaskStateResponse { cancel: true }, r#"{"cancel":true}"#);
        golden(&LogsQuery { from: 42 }, r#"{"from":42}"#);
        golden(&LogsResponse { next_line: 57 }, r#"{"next_line":57}"#);
    }

    #[test]
    fn golden_errors() {
        golden(&ErrorBody::new(ErrorCode::Forbidden, "not your task"), r#"{"code":"forbidden","message":"not your task"}"#);
        golden(
            &ErrorBody::unsupported_protocol(),
            r#"{"code":"unsupported_protocol","message":"this server does not speak the runner's protocol version","supported_versions":[1]}"#,
        );
        golden(&ErrorBody::log_gap(9), r#"{"code":"conflict","message":"the log continues at line 9","next_line":9}"#);
        let invalid: ErrorBody = ValidationError::new("bad").into();
        assert_eq!((invalid.code, invalid.message.as_str()), (ErrorCode::BadRequest, "bad"));
        let codes = [
            ErrorCode::BadRequest,
            ErrorCode::Unauthenticated,
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Conflict,
            ErrorCode::TooLarge,
            ErrorCode::UnsupportedProtocol,
            ErrorCode::Throttled,
            ErrorCode::Unavailable,
        ];
        let names = codes.map(|code| serde_json::to_string(&code).unwrap());
        assert_eq!(
            names,
            [
                r#""bad_request""#,
                r#""unauthenticated""#,
                r#""forbidden""#,
                r#""not_found""#,
                r#""conflict""#,
                r#""too_large""#,
                r#""unsupported_protocol""#,
                r#""throttled""#,
                r#""unavailable""#
            ]
        );
        assert_eq!(codes.map(ErrorCode::status), [400, 401, 403, 404, 409, 413, 426, 429, 503]);
    }

    #[test]
    fn envelopes_are_forward_compatible_and_enums_closed() {
        // A newer server or runner may add fields within v1 …
        let error: ErrorBody = serde_json::from_str(r#"{"code":"quota_exceeded","message":"m","hint":"later"}"#).unwrap();
        assert_eq!((error.code, error.code.status()), (ErrorCode::Unknown, 503));
        let state: TaskStateResponse = serde_json::from_str(r#"{"cancel":false,"reason":"x"}"#).unwrap();
        assert!(!state.cancel);
        let caps: Capabilities = serde_json::from_str(&CAPABILITIES.replace('}', r#","gpus":2}"#)).unwrap();
        assert_eq!(caps, capabilities());
        // … but an unknown backend, status or result is refused rather than guessed.
        assert!(serde_json::from_str::<Backend>(r#""vm""#).is_err());
        assert!(serde_json::from_str::<StepStatus>(r#""paused""#).is_err());
        assert!(serde_json::from_str::<TaskResult>(r#""skipped""#).is_err());
    }

    #[test]
    fn runner_validation() {
        let request = |change: fn(&mut RegisterRequest)| {
            let mut request =
                RegisterRequest { token: Secret::new("t"), name: "build-1".into(), labels: labels(), capabilities: capabilities() };
            change(&mut request);
            request.validate().map_err(|error| error.message)
        };
        assert!(request(|_| ()).is_ok());
        let refused: [Refusal<RegisterRequest>; 12] = [
            (|r| r.token = Secret::default(), "registration token"),
            (|r| r.token = Secret::new("x".repeat(257)), "registration token"),
            (|r| r.name = String::new(), "runner name"),
            (|r| r.name = "a\nb".into(), "runner name"),
            (|r| r.labels.clear(), "1 to 64 labels"),
            (|r| r.labels[0].name = "has space".into(), "label `has space`"),
            (|r| r.labels[1].name = "ubuntu-latest".into(), "must not repeat"),
            (|r| r.capabilities.backends = vec![Backend::Container], "label `self-hosted` uses a backend"),
            (|r| r.capabilities.backends = vec![Backend::Host, Backend::Host], "backends must not repeat"),
            (|r| r.capabilities.capacity = 0, "capacity"),
            (|r| r.capabilities.capacity = MAX_CAPACITY + 1, "capacity"),
            (|r| r.capabilities.os = "linux; rm".into(), "`os`"),
        ];
        for (change, message) in refused {
            let error = request(change).unwrap_err();
            assert!(error.contains(message), "{error:?} should mention {message:?}");
        }
        assert!(
            request(|r| r.labels = (0..65).map(|i| DeclaredLabel { name: format!("l{i}"), backend: Backend::Host }).collect()).is_err()
        );
    }

    #[test]
    fn token_and_key_shapes() {
        assert!(valid_runner_credential(CREDENTIAL));
        assert!(valid_task_token(&format!("rbat_{}", "a-_9".repeat(10) + "xyz")));
        assert!(!valid_task_token(CREDENTIAL));
        assert!(!valid_runner_credential(&CREDENTIAL[..CREDENTIAL.len() - 1]));
        assert!(!valid_runner_credential(&CREDENTIAL.replace('A', "+")));
        assert!(valid_idempotency_key("0b1c2d3e-0000-4000-8000-000000000000"));
        assert!(!valid_idempotency_key(""));
        assert!(!valid_idempotency_key(&"k".repeat(MAX_IDEMPOTENCY_KEY_BYTES + 1)));
        assert!(!valid_idempotency_key("a b"));
        let response = RegisterResponse {
            runner_id: RunnerId(1),
            credential: Secret::new("rbrn_short"),
            intervals: Intervals { long_poll_seconds: 25, heartbeat_seconds: 10 },
        };
        assert!(response.validate().is_err());
    }

    #[test]
    fn interval_poll_and_state_caps() {
        assert!(Intervals { long_poll_seconds: 26, heartbeat_seconds: 10 }.validate().is_err());
        assert!(Intervals { long_poll_seconds: 25, heartbeat_seconds: 11 }.validate().is_err());
        assert!(Intervals { long_poll_seconds: 0, heartbeat_seconds: 1 }.validate().is_err());
        assert!(PollRequest { free_slots: 0 }.validate().is_err());

        let step = StepState { index: 0, status: StepStatus::Running, started_at: Some(5), stopped_at: None, log_index: 0, log_length: 0 };
        let state = |steps: Vec<StepState>, outputs: BTreeMap<String, String>, result| {
            TaskStateRequest { steps, outputs, result, summaries: BTreeMap::new(), post_steps: BTreeMap::new(), environment_url: None }
                .validate()
        };
        assert!(state(vec![step], BTreeMap::new(), None).is_ok());
        assert!(state(vec![step], BTreeMap::new(), Some(TaskResult::Failure)).is_err(), "a final state with a running step");
        assert!(state(vec![step, step], BTreeMap::new(), None).is_err(), "a repeated index");
        assert!(state(vec![StepState { index: 511, ..step }], BTreeMap::new(), None).is_ok(), "post steps follow the template's");
        assert!(state(vec![StepState { index: 512, ..step }], BTreeMap::new(), None).is_err());
        assert!(state(vec![StepState { stopped_at: Some(4), ..step }], BTreeMap::new(), None).is_err());
        assert!(state(vec![StepState { log_index: u64::MAX, log_length: 1, ..step }], BTreeMap::new(), None).is_err());
        let steps = (0..513).map(|index| StepState { index, ..step }).collect();
        assert!(state(steps, BTreeMap::new(), None).is_err());

        let outputs = |name: &str, bytes: usize| BTreeMap::from([(name.to_owned(), "x".repeat(bytes))]);
        assert!(state(Vec::new(), outputs("ok_name-1", MAX_OUTPUT_BYTES - 9), None).is_ok());
        assert!(state(Vec::new(), outputs("ok_name-1", MAX_OUTPUT_BYTES - 8), None).is_err());
        assert!(state(Vec::new(), outputs("1st", 1), None).is_err());
        assert!(state(Vec::new(), outputs(&"k".repeat(MAX_OUTPUT_KEY_BYTES + 1), 1), None).is_err());

        // Actions: step summaries.
        let summarised = |summaries: &[(u32, usize)]| TaskStateRequest {
            steps: Vec::new(),
            outputs: BTreeMap::new(),
            result: Some(TaskResult::Success),
            summaries: summaries.iter().map(|(index, bytes)| (*index, "x".repeat(*bytes))).collect(),
            post_steps: BTreeMap::new(),
            environment_url: None,
        };
        assert!(summarised(&[(0, MAX_STEP_SUMMARY_BYTES)]).validate().is_ok());
        assert!(summarised(&[(0, MAX_STEP_SUMMARY_BYTES + 1)]).validate().is_err());
        assert!(summarised(&[(512, 1)]).validate().is_err(), "past the last step");

        // Actions: third-party actions.
        let posted =
            |index: u32, name: &str| TaskStateRequest { post_steps: BTreeMap::from([(index, name.to_owned())]), ..summarised(&[]) };
        assert!(posted(3, "Post Run actions/setup-node@v4").validate().is_ok());
        golden(&posted(3, "Post x"), r#"{"steps":[],"outputs":{},"result":"success","post_steps":{"3":"Post x"}}"#);
        assert!(posted(512, "Post x").validate().is_err());
        assert!(posted(3, "").validate().is_err());
        assert!(
            posted(
                3, "a
b"
            )
            .validate()
            .is_err()
        );
        assert!(posted(3, &"x".repeat(MAX_POST_STEP_NAME_BYTES + 1)).validate().is_err());
        let four: Vec<_> = (0..4).map(|index| (index, MAX_STEP_SUMMARY_BYTES)).collect();
        assert!(summarised(&four).validate().is_ok());
        let five: Vec<_> = (0..5).map(|index| (index, MAX_STEP_SUMMARY_BYTES)).collect();
        assert!(summarised(&five).validate().is_err(), "the task's budget");
        let largest =
            TaskStateRequest { outputs: BTreeMap::from([("big".to_owned(), "x".repeat(MAX_OUTPUT_BYTES - 3))]), ..summarised(&four) };
        assert!(serde_json::to_vec(&largest).unwrap().len() < Endpoint::TaskState.body_cap(), "the largest valid call fits its cap");
    }

    // Actions: artifacts (A).
    #[test]
    fn artifact_names_never_walk_up() {
        for name in ["dist", "dist-linux_x86.64", ".hidden", "...", "a..b"] {
            assert!(valid_artifact_name(name), "{name}");
        }
        for name in ["", ".", "..", "a/b", "a\\b", "../x", "a?b", "a b", "é"] {
            assert!(!valid_artifact_name(name), "{name}");
        }
        assert!(valid_artifact_name(&"x".repeat(128)) && !valid_artifact_name(&"x".repeat(129)));
    }

    #[test]
    fn endpoints_and_caps() {
        let routes = Endpoint::ALL.map(|endpoint| (endpoint.path(), endpoint.body_cap()));
        assert_eq!(
            routes,
            [
                ("register", 65_536),
                ("declare", 65_536),
                ("poll", 4_096),
                ("tasks/{id}/state", 4_194_304),
                ("tasks/{id}/logs", 8_388_608),
                ("unregister", 1_024)
            ]
        );
        assert_eq!((PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS), (1, [1]));
        assert_eq!(TaskId(3).to_string(), "3");
    }

    #[test]
    fn secrets_never_appear_in_debug_output() {
        let request =
            RegisterRequest { token: Secret::new("registration-token"), name: "n".into(), labels: labels(), capabilities: capabilities() };
        let response = RegisterResponse {
            runner_id: RunnerId(1),
            credential: Secret::new(CREDENTIAL),
            intervals: Intervals { long_poll_seconds: 1, heartbeat_seconds: 1 },
        };
        let debug = format!("{request:?} {response:?}");
        assert!(!debug.contains("registration-token") && !debug.contains(CREDENTIAL), "{debug}");
        assert!(debug.contains("Secret(***)"));
    }
}
