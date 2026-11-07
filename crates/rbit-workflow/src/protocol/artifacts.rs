//! Protocol artifacts contracts and original validation.
use serde::{Deserialize, Serialize};

/// The shape of an artifact name: 1 to 128 of `[A-Za-z0-9._-]`, but not `.` or `..`
/// (§12.7). A name is a path segment of [`crate::protocol::ARTIFACTS_PATH`] and the directory a runner
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

/// The answer to [`crate::protocol::CacheLookupRequest`]: the entry to restore, if any.
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
