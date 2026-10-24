//! rbit Actions: the value types shared by storage, services and web, and the pure
//! decisions the scheduler makes with them (see `docs/design/actions.md`).
//!
//! * [`Status`] is the life cycle of runs and jobs, numbered as in Forgejo so the values in
//!   the database and on the wire agree; [`aggregate`] derives a run's status from its jobs.
//! * [`RunnerScope`] and [`runner_sees`] decide which runners may take which jobs;
//!   [`runs_on`] matches a job's `runs-on` labels against a runner's labels.
//! * [`classify_pull_request`] decides whether a pull request run is trusted (§11.1), and
//!   [`token_permissions`] what its task token may do (§11.4).
//! * [`RunnerLabel`], [`SecretName`] and [`VariableName`] are validated at construction.
//!
//! The Actions foundation (workstream F) owns this file.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{AccountId, RepoAccess, RepoId};

macro_rules! id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct $name(pub i64);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

id!(
    /// A workflow run: one workflow file triggered by one event.
    RunId
);
id!(
    /// One job of a run (one matrix combination of one workflow job).
    JobId
);
id!(
    /// One attempt of a job on a runner. Never reused, so a task token cannot outlive it.
    TaskId
);
id!(
    /// A registered runner.
    RunnerId
);
id!(
    /// A runner registration token.
    RegistrationTokenId
);
id!(
    /// An encrypted secret. Its id is part of the ciphertext's associated data.
    SecretId
);
id!(
    /// A plain configuration variable.
    VariableId
);

/// The life cycle of a run or a job, with Forgejo's numbers (`0` is Forgejo's "unknown"
/// and never stored). [`Status::AwaitingApproval`] is rbit's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Success = 1,
    Failure = 2,
    Cancelled = 3,
    Skipped = 4,
    /// Ready for a runner.
    Waiting = 5,
    Running = 6,
    /// A job whose `needs` have not all finished, or a run waiting for its concurrency group.
    Blocked = 7,
    /// An untrusted pull request run nobody has approved yet.
    AwaitingApproval = 8,
}

/// The status of a run.
pub type RunStatus = Status;
/// The status of a job.
pub type JobStatus = Status;

impl Status {
    pub const ALL: [Self; 8] =
        [Self::Success, Self::Failure, Self::Cancelled, Self::Skipped, Self::Waiting, Self::Running, Self::Blocked, Self::AwaitingApproval];

    /// The stored and wire value.
    pub const fn code(self) -> i64 {
        self as i64
    }

