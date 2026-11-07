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

mod artifacts;
mod endpoints;
mod errors;
mod limits;
mod runners;
mod tasks;
mod tokens;
mod validation;

pub use self::artifacts::{
    ArtifactInfo, ArtifactList, ArtifactQuery, ArtifactRunQuery, CacheEntryInfo, CacheLookupRequest, CacheLookupResponse, CacheSaveQuery,
    valid_artifact_name,
};
pub use self::endpoints::Endpoint;
pub use self::errors::{ErrorBody, ErrorCode};
pub use self::limits::{
    ARTIFACTS_PATH, BASE_PATH, CACHES_PATH, CONTENT_SHA256_HEADER, IDEMPOTENCY_KEY_HEADER, MAX_CAPABILITY_BYTES, MAX_CAPACITY,
    MAX_ENVIRONMENT_URL_BYTES, MAX_GZIP_RATIO, MAX_HEARTBEAT_INTERVAL, MAX_IDEMPOTENCY_KEY_BYTES, MAX_LABEL_CHARS, MAX_LOG_LINE_BYTES,
    MAX_LONG_POLL, MAX_OUTPUT_BYTES, MAX_OUTPUT_KEY_BYTES, MAX_POST_STEP_NAME_BYTES, MAX_REGISTRATION_TOKEN_BYTES, MAX_RUNNER_LABELS,
    MAX_RUNNER_NAME_CHARS, MAX_STEP_SUMMARY_BYTES, MAX_TASK_SUMMARY_BYTES, PROTOCOL_HEADER, PROTOCOL_VERSION, RUNNER_CREDENTIAL_PREFIX,
    SUPPORTED_PROTOCOL_VERSIONS, TASK_TOKEN_PREFIX, TOKEN_BODY_CHARS, USER_AGENT_PREFIX,
};
pub use self::runners::{
    Backend, Capabilities, DeclareRequest, DeclareResponse, DeclaredLabel, Intervals, RegisterRequest, RegisterResponse, RunnerId,
    UnregisterRequest,
};
pub use self::tasks::{
    LogsQuery, LogsResponse, PollRequest, PollResponse, StepState, StepStatus, TaskId, TaskResult, TaskStateRequest, TaskStateResponse,
};
pub use self::tokens::{valid_idempotency_key, valid_runner_credential, valid_task_token};
pub use self::validation::ValidationError;
pub(crate) use self::validation::{ensure, unique, validate_labels, validate_outputs};

#[cfg(test)]
pub(crate) mod tests;
