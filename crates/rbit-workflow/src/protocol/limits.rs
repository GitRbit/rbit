//! Protocol limits contracts and original validation.
use std::time::Duration;

/// The protocol this build speaks.
pub const PROTOCOL_VERSION: u32 = 1;

/// The versions a server built from this crate accepts; listed in an
/// [`crate::protocol::ErrorCode::UnsupportedProtocol`] answer.
pub const SUPPORTED_PROTOCOL_VERSIONS: [u32; 1] = [PROTOCOL_VERSION];

/// Where every endpoint lives; [`crate::protocol::Endpoint::path`] is relative to it.
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

/// Longest [`crate::protocol::IDEMPOTENCY_KEY_HEADER`] value, in bytes.
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

pub(super) const KIB: usize = 1024;

pub(super) const MIB: usize = 1024 * KIB;

/// Longest environment URL a runner may report.
pub const MAX_ENVIRONMENT_URL_BYTES: usize = 2048;

/// Longest name of a step the runner adds.
pub const MAX_POST_STEP_NAME_BYTES: usize = 256;

// Actions: artifacts (A).
/// Where a task's artifacts are, below [`crate::protocol::BASE_PATH`] (`{id}` is the task id):
/// `PUT …/{name}` uploads one (a tar.gz, with [`crate::protocol::CONTENT_SHA256_HEADER`] and
/// [`crate::protocol::ArtifactQuery`]), `GET` lists the run's ([`crate::protocol::ArtifactList`]), `GET …/{name}` downloads one.
/// The upload's cap is the server's `[actions] max_artifact_bytes`, not a protocol constant.
pub const ARTIFACTS_PATH: &str = "tasks/{id}/artifacts";

// Actions: shared cache.
/// Where a task's cache entries are, below [`crate::protocol::BASE_PATH`] (`{id}` is the task id), when the
/// server declares [`crate::protocol::DeclareResponse::shared_cache`]: `POST …/lookup` finds the entry to
/// restore ([`crate::protocol::CacheLookupRequest`], answered with [`crate::protocol::CacheLookupResponse`]), `GET …/{entry}`
/// downloads it (a tar.gz, with [`crate::protocol::CONTENT_SHA256_HEADER`]), and `PUT` saves one (with
/// [`crate::protocol::CONTENT_SHA256_HEADER`] and [`crate::protocol::CacheSaveQuery`]; `201` and its [`crate::protocol::CacheEntryInfo`], or
/// `409` when the key is saved already). The server picks the scopes from the task's run.
pub const CACHES_PATH: &str = "tasks/{id}/caches";