    pub fn from_code(code: i64) -> Option<Self> {
        Self::ALL.into_iter().find(|status| status.code() == code)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
            Self::Waiting => "waiting",
            Self::Running => "running",
            Self::Blocked => "blocked",
            Self::AwaitingApproval => "awaiting_approval",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|status| status.as_str() == value)
    }

    /// Success, failure, cancelled or skipped: nothing will change it except a re-run.
    pub const fn is_done(self) -> bool {
        matches!(self, Self::Success | Self::Failure | Self::Cancelled | Self::Skipped)
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A run's status from its jobs' statuses: all skipped → skipped; all success or skipped →
/// success; otherwise the first present of running, waiting, blocked, awaiting approval,
/// cancelled, failure. A run without jobs failed (it is a workflow error).
pub fn aggregate(jobs: impl IntoIterator<Item = JobStatus>) -> RunStatus {
    const PRECEDENCE: [Status; 6] =
        [Status::Running, Status::Waiting, Status::Blocked, Status::AwaitingApproval, Status::Cancelled, Status::Failure];
    // One bit per status code.
    let present = jobs.into_iter().fold(0u16, |present, status| present | 1 << status.code());
    let has = |status: Status| present & 1 << status.code() != 0;
    let only = |allowed: &[Status]| Status::ALL.into_iter().all(|status| allowed.contains(&status) || !has(status));
    if present == 0 {
        Status::Failure
    } else if only(&[Status::Skipped]) {
        Status::Skipped
    } else if only(&[Status::Success, Status::Skipped]) {
        Status::Success
    } else {
        PRECEDENCE.into_iter().find(|&status| has(status)).unwrap_or(Status::Failure)
    }
}

/// Why a submitted Actions name was rejected. The message is safe to show to end users.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ActionsNameError {
    #[error("{kind} must not be empty")]
    Empty { kind: &'static str },
    #[error("{kind} must be at most {max} characters")]
    TooLong { kind: &'static str, max: usize },
    #[error("{kind} may only contain {allowed}")]
    InvalidCharacter { kind: &'static str, allowed: &'static str },
    #[error("{kind} must not start with a digit")]
    LeadingDigit { kind: &'static str },
    #[error("{kind} must not start with {prefix}")]
    ReservedPrefix { kind: &'static str, prefix: &'static str },
}

macro_rules! validated_name {
    ($(#[$meta:meta])* $name:ident, $parse:path) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl FromStr for $name {
            type Err = ActionsNameError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                $parse(value).map(Self)
            }
        }

        impl TryFrom<String> for $name {
            type Error = ActionsNameError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                value.parse()
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

/// Longest runner label, in characters.
pub const MAX_LABEL_CHARS: usize = 64;
/// Most labels one runner may declare.
pub const MAX_RUNNER_LABELS: usize = 64;
/// Longest secret or variable name, in characters.
pub const MAX_NAME_CHARS: usize = 100;
/// Prefixes of the names runners set themselves; a secret or variable cannot shadow them.
pub const RESERVED_NAME_PREFIXES: [&str; 4] = ["GITHUB_", "GITEA_", "FORGEJO_", "RBIT_"];

fn parse_label(value: &str) -> Result<String, ActionsNameError> {
    const KIND: &str = "runner label";
    if value.is_empty() {
        return Err(ActionsNameError::Empty { kind: KIND });
    }
    if value.chars().count() > MAX_LABEL_CHARS {
        return Err(ActionsNameError::TooLong { kind: KIND, max: MAX_LABEL_CHARS });
    }
    if !value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')) {
        return Err(ActionsNameError::InvalidCharacter { kind: KIND, allowed: "letters, digits, '.', '_' and '-'" });
    }
    Ok(value.to_owned())
}

/// Secret and variable names are case-insensitive, as on GitHub and Forgejo: they are
/// stored upper-cased.
fn parse_environment_name(kind: &'static str, value: &str) -> Result<String, ActionsNameError> {
    let value = value.trim().to_ascii_uppercase();
    if value.is_empty() {
        return Err(ActionsNameError::Empty { kind });
    }
    if value.len() > MAX_NAME_CHARS {
        return Err(ActionsNameError::TooLong { kind, max: MAX_NAME_CHARS });
    }
    if !value.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_') {
        return Err(ActionsNameError::InvalidCharacter { kind, allowed: "letters, digits and '_'" });
    }
    if value.starts_with(|c: char| c.is_ascii_digit()) {
        return Err(ActionsNameError::LeadingDigit { kind });
    }
    if let Some(prefix) = RESERVED_NAME_PREFIXES.into_iter().find(|prefix| value.starts_with(prefix)) {
        return Err(ActionsNameError::ReservedPrefix { kind, prefix });
    }
    Ok(value)
}

fn parse_secret_name(value: &str) -> Result<String, ActionsNameError> {
    parse_environment_name("secret name", value)
}

fn parse_variable_name(value: &str) -> Result<String, ActionsNameError> {
    parse_environment_name("variable name", value)
}

validated_name!(
    /// A label a runner declares or a job asks for in `runs-on` (`ubuntu-latest`,
    /// `self-hosted`): 1 to [`MAX_LABEL_CHARS`] of `[A-Za-z0-9._-]`. Compared exactly.
    RunnerLabel,
    parse_label
);
validated_name!(
    /// The name of a secret: `[A-Z_][A-Z0-9_]*`, at most [`MAX_NAME_CHARS`], not starting
    /// with a [`RESERVED_NAME_PREFIXES`] entry. Input is upper-cased first.
    SecretName,
    parse_secret_name
);
validated_name!(
    /// The name of a variable, with the rules of [`SecretName`].
    VariableName,
    parse_variable_name
);

/// Whether a runner with `runner` labels can run a job asking for `job` labels: the job
/// asks for at least one label and the runner has every one of them.
pub fn runs_on(job: &[RunnerLabel], runner: &[RunnerLabel]) -> bool {
    !job.is_empty() && job.iter().all(|label| runner.contains(label))
}

/// Who a runner works for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RunnerScope {
    /// Registered by an administrator. Serves the owners an administrator allowed, or
    /// everyone with `[actions] instance_runners_for_all`.
    Instance,
    /// Serves every repository of one user or organisation.
    Owner(AccountId),
    /// Serves one repository.
    Repository(RepoId),
}

impl RunnerScope {
    /// The stored `scope_kind`.
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Instance => "instance",
            Self::Owner(_) => "owner",
            Self::Repository(_) => "repository",
        }
    }

    /// Rebuilds a scope from its stored columns; `None` when they disagree.
    pub fn from_parts(kind: &str, owner: Option<AccountId>, repo: Option<RepoId>) -> Option<Self> {
        match (kind, owner, repo) {
            ("instance", None, None) => Some(Self::Instance),
            ("owner", Some(owner), None) => Some(Self::Owner(owner)),
            ("repository", None, Some(repo)) => Some(Self::Repository(repo)),
            _ => None,
        }
    }

    /// The `owner_id` column.
    pub const fn owner(self) -> Option<AccountId> {
        if let Self::Owner(owner) = self { Some(owner) } else { None }
    }

    /// The `repo_id` column.
    pub const fn repo(self) -> Option<RepoId> {
        if let Self::Repository(repo) = self { Some(repo) } else { None }
    }
}

