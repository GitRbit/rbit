//! Column conversions shared by the `actions_*` stores: scopes, statuses, label lists and
//! timestamps. Rows are decoded here into `rbit_core::actions` types; a value that does not
//! decode is reported as [`StorageError::Corrupt`].

use rbit_core::{
    AccountId, RepoId, Timestamp,
    actions::{RunnerLabel, RunnerScope, SecretScope, Status},
};

use crate::StorageError;

pub(crate) fn corrupt(what: &str, detail: impl std::fmt::Display) -> StorageError {
    StorageError::Corrupt(format!("{what}: {detail}"))
}

pub(crate) fn status(code: i64) -> Result<Status, StorageError> {
    Status::from_code(code).ok_or_else(|| corrupt("actions status", code))
}

pub(crate) fn runner_scope(kind: &str, owner: Option<i64>, repo: Option<i64>) -> Result<RunnerScope, StorageError> {
    RunnerScope::from_parts(kind, owner.map(AccountId), repo.map(RepoId)).ok_or_else(|| corrupt("runner scope", kind))
}

pub(crate) fn secret_scope(kind: &str, owner: Option<i64>, repo: Option<i64>) -> Result<SecretScope, StorageError> {
    SecretScope::from_parts(kind, owner.map(AccountId), repo.map(RepoId)).ok_or_else(|| corrupt("secret scope", kind))
}

/// The `(scope_kind, owner_id, repo_id)` columns of a runner scope.
pub(crate) fn runner_columns(scope: RunnerScope) -> (&'static str, Option<i64>, Option<i64>) {
    (scope.kind(), scope.owner().map(|owner| owner.0), scope.repo().map(|repo| repo.0))
}

/// The `(scope_kind, owner_id, repo_id)` columns of a secret scope.
pub(crate) fn secret_columns(scope: SecretScope) -> (&'static str, Option<i64>, Option<i64>) {
    (scope.kind(), scope.owner().map(|owner| owner.0), scope.repo().map(|repo| repo.0))
}

pub(crate) fn labels_json(labels: &[RunnerLabel]) -> Result<String, StorageError> {
    serde_json::to_string(labels).map_err(|error| corrupt("runner labels", error))
}

pub(crate) fn labels(json: &str) -> Result<Vec<RunnerLabel>, StorageError> {
    serde_json::from_str(json).map_err(|error| corrupt("runner labels", error))
}

pub(crate) fn strings_json(values: &[String]) -> Result<String, StorageError> {
    serde_json::to_string(values).map_err(|error| corrupt("string list", error))
}

pub(crate) fn strings(json: &str) -> Result<Vec<String>, StorageError> {
    serde_json::from_str(json).map_err(|error| corrupt("string list", error))
}

pub(crate) fn timestamp(seconds: Option<i64>) -> Option<Timestamp> {
    seconds.map(Timestamp)
}

/// A fixed-size byte column (`key_id`, `nonce`).
pub(crate) fn bytes<const N: usize>(what: &str, value: &[u8]) -> Result<[u8; N], StorageError> {
    value.try_into().map_err(|_| corrupt(what, format!("{} bytes instead of {N}", value.len())))
}
