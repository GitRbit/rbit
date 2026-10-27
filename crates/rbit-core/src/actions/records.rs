//! The stored Actions records as the rest of rbit sees them: runs, jobs, tasks, steps,
//! runners, registration tokens, secrets, variables, schedules and settings. Built by the
//! `actions_*` stores in rbit-storage; tokens appear only as digests there, and never here.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    AccountId, RepoId, Timestamp,
    actions::{
        ArtifactId, DefaultTokenPermission, DeploymentReviewId, EnvironmentId, JobId, JobStatus, RegistrationTokenId, RunId, RunStatus,
        RunnerId, RunnerLabel, RunnerScope, SecretId, SecretName, SecretScope, Status, TaskId, VariableId, VariableName,
    },
};

/// What started a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunEvent {
    Push,
    PullRequest,
    WorkflowDispatch,
    Schedule,
}

impl RunEvent {
    pub const ALL: [Self; 4] = [Self::Push, Self::PullRequest, Self::WorkflowDispatch, Self::Schedule];

    /// The GitHub event name, as stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::PullRequest => "pull_request",
            Self::WorkflowDispatch => "workflow_dispatch",
            Self::Schedule => "schedule",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|event| event.as_str() == value)
    }
}

impl fmt::Display for RunEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A repository's Actions settings. A repository without a stored row has Actions off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepoActionsSettings {
    pub repo_id: RepoId,
    pub enabled: bool,
    pub token_permission: DefaultTokenPermission,
    pub updated_at: Timestamp,
}

/// A workflow run. The event payload and dispatch inputs are read separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub id: RunId,
    pub repo_id: RepoId,
    /// `#n` in the repository, never reused.
    pub number: i64,
    /// `.forgejo/workflows/ci.yml`.
    pub workflow_path: String,
    pub workflow_name: String,
    /// The commit subject, pull request title or "Scheduled".
    pub title: String,
    pub event: RunEvent,
    /// The full ref (`refs/heads/main`, `refs/pull/7/head`).
    pub reference: String,
    pub sha: String,
    pub actor: Option<AccountId>,
    pub trigger_actor: Option<AccountId>,
    pub pull_number: Option<i64>,
    pub is_fork: bool,
    /// Untrusted runs get a read-only token, no secrets and no variables.
    pub trusted: bool,
    pub status: RunStatus,
    pub attempt: i64,
    pub concurrency_group: Option<String>,
    pub cancel_in_progress: bool,
    /// Why the workflow produced no jobs.
    pub error: Option<String>,
    pub approved_by: Option<AccountId>,
    pub approved_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
    pub logs_expired_at: Option<Timestamp>,
}

impl Run {
    /// A run recorded for a workflow that failed to parse or plan.
    pub fn is_workflow_error(&self) -> bool {
        self.error.is_some()
    }
}

/// One job of a run. Its payload and outputs are read separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: JobId,
    pub run_id: RunId,
    pub repo_id: RepoId,
    /// The key under `jobs` in the workflow.
    pub key: String,
    pub name: String,
    /// The matrix combination as a JSON object, `None` without a matrix.
    pub matrix: Option<String>,
    /// The job keys this job waits for.
    pub needs: Vec<String>,
    /// The labels a runner must have.
    pub runs_on: Vec<String>,
    /// The effective `permissions:` as a JSON object, `None` when the workflow sets none.
    pub permissions: Option<String>,
    pub status: JobStatus,
    pub attempt: i64,
    pub task_id: Option<TaskId>,
    pub runner_id: Option<RunnerId>,
    pub created_at: Timestamp,
    pub queued_at: Option<Timestamp>,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
    // Actions: environments.
    /// The deployment environment the job runs in, by name.
    pub environment: Option<String>,
    /// Its `environment.url` as the runner evaluated it after the steps.
    pub environment_url: Option<String>,
    /// Waiting for a reviewer of its environment (its status is then awaiting approval).
    pub awaiting_review: bool,
}

/// Where a task's log is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogState {
    /// Being written: a plain file.
    Live,
    /// Compressed after the task finished.
    Final,
    /// Removed by retention or the instance log budget.
    Expired,
}

impl LogState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Final => "final",
            Self::Expired => "expired",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Live, Self::Final, Self::Expired].into_iter().find(|state| state.as_str() == value)
    }
}