/// Whether a runner of `scope` may take a job of repository `repo` owned by `owner`.
/// `instance_runners_allowed` is true when instance runners serve that owner (the owner is
/// allowed by an administrator, or `[actions] instance_runners_for_all` is on).
pub fn runner_sees(scope: RunnerScope, repo: RepoId, owner: AccountId, instance_runners_allowed: bool) -> bool {
    match scope {
        RunnerScope::Instance => instance_runners_allowed,
        RunnerScope::Owner(id) => id == owner,
        RunnerScope::Repository(id) => id == repo,
    }
}

/// Where a secret or variable is defined. Repository values win over owner values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SecretScope {
    Owner(AccountId),
    Repository(RepoId),
}

impl SecretScope {
    /// The stored `scope_kind`.
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Owner(_) => "owner",
            Self::Repository(_) => "repository",
        }
    }

    /// The account or repository id.
    pub const fn id(self) -> i64 {
        match self {
            Self::Owner(AccountId(id)) | Self::Repository(RepoId(id)) => id,
        }
    }

    /// Rebuilds a scope from its stored columns; `None` when they disagree.
    pub fn from_parts(kind: &str, owner: Option<AccountId>, repo: Option<RepoId>) -> Option<Self> {
        match (kind, owner, repo) {
            ("owner", Some(owner), None) => Some(Self::Owner(owner)),
            ("repository", None, Some(repo)) => Some(Self::Repository(repo)),
            _ => None,
        }
    }

    /// The `owner_id` column.
    pub const fn owner(self) -> Option<AccountId> {
        if let Self::Owner(owner) = self { Some(owner) } else { None }
    }

    /// The `repo_id` column.
    pub const fn repo(self) -> Option<RepoId> {
        if let Self::Repository(repo) = self { Some(repo) } else { None }
    }
}

/// The actor whose action caused a pull request event: the author for `opened`,
/// `reopened`, `ready_for_review` and `edited`, the pusher of the new head for
/// `synchronize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggeringActor {
    /// The pusher of the head is not known (or the head moved without a user push).
    Unknown,
    Known {
        /// The actor's access to the base repository now.
        access: RepoAccess,
        suspended: bool,
        /// The actor is the pull request's author.
        is_author: bool,
    },
}

/// What the base repository knows about a pull request's author.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorStanding {
    /// Nothing that vouches for them.
    Unknown,
    /// Listed in the repository's trusted users, or has a pull request merged into it.
    Vouched,
    /// Suspended: never approved automatically.
    Suspended,
}

/// How a pull request run may run (§11.1 of the design).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunTrust {
    /// Secrets, variables, and the repository's token permission.
    Trusted,
    /// A read-only token, no secrets and no variables — whoever approves it.
    Untrusted {
        /// The run waits for a writer's approval before any job is dispatched.
        awaits_approval: bool,
    },
}

impl RunTrust {
    pub const fn is_trusted(self) -> bool {
        matches!(self, Self::Trusted)
    }
}

