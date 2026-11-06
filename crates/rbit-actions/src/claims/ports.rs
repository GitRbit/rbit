//! Borrowed, statically dispatched ports for the existing scheduler's records.
use std::future::Future;

use super::Claimed;

/// Trusted composition supplies concrete model/secret types, without converting them
/// into a second DTO tree. Debug/serialization are deliberately not required of tokens.
pub trait Specification: Sync {
    type Error: Send;
    type Job: Send + Sync;
    type Task: Send + Sync;
    type Token: Send + Sync;
    type Spec: Send + Sync;

    fn new_token(&self) -> Self::Token;
    /// Inner failure means a permanently unrunnable stored specification; an outer
    /// failure keeps the original infrastructure-error behavior (no automatic retry).
    fn specification(
        &self,
        job: &Self::Job,
        task: &Self::Task,
        token: &Self::Token,
    ) -> impl Future<Output = Result<Result<Self::Spec, String>, Self::Error>> + Send;
    fn remember_masks(&self, spec: &Self::Spec);
    fn fail_unrunnable(&self, task: &Self::Task, reason: &str) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// One concrete store handle is reused for the complete nonempty-label claim loop.
/// SQL retains the atomic scope/label/trust/environment/attempt and ephemeral gates.
pub trait ClaimQueue: Specification {
    fn claim(
        &self,
        token: &Self::Token,
        key: Option<&str>,
    ) -> impl Future<Output = Result<Option<Claimed<Self::Job, Self::Task>>, Self::Error>> + Send;
    fn remember_key(&self, key: &str, task: &Self::Task);
    fn claimed(&self, claim: &Claimed<Self::Job, Self::Task>);
    fn create_steps(&self, task: &Self::Task, spec: &Self::Spec) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn report_statuses(&self, job: &Self::Job) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// A replay may rotate only the same runner's still-running task token. Lookup keeps
/// the existing remembered-key path and process-local one-time restart fallback.
pub trait LeaseReplay: Specification {
    fn lookup(&self, key: &str) -> impl Future<Output = Result<Option<Self::Task>, Self::Error>> + Send;
    fn belongs_to_runner(&self, task: &Self::Task) -> bool;
    fn is_running(&self, task: &Self::Task) -> bool;
    /// The SQL CAS refuses a token cleared by concurrent completion/cancellation.
    fn replace_token(&self, task: &Self::Task, token: &Self::Token) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn job(&self, task: &Self::Task) -> impl Future<Output = Result<Option<Self::Job>, Self::Error>> + Send;
    fn resumed(&self, task: &Self::Task);
}