/// One attempt of a job on a runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: TaskId,
    pub job_id: JobId,
    pub attempt: i64,
    pub runner_id: Option<RunnerId>,
    pub status: Status,
    pub started_at: Timestamp,
    pub stopped_at: Option<Timestamp>,
    pub heartbeat_at: Timestamp,
    pub log_lines: i64,
    pub log_bytes: i64,
    pub log_state: LogState,
    pub log_truncated: bool,
}

/// One step of a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub task_id: TaskId,
    pub number: i64,
    pub name: String,
    pub status: Status,
    pub started_at: Option<Timestamp>,
    pub stopped_at: Option<Timestamp>,
    /// The step's first line in the task's log, and how many lines it has.
    pub log_index: i64,
    pub log_length: i64,
}

/// A registered runner. Its token is known only as a digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runner {
    pub id: RunnerId,
    pub uuid: String,
    pub name: String,
    pub scope: RunnerScope,
    pub labels: Vec<RunnerLabel>,
    /// The labels its registration token allowed, `None` for any.
    pub pinned_labels: Option<Vec<RunnerLabel>>,
    pub version: String,
    pub ephemeral: bool,
    pub disabled: bool,
    pub last_online: Option<Timestamp>,
    pub created_at: Timestamp,
    // Actions: S.
    /// What it declared it can do at its last start.
    pub capabilities: RunnerCapabilities,
    /// The runner protocol version it speaks.
    pub protocol: u32,
}

/// A runner that reported this recently counts as online; idle runners poll every 25 s.
pub const RUNNER_ONLINE_SECONDS: i64 = 90;

impl Runner {
    /// Whether it reported within [`RUNNER_ONLINE_SECONDS`] of `now` (disabled or not).
    pub fn is_online(&self, now: Timestamp) -> bool {
        self.last_online.is_some_and(|seen| now.seconds() - seen.seconds() <= RUNNER_ONLINE_SECONDS)
    }
}

// Actions: S.
/// How a runner runs the jobs of a label (`rbit_workflow::protocol::Backend`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerBackend {
    /// Docker or Podman.
    Container,
    /// Child processes of the runner's user: no isolation.
    Host,
}

/// What a runner declared about itself, stored as JSON (`actions_runners.capabilities`).
/// Unknown keys are ignored and missing ones default, so a newer runner's declaration and
/// a runner that has not declared yet both read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RunnerCapabilities {
    /// The backends that work on its host.
    pub backends: Vec<RunnerBackend>,
    /// `linux`, `macos`, `windows`.
    pub os: String,
    /// `x86_64`, `aarch64`.
    pub arch: String,
    /// Tasks it runs at once.
    pub capacity: u32,
    /// The labels it runs on the host backend: jobs there have no isolation.
    pub host_labels: Vec<RunnerLabel>,
}

impl RunnerCapabilities {
    /// Whether any of its labels runs jobs on the host backend ("host — no isolation").
    pub fn runs_on_host(&self) -> bool {
        !self.host_labels.is_empty()
    }
}

/// A runner registration token. Its value is shown once and stored as a digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationToken {
    pub id: RegistrationTokenId,
    pub scope: RunnerScope,
    /// A runner registered with it may declare only these labels.
    pub labels: Option<Vec<RunnerLabel>>,
    /// `None`: unlimited.
    pub uses_remaining: Option<i64>,
    pub created_by: Option<AccountId>,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
    pub revoked_at: Option<Timestamp>,
}

impl RegistrationToken {
    /// Whether a runner can still register with it at `now`.
    pub fn is_usable(&self, now: Timestamp) -> bool {
        self.revoked_at.is_none() && now < self.expires_at && self.uses_remaining.is_none_or(|uses| uses > 0)
    }
}

/// Which repositories of its owner an owner secret is given to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SecretVisibility {
    #[default]
    All,
    Selected,
}

impl SecretVisibility {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Selected => "selected",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::All, Self::Selected].into_iter().find(|visibility| visibility.as_str() == value)
    }
}

/// A secret's AES-256-GCM ciphertext as stored.
#[derive(Clone, PartialEq, Eq)]
pub struct SealedSecret {
    /// The first 8 bytes of SHA-256 of the key that sealed it.
    pub key_id: [u8; 8],
    pub nonce: [u8; 12],
    /// The ciphertext with its 16-byte tag.
    pub ciphertext: Vec<u8>,
}

impl fmt::Debug for SealedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SealedSecret").field("key_id", &self.key_id).field("ciphertext_len", &self.ciphertext.len()).finish_non_exhaustive()
    }
}

/// A secret as settings show it: everything but the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretInfo {
    pub id: SecretId,
    pub scope: SecretScope,
    pub name: SecretName,
    pub visibility: SecretVisibility,
    pub key_id: [u8; 8],
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub updated_by: Option<AccountId>,
}