/// Classifies a `pull_request` run when it is created. Trusted only when the actor who
/// caused the event can write to the base repository now and is not suspended. An untrusted
/// run skips the approval wait only when the actor is the author and the author is vouched
/// for — approval never grants secrets.
pub fn classify_pull_request(actor: TriggeringActor, author: AuthorStanding) -> RunTrust {
    match actor {
        TriggeringActor::Known { access, suspended: false, .. } if access.can_write() => RunTrust::Trusted,
        TriggeringActor::Known { suspended: false, is_author: true, .. } => {
            RunTrust::Untrusted { awaits_approval: author != AuthorStanding::Vouched }
        }
        TriggeringActor::Known { .. } | TriggeringActor::Unknown => RunTrust::Untrusted { awaits_approval: true },
    }
}

/// One coarse permission of a task token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionLevel {
    #[default]
    None,
    Read,
    Write,
}

impl PermissionLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Read => "read",
            Self::Write => "write",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::None, Self::Read, Self::Write].into_iter().find(|level| level.as_str() == value)
    }
}

/// The coarse permissions a workflow declares or a token carries: `contents` (Git, releases),
/// `issues` and `pull-requests` (comments). Every other GitHub permission is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TokenPermissions {
    pub contents: PermissionLevel,
    pub issues: PermissionLevel,
    pub pull_requests: PermissionLevel,
}

impl TokenPermissions {
    /// Every permission at `level` (`read-all`, `write-all`, and `{}` for `None`).
    pub const fn uniform(level: PermissionLevel) -> Self {
        Self { contents: level, issues: level, pull_requests: level }
    }

    /// Each permission at most `limit`'s.
    #[must_use]
    pub fn at_most(self, limit: Self) -> Self {
        Self {
            contents: self.contents.min(limit.contents),
            issues: self.issues.min(limit.issues),
            pull_requests: self.pull_requests.min(limit.pull_requests),
        }
    }
}

/// A repository's default workflow token permission (its Actions settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultTokenPermission {
    #[default]
    Read,
    Write,
}

impl DefaultTokenPermission {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Read, Self::Write].into_iter().find(|level| level.as_str() == value)
    }

    pub const fn permissions(self) -> TokenPermissions {
        TokenPermissions::uniform(match self {
            Self::Read => PermissionLevel::Read,
            Self::Write => PermissionLevel::Write,
        })
    }
}

/// What a task token may do (§11.4): the repository default, lowered by the job's
/// `permissions:` or else the workflow's, and read-only for untrusted runs. Declarations
/// only ever lower the default.
pub fn token_permissions(
    default: DefaultTokenPermission,
    workflow: Option<TokenPermissions>,
    job: Option<TokenPermissions>,
    trust: RunTrust,
) -> TokenPermissions {
    let mut granted = default.permissions();
    if let Some(declared) = job.or(workflow) {
        granted = granted.at_most(declared);
    }
    if !trust.is_trusted() {
        granted = granted.at_most(TokenPermissions::uniform(PermissionLevel::Read));
    }
    granted
}

#[cfg(test)]
mod tests {
    use super::*;

    use Status::{AwaitingApproval, Blocked, Cancelled, Failure, Running, Skipped, Success, Waiting};

    #[test]
    fn status_codes_follow_forgejo() {
        let codes: Vec<_> = Status::ALL.iter().map(|status| status.code()).collect();
        assert_eq!(codes, [1, 2, 3, 4, 5, 6, 7, 8]);
        for status in Status::ALL {
            assert_eq!(Status::from_code(status.code()), Some(status));
            assert_eq!(Status::parse(status.as_str()), Some(status));
            assert_eq!(status.to_string(), status.as_str());
        }
        assert_eq!(Status::from_code(0), None);
        assert_eq!(Status::from_code(9), None);
        assert_eq!(Status::parse("Success"), None);
        let done: Vec<_> = Status::ALL.into_iter().filter(|status| status.is_done()).collect();
        assert_eq!(done, [Success, Failure, Cancelled, Skipped]);
    }

    #[test]
    fn aggregate_table() {
        let cases: &[(&[Status], Status)] = &[
            (&[], Failure),
            (&[Skipped], Skipped),
            (&[Skipped, Skipped], Skipped),
            (&[Success], Success),
            (&[Success, Skipped], Success),
            (&[Skipped, Success, Success], Success),
            (&[Failure], Failure),
            (&[Cancelled], Cancelled),
            (&[Failure, Cancelled], Cancelled),
            (&[Success, Failure], Failure),
            (&[Skipped, Failure], Failure),
            (&[Waiting], Waiting),
            (&[Blocked], Blocked),
            (&[AwaitingApproval], AwaitingApproval),
            (&[Running], Running),
            (&[Running, Waiting, Blocked, AwaitingApproval, Cancelled, Failure, Success, Skipped], Running),
            (&[Waiting, Blocked, AwaitingApproval, Cancelled, Failure], Waiting),
            (&[Blocked, AwaitingApproval, Cancelled, Failure], Blocked),
            (&[AwaitingApproval, Cancelled, Failure], AwaitingApproval),
            (&[Success, Blocked], Blocked),
            (&[Failure, Waiting], Waiting),
            (&[Cancelled, Success], Cancelled),
        ];
        for (jobs, expected) in cases {
            assert_eq!(aggregate(jobs.iter().copied()), *expected, "{jobs:?}");
            assert_eq!(aggregate(jobs.iter().rev().copied()), *expected, "order must not matter: {jobs:?}");
        }
    }

    #[test]
    fn aggregate_of_every_pair_is_one_of_its_inputs_or_success() {
        for a in Status::ALL {
            for b in Status::ALL {
                let result = aggregate([a, b]);
                assert!(result == a || result == b || result == Success, "{a:?} {b:?} → {result:?}");
                assert_eq!(result.is_done(), a.is_done() && b.is_done(), "{a:?} {b:?}");
            }
        }
    }

    fn labels(values: &[&str]) -> Vec<RunnerLabel> {
        values.iter().map(|value| value.parse().unwrap()).collect()
    }

    #[test]
    fn runner_labels() {
        for valid in ["ubuntu-latest", "self-hosted", "ubuntu-22.04", "a", "X_Y", &"a".repeat(64)] {
            assert_eq!(valid.parse::<RunnerLabel>().unwrap().as_str(), valid);
        }
        assert_eq!("".parse::<RunnerLabel>(), Err(ActionsNameError::Empty { kind: "runner label" }));
        assert!(matches!("a".repeat(65).parse::<RunnerLabel>(), Err(ActionsNameError::TooLong { max: 64, .. })));
        for invalid in ["ubuntu latest", "docker://node", "a:b", "é", " a", "a/b", "${{ matrix.os }}"] {
            assert!(matches!(invalid.parse::<RunnerLabel>(), Err(ActionsNameError::InvalidCharacter { .. })), "{invalid}");
        }
        let label: RunnerLabel = serde_json::from_str("\"linux\"").unwrap();
        assert_eq!(serde_json::to_string(&label).unwrap(), "\"linux\"");
        assert!(serde_json::from_str::<RunnerLabel>("\"a b\"").is_err());
    }

    #[test]
    fn runs_on_table() {
        let runner = labels(&["self-hosted", "linux", "x64"]);
        let cases: &[(&[&str], bool)] = &[
            (&[], false),
            (&["linux"], true),
            (&["self-hosted", "linux"], true),
            (&["self-hosted", "linux", "x64"], true),
            (&["linux", "linux"], true),
            (&["windows"], false),
            (&["linux", "arm64"], false),
            (&["Linux"], false),
        ];
        for (job, expected) in cases {
            assert_eq!(runs_on(&labels(job), &runner), *expected, "{job:?}");
        }
        assert!(!runs_on(&labels(&["linux"]), &[]));
        assert!(!runs_on(&[], &[]));
    }