/// A configuration variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    pub id: VariableId,
    pub scope: SecretScope,
    pub name: VariableName,
    pub value: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub updated_by: Option<AccountId>,
}

/// One `schedule` entry of a default-branch workflow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleEntry {
    pub repo_id: RepoId,
    pub workflow_path: String,
    pub cron: String,
    pub next_at: Timestamp,
    pub failures: i64,
    pub paused: bool,
}

/// A pull request author whose untrusted runs need no approval in a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedUser {
    pub repo_id: RepoId,
    pub user_id: AccountId,
    pub approved_by: Option<AccountId>,
    pub created_at: Timestamp,
}

// Actions: artifacts (A).
/// Files a job of a run uploaded, as one tar.gz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub id: ArtifactId,
    pub run_id: RunId,
    pub repo_id: RepoId,
    /// The run attempt that uploaded it.
    pub attempt: i64,
    pub job_id: Option<JobId>,
    pub name: String,
    /// Bytes of the tar.gz.
    pub size: u64,
    pub sha256: [u8; 32],
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_names_round_trip() {
        for event in RunEvent::ALL {
            assert_eq!(RunEvent::parse(event.as_str()), Some(event));
        }
        assert_eq!(RunEvent::parse("pull_request_target"), None);
        for state in [LogState::Live, LogState::Final, LogState::Expired] {
            assert_eq!(LogState::parse(state.as_str()), Some(state));
        }
        for visibility in [SecretVisibility::All, SecretVisibility::Selected] {
            assert_eq!(SecretVisibility::parse(visibility.as_str()), Some(visibility));
        }
    }

    #[test]
    fn registration_tokens_expire_run_out_and_can_be_revoked() {
        let token = RegistrationToken {
            id: RegistrationTokenId(1),
            scope: RunnerScope::Instance,
            labels: None,
            uses_remaining: Some(1),
            created_by: None,
            created_at: Timestamp(100),
            expires_at: Timestamp(200),
            revoked_at: None,
        };
        assert!(token.is_usable(Timestamp(199)));
        assert!(!token.is_usable(Timestamp(200)));
        assert!(!RegistrationToken { uses_remaining: Some(0), ..token.clone() }.is_usable(Timestamp(150)));
        assert!(RegistrationToken { uses_remaining: None, ..token.clone() }.is_usable(Timestamp(150)));
        assert!(!RegistrationToken { revoked_at: Some(Timestamp(120)), ..token }.is_usable(Timestamp(150)));
    }

    #[test]
    fn sealed_secrets_do_not_print_their_ciphertext() {
        let sealed = SealedSecret { key_id: [1; 8], nonce: [2; 12], ciphertext: vec![0xAB; 20] };
        let printed = format!("{sealed:?}");
        assert!(printed.contains("ciphertext_len: 20") && !printed.contains("171"), "{printed}");
    }
}

// Actions: environments.
/// Which refs may deploy to an environment.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BranchPolicy {
    /// Any branch or tag.
    #[default]
    All,
    /// Branches with a branch protection rule.
    Protected,
    /// Branches and tags matching these GitHub filter patterns (`main`, `release/*`).
    Selected(Vec<String>),
}

impl BranchPolicy {
    /// The stored `branch_policy`.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Protected => "protected",
            Self::Selected(_) => "selected",
        }
    }
}

/// A repository's deployment environment (Actions: environments). A job that names it with
/// `environment:` gets its secrets and variables over the repository's, waits for one of its
/// reviewers when it has any, and runs only from the refs its branch policy allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    pub id: EnvironmentId,
    pub repo_id: RepoId,
    pub name: String,
    /// Who may approve its jobs; none means they run without review.
    pub reviewers: Vec<AccountId>,
    /// Whoever started the run may not approve it.
    pub prevent_self_review: bool,
    pub branch_policy: BranchPolicy,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A reviewer's decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReviewDecision {
    Approved,
    Rejected,
}

impl ReviewDecision {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Approved, Self::Rejected].into_iter().find(|decision| decision.as_str() == value)
    }
}

/// A reviewer's decision on one attempt of a job waiting for its environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentReview {
    pub id: DeploymentReviewId,
    pub job_id: JobId,
    pub attempt: i64,
    pub environment: String,
    pub reviewer: Option<AccountId>,
    pub decision: ReviewDecision,
    pub comment: String,
    pub created_at: Timestamp,
}