    #[test]
    fn secret_and_variable_names() {
        for (input, stored) in [("TOKEN", "TOKEN"), ("npm_token", "NPM_TOKEN"), (" _X1 ", "_X1"), ("A", "A"), ("GITHUB", "GITHUB")] {
            assert_eq!(input.parse::<SecretName>().unwrap().as_str(), stored);
            assert_eq!(input.parse::<VariableName>().unwrap().as_str(), stored);
        }
        assert!("A".repeat(100).parse::<SecretName>().is_ok());
        let cases: &[(&str, ActionsNameError)] = &[
            ("", ActionsNameError::Empty { kind: "secret name" }),
            ("   ", ActionsNameError::Empty { kind: "secret name" }),
            (&"A".repeat(101), ActionsNameError::TooLong { kind: "secret name", max: 100 }),
            ("1TOKEN", ActionsNameError::LeadingDigit { kind: "secret name" }),
            ("MY-TOKEN", ActionsNameError::InvalidCharacter { kind: "secret name", allowed: "letters, digits and '_'" }),
            ("MY TOKEN", ActionsNameError::InvalidCharacter { kind: "secret name", allowed: "letters, digits and '_'" }),
            ("TÖKEN", ActionsNameError::InvalidCharacter { kind: "secret name", allowed: "letters, digits and '_'" }),
            ("GITHUB_TOKEN", ActionsNameError::ReservedPrefix { kind: "secret name", prefix: "GITHUB_" }),
            ("gitea_x", ActionsNameError::ReservedPrefix { kind: "secret name", prefix: "GITEA_" }),
            ("Forgejo_Token", ActionsNameError::ReservedPrefix { kind: "secret name", prefix: "FORGEJO_" }),
            ("RBIT_", ActionsNameError::ReservedPrefix { kind: "secret name", prefix: "RBIT_" }),
        ];
        for (input, error) in cases {
            assert_eq!(input.parse::<SecretName>().as_ref(), Err(error), "{input}");
        }
        assert!(matches!("RBIT_X".parse::<VariableName>(), Err(ActionsNameError::ReservedPrefix { kind: "variable name", .. })));
    }

    #[test]
    fn scopes_round_trip_through_their_columns() {
        for scope in [RunnerScope::Instance, RunnerScope::Owner(AccountId(3)), RunnerScope::Repository(RepoId(4))] {
            assert_eq!(RunnerScope::from_parts(scope.kind(), scope.owner(), scope.repo()), Some(scope));
        }
        assert_eq!(RunnerScope::from_parts("instance", Some(AccountId(1)), None), None);
        assert_eq!(RunnerScope::from_parts("owner", None, None), None);
        assert_eq!(RunnerScope::from_parts("owner", Some(AccountId(1)), Some(RepoId(1))), None);
        assert_eq!(RunnerScope::from_parts("repository", Some(AccountId(1)), None), None);
        assert_eq!(RunnerScope::from_parts("global", None, None), None);
        for scope in [SecretScope::Owner(AccountId(3)), SecretScope::Repository(RepoId(4))] {
            assert_eq!(SecretScope::from_parts(scope.kind(), scope.owner(), scope.repo()), Some(scope));
        }
        assert_eq!((SecretScope::Owner(AccountId(3)).id(), SecretScope::Repository(RepoId(4)).id()), (3, 4));
        assert_eq!(SecretScope::from_parts("instance", None, None), None);
        assert_eq!(SecretScope::from_parts("repository", None, None), None);
    }

    #[test]
    fn runner_visibility_table() {
        let (repo, other_repo, owner, other_owner) = (RepoId(1), RepoId(2), AccountId(10), AccountId(20));
        let cases = [
            (RunnerScope::Instance, true, true),
            (RunnerScope::Instance, false, false),
            (RunnerScope::Owner(owner), false, true),
            (RunnerScope::Owner(owner), true, true),
            (RunnerScope::Owner(other_owner), true, false),
            (RunnerScope::Repository(repo), false, true),
            (RunnerScope::Repository(other_repo), true, false),
        ];
        for (scope, instance_allowed, expected) in cases {
            assert_eq!(runner_sees(scope, repo, owner, instance_allowed), expected, "{scope:?} {instance_allowed}");
        }
        // A repository id that happens to equal an account id grants nothing.
        assert!(!runner_sees(RunnerScope::Repository(RepoId(10)), repo, owner, true));
        assert!(!runner_sees(RunnerScope::Owner(AccountId(1)), repo, owner, true));
    }

    #[test]
    fn pull_request_trust_table() {
        use AuthorStanding::{Suspended, Unknown, Vouched};
        let known = |access, suspended, is_author| TriggeringActor::Known { access, suspended, is_author };
        let trusted = RunTrust::Trusted;
        let waits = RunTrust::Untrusted { awaits_approval: true };
        let runs = RunTrust::Untrusted { awaits_approval: false };
        let accesses = [RepoAccess::None, RepoAccess::Read, RepoAccess::Write, RepoAccess::Admin];
        for author in [Unknown, Vouched, Suspended] {
            assert_eq!(classify_pull_request(TriggeringActor::Unknown, author), waits, "unknown pusher, {author:?}");
            for access in accesses {
                for is_author in [false, true] {
                    // A suspended actor is never trusted and never skips the wait.
                    assert_eq!(classify_pull_request(known(access, true, is_author), author), waits);
                    let expected = if access.can_write() {
                        trusted
                    } else if is_author && author == Vouched {
                        runs
                    } else {
                        waits
                    };
                    assert_eq!(
                        classify_pull_request(known(access, false, is_author), author),
                        expected,
                        "{access:?} {is_author} {author:?}"
                    );
                }
            }
        }
        // The cases §11.1 calls out.
        assert_eq!(classify_pull_request(known(RepoAccess::Read, false, true), Unknown), waits, "same-repository pull from a reader");
        assert_eq!(
            classify_pull_request(known(RepoAccess::Read, false, false), Vouched),
            waits,
            "someone else pushed a vouched author's head"
        );
        assert_eq!(classify_pull_request(known(RepoAccess::Write, false, false), Unknown), trusted, "a writer pushed the head");
        assert_eq!(classify_pull_request(known(RepoAccess::Read, false, true), Suspended), waits);
    }

    #[test]
    fn permission_levels() {
        for level in [PermissionLevel::None, PermissionLevel::Read, PermissionLevel::Write] {
            assert_eq!(PermissionLevel::parse(level.as_str()), Some(level));
        }
        assert!(PermissionLevel::None < PermissionLevel::Read && PermissionLevel::Read < PermissionLevel::Write);
        assert_eq!(PermissionLevel::parse("admin"), None);
        for default in [DefaultTokenPermission::Read, DefaultTokenPermission::Write] {
            assert_eq!(DefaultTokenPermission::parse(default.as_str()), Some(default));
        }
        assert_eq!(DefaultTokenPermission::default(), DefaultTokenPermission::Read);
        assert_eq!(DefaultTokenPermission::parse("none"), None);
    }

    #[test]
    fn token_permissions_only_ever_lower() {
        use PermissionLevel::{None as N, Read as R, Write as W};
        let p = |contents, issues, pull_requests| TokenPermissions { contents, issues, pull_requests };
        let all = [N, R, W];
        let mut declared = vec![None];
        for contents in all {
            for issues in all {
                for pull_requests in all {
                    declared.push(Some(p(contents, issues, pull_requests)));
                }
            }
        }
        let trusts = [RunTrust::Trusted, RunTrust::Untrusted { awaits_approval: true }, RunTrust::Untrusted { awaits_approval: false }];
        for default in [DefaultTokenPermission::Read, DefaultTokenPermission::Write] {
            for &workflow in &declared {
                for &job in &declared {
                    for trust in trusts {
                        let granted = token_permissions(default, workflow, job, trust);
                        let ceiling = if trust.is_trusted() { default.permissions() } else { TokenPermissions::uniform(R) };
                        assert_eq!(granted.at_most(ceiling), granted, "never above the default or read for untrusted runs");
                        let effective = job.or(workflow).unwrap_or(TokenPermissions::uniform(W));
                        assert_eq!(granted, effective.at_most(ceiling), "{default:?} {workflow:?} {job:?} {trust:?}");
                    }
                }
            }
        }
        // Spot checks of the common shapes.
        let trusted = RunTrust::Trusted;
        assert_eq!(token_permissions(DefaultTokenPermission::Read, None, None, trusted), TokenPermissions::uniform(R));
        assert_eq!(token_permissions(DefaultTokenPermission::Write, None, None, trusted), TokenPermissions::uniform(W));
        assert_eq!(
            token_permissions(DefaultTokenPermission::Read, Some(TokenPermissions::uniform(W)), None, trusted),
            TokenPermissions::uniform(R),
            "write-all cannot raise a read default"
        );
        assert_eq!(
            token_permissions(DefaultTokenPermission::Write, Some(p(R, W, N)), Some(TokenPermissions::uniform(N)), trusted),
            TokenPermissions::uniform(N),
            "the job's declaration replaces the workflow's"
        );
        assert_eq!(token_permissions(DefaultTokenPermission::Write, Some(p(W, N, R)), None, trusted), p(W, N, R));
        assert_eq!(
            token_permissions(DefaultTokenPermission::Write, None, None, RunTrust::Untrusted { awaits_approval: false }),
            TokenPermissions::uniform(R)
        );
    }
}
