//! The typed model of a workflow file: every key of the subset in §12.2 of
//! `docs/design/actions.md`, at every level.
//!
//! A [`Workflow`] only exists once [`crate::parse`] has validated it: no key is unknown,
//! job ids are well formed, `needs` name existing jobs and form no cycle, filter patterns
//! compile, every `${{ }}` parses and uses only the contexts its field is evaluated with,
//! each step has exactly one of `run` and `uses`, and the file is within its
//! [`crate::Limits`].
//!
//! Values that hold expressions are kept as written (template strings); planning
//! evaluates the ones the server decides (§12.4) and the runner evaluates the rest.

use std::collections::BTreeMap;

pub use crate::spec::{ActionSource, ActionStep, Builtin, BuiltinKind, Env, RegistryCredentials, RunDefaults, StepAction};
use crate::{cron::Cron, error::Location, pattern::Patterns};

/// A parsed workflow file.
#[derive(Debug, Clone)]
pub struct Workflow {
    /// `name`; runs show the file path when it is absent.
    pub name: Option<String>,
    /// `run-name`, as written (it may contain expressions).
    pub run_name: Option<String>,
    pub on: On,
    /// Top-level `env`, as templates.
    pub env: Env,
    /// Top-level `defaults.run`.
    pub defaults: RunDefaults,
    /// Top-level `permissions`.
    pub permissions: Option<Permissions>,
    /// Workflow-level `concurrency`.
    pub concurrency: Option<Concurrency>,
    /// The jobs in file order.
    pub jobs: Vec<Job>,
}

impl Workflow {
    pub fn job(&self, id: &str) -> Option<&Job> {
        self.jobs.iter().find(|job| job.id == id)
    }
}

/// The events a workflow runs on (`on:`).
#[derive(Debug, Clone, Default)]
pub struct On {
    pub push: Option<PushFilters>,
    pub pull_request: Option<PullRequestFilters>,
    pub workflow_dispatch: Option<Dispatch>,
    /// `schedule` entries, in file order.
    pub schedule: Vec<Schedule>,
    // Actions: reusable workflows.
    /// `workflow_call`: other workflows may call this one.
    pub workflow_call: Option<WorkflowCall>,
    // Actions: workflow_run and repository_dispatch.
    pub workflow_run: Option<WorkflowRunFilters>,
    pub repository_dispatch: Option<RepositoryDispatchFilters>,
    /// Other events, by name. rbit starts no runs for them.
    pub other: Vec<String>,
}

// Actions: reusable workflows.
/// `on.workflow_call`: what a calling job passes and gets back.
#[derive(Debug, Clone, Default)]
pub struct WorkflowCall {
    pub inputs: BTreeMap<String, CallInput>,
    /// `outputs.<name>.value`, as templates over the `jobs` context.
    pub outputs: BTreeMap<String, String>,
    /// Each declared secret, and whether it is required.
    pub secrets: BTreeMap<String, bool>,
}

/// One `on.workflow_call.inputs` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct CallInput {
    pub kind: CallInputKind,
    pub required: bool,
    /// `default`, as a value of `kind`.
    pub default: Option<serde_json::Value>,
}

/// The type of a `workflow_call` input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallInputKind {
    Boolean,
    Number,
    String,
}

impl CallInputKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Number => "number",
            Self::String => "string",
        }
    }
}

/// A job that calls a reusable workflow (`jobs.<id>.uses`).
#[derive(Debug, Clone)]
pub struct JobCall {
    /// `uses:` as written.
    pub uses: String,
    pub target: CallTarget,
    /// `with:`, as templates.
    pub with: BTreeMap<String, String>,
    pub secrets: CallSecrets,
}

/// The workflow a job calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallTarget {
    /// `./path/to/workflow.yml`, in the repository and commit of the calling workflow.
    Local { path: String },
    /// `owner/repo/path/to/workflow.yml@ref`, a repository of this instance.
    Remote { owner: String, repo: String, path: String, reference: String },
}

/// `jobs.<id>.secrets`: `inherit`, or each secret of the called workflow by name with a
/// template over the caller's `secrets`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallSecrets {
    Inherit,
    Map(BTreeMap<String, String>),
}

impl Default for CallSecrets {
    fn default() -> Self {
        Self::Map(BTreeMap::new())
    }
}

// Actions: workflow_run and repository_dispatch.
/// `on.workflow_run`: runs of these workflows (by `name:`) of the same repository start
/// this one.
#[derive(Debug, Clone, Default)]
pub struct WorkflowRunFilters {
    pub workflows: Vec<String>,
    /// `None` means all three.
    pub types: Option<Vec<WorkflowRunType>>,
    /// Matched against the triggering run's branch.
    pub branches: Filter,
}

/// What happened to the triggering run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowRunType {
    Requested,
    InProgress,
    Completed,
}

impl WorkflowRunType {
    pub const ALL: [Self; 3] = [Self::Requested, Self::InProgress, Self::Completed];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }
}

/// `on.repository_dispatch`: the `event_type`s that start it; `None` means any.
#[derive(Debug, Clone, Default)]
pub struct RepositoryDispatchFilters {
    pub types: Option<Vec<String>>,
}

/// One of the three filters of an event: absent, an inclusion list (`branches`) or an
/// ignore list (`branches-ignore`). GitHub refuses both at once, and so does rbit.
#[derive(Debug, Clone, Default)]
pub enum Filter {
    #[default]
    Absent,
    Include(Patterns),
    Ignore(Patterns),
}

impl Filter {
    pub fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }

    /// Whether the inputs pass the filter (act's `!Skip` and `!Filter`); an absent filter
    /// passes everything.
    pub fn passes<I, S>(&self, inputs: I) -> bool
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        match self {
            Self::Absent => true,
            Self::Include(patterns) => !patterns.skip(inputs),
            Self::Ignore(patterns) => !patterns.filter(inputs),
        }
    }
}

/// `on.push`.
#[derive(Debug, Clone, Default)]
pub struct PushFilters {
    pub branches: Filter,
    pub tags: Filter,
    pub paths: Filter,
}

/// `on.pull_request`.
#[derive(Debug, Clone, Default)]
pub struct PullRequestFilters {
    /// `types`; `None` means GitHub's default (`opened`, `reopened`, `synchronize`).
    pub types: Option<Vec<ActivityType>>,
    /// Matched against the base branch.
    pub branches: Filter,
    pub paths: Filter,
}

/// A pull request activity type. rbit emits the named ones; [`ActivityType::Other`] keeps
/// the GitHub types it never emits (`labeled`, `assigned`, …), which therefore never match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityType {
    Opened,
    Reopened,
    Synchronize,
    Closed,
    Edited,
    ReadyForReview,
    Other(String),
}

impl ActivityType {
    /// The types a `pull_request` trigger without `types` runs on.
    pub const DEFAULT: [Self; 3] = [Self::Opened, Self::Reopened, Self::Synchronize];

    /// Every type GitHub documents for `pull_request`, so a typo is an error.
    const GITHUB_OTHERS: [&'static str; 14] = [
        "assigned",
        "unassigned",
        "labeled",
        "unlabeled",
        "converted_to_draft",
        "locked",
        "unlocked",
        "review_requested",
        "review_request_removed",
        "auto_merge_enabled",
        "auto_merge_disabled",
        "milestoned",
        "demilestoned",
        "enqueued",
    ];

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "opened" => Self::Opened,
            "reopened" => Self::Reopened,
            "synchronize" => Self::Synchronize,
            "closed" => Self::Closed,
            "edited" => Self::Edited,
            "ready_for_review" => Self::ReadyForReview,
            other if Self::GITHUB_OTHERS.contains(&other) || other == "dequeued" => Self::Other(other.to_owned()),
            _ => return None,
        })
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Opened => "opened",
            Self::Reopened => "reopened",
            Self::Synchronize => "synchronize",
            Self::Closed => "closed",
            Self::Edited => "edited",
            Self::ReadyForReview => "ready_for_review",
            Self::Other(other) => other,
        }
    }
}

/// `on.workflow_dispatch`.
#[derive(Debug, Clone, Default)]
pub struct Dispatch {
    pub inputs: Vec<DispatchInput>,
}

/// One `workflow_dispatch` input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchInput {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
    /// The default as text (`true`, `3`, an option of a choice).
    pub default: Option<String>,
    pub kind: InputKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    String,
    Boolean,
    Number,
    /// One of these options.
    Choice(Vec<String>),
    /// GitHub's deployment environment picker; rbit has no environments and treats it as a string.
    Environment,
}

/// One `schedule` entry: a valid expression at least [`crate::cron::MIN_INTERVAL_MINUTES`]
/// apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub cron: Cron,
    pub at: Location,
}

/// One coarse permission of the workflow token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Permission {
    #[default]
    None,
    Read,
    Write,
}

/// `permissions:`, coarsely: `contents`, `issues` and `pull-requests`. Every other
/// permission is accepted and ignored. A mapping leaves unnamed permissions at `none`, as
/// on GitHub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Permissions {
    pub contents: Permission,
    pub issues: Permission,
    pub pull_requests: Permission,
}

impl Permissions {
    pub const fn uniform(level: Permission) -> Self {
        Self { contents: level, issues: level, pull_requests: level }
    }
}

/// Workflow-level or job-level `concurrency`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Concurrency {
    /// The group, as written (it may contain expressions).
    pub group: String,
    /// `cancel-in-progress`: `true`, `false` or one expression, as text.
    pub cancel_in_progress: String,
    pub at: Location,
}

/// One job of a workflow.
#[derive(Debug, Clone)]
pub struct Job {
    /// The key under `jobs`: `[A-Za-z_][A-Za-z0-9_-]*`.
    pub id: String,
    /// `name`, as written (it may contain expressions).
    pub name: Option<String>,
    /// The jobs this one waits for; each exists in the workflow, none twice.
    pub needs: Vec<String>,
    /// `if`, as written; `None` means `success()`.
    pub condition: Option<String>,
    /// `runs-on` labels, as written (they may contain expressions).
    pub runs_on: Vec<String>,
    /// Job-level `permissions`; they replace the workflow's.
    pub permissions: Option<Permissions>,
    /// `env`, as templates.
    pub env: Env,
    /// `defaults.run`, as written; the workflow's apply where this sets nothing.
    pub defaults: RunDefaults,
    /// `outputs`, as templates the runner evaluates after the last step.
    pub outputs: BTreeMap<String, String>,
    /// `timeout-minutes` as written (a number or an expression); `None` means
    /// [`crate::spec::DEFAULT_TIMEOUT_MINUTES`].
    pub timeout_minutes: Option<String>,
    /// `continue-on-error` as written (`true`, `false` or an expression).
    pub continue_on_error: Option<String>,
    /// Job-level `concurrency`.
    pub concurrency: Option<Concurrency>,
    pub strategy: Strategy,
    pub container: Option<Container>,
    /// `services`, in file order.
    pub services: Vec<Service>,
    /// At least one, at most [`crate::spec::MAX_STEPS`].
    pub steps: Vec<Step>,
    // Actions: environments.
    /// `environment`: the deployment environment the job runs in.
    pub environment: Option<EnvironmentRef>,
    // Actions: reusable workflows.
    /// `uses`: the job calls a reusable workflow; it then has no `runs-on` and no steps.
    pub call: Option<JobCall>,
    pub at: Location,
}

// Actions: environments.
/// A job's `environment`: `name` (or `{name, url}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentRef {
    /// A template the server evaluates when it plans the job (`github`, `inputs`, `vars`,
    /// `matrix`, `strategy`).
    pub name: String,
    /// A template the runner evaluates after the job's steps (it may use `steps`).
    pub url: Option<String>,
}

/// A job's `strategy`.
#[derive(Debug, Clone, Default)]
pub struct Strategy {
    pub matrix: Option<Matrix>,
    /// `fail-fast` as written (`true`, `false` or an expression); `None` means `true`.
    pub fail_fast: Option<String>,
    /// `max-parallel` as written (a number or an expression); `None` means no limit.
    pub max_parallel: Option<String>,
}

/// The job `container`: `container: image` or the mapping form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Container {
    /// The image, as written (it may contain expressions).
    pub image: String,
    pub env: Env,
    /// `credentials`, as templates.
    pub credentials: Option<RegistryCredentials>,
    /// `options`, as written (it may contain expressions; see [`crate::options`]).
    pub options: Option<String>,
    /// `volumes`, as written: named volumes only.
    pub volumes: Vec<String>,
    pub at: Location,
}

/// One `services.<id>` container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    /// The key under `services`: also its host name on the job's network.
    pub id: String,
    /// The image, as written (it may contain expressions).
    pub image: String,
    pub env: Env,
    pub credentials: Option<RegistryCredentials>,
    /// `options`, as written.
    pub options: Option<String>,
    /// `volumes`, as written.
    pub volumes: Vec<String>,
    pub at: Location,
}

/// One step of a job. Everything but `id` may contain expressions for the runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// `id`, unique within the job: `[A-Za-z_][A-Za-z0-9_-]*`.
    pub id: Option<String>,
    pub name: Option<String>,
    /// `if`, as written; `None` means `success()`.
    pub condition: Option<String>,
    /// `run`, or `uses` resolved to a built-in ([`crate::builtins`]).
    pub action: StepAction,
    /// `shell` (`run` steps only): one of [`Shell`]'s names, or an expression.
    pub shell: Option<String>,
    /// `working-directory` (`run` steps only).
    pub working_directory: Option<String>,
    pub env: Env,
    /// `continue-on-error` as written (`true`, `false` or an expression).
    pub continue_on_error: Option<String>,
    /// `timeout-minutes` as written (a number or an expression).
    pub timeout_minutes: Option<String>,
    pub at: Location,
}

/// The shells a `run` step may name (§12.2). A custom command template with `{0}` for the
/// script's path (`perl {0}`) is a shell too; the runner runs it as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shell {
    Bash,
    Sh,
    Pwsh,
    Powershell,
    Cmd,
    Python,
}

impl Shell {
    pub const ALL: [Self; 6] = [Self::Bash, Self::Sh, Self::Pwsh, Self::Powershell, Self::Cmd, Self::Python];

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|shell| shell.name() == name)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Sh => "sh",
            Self::Pwsh => "pwsh",
            Self::Powershell => "powershell",
            Self::Cmd => "cmd",
            Self::Python => "python",
        }
    }

    /// The command line that runs a script, `{0}` standing for the script's path. These
    /// are GitHub's, so scripts behave as they do there.
    pub const fn command(self) -> &'static str {
        match self {
            Self::Bash => "bash --noprofile --norc -eo pipefail {0}",
            Self::Sh => "sh -e {0}",
            Self::Pwsh => "pwsh -command \". '{0}'\"",
            Self::Powershell => "powershell -command \". '{0}'\"",
            Self::Cmd => "cmd /D /E:ON /V:OFF /S /C \"CALL \"{0}\"\"",
            Self::Python => "python {0}",
        }
    }

    /// The extension the script file needs for the shell to run it.
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Bash | Self::Sh => ".sh",
            Self::Pwsh | Self::Powershell => ".ps1",
            Self::Cmd => ".cmd",
            Self::Python => ".py",
        }
    }
}

/// `strategy.matrix`, as data.
///
/// Strings may hold `${{ }}` over `github`, `inputs` and `vars`: the whole matrix
/// (`${{ fromJSON(inputs.matrix) }}`), a dimension's list, `include`, `exclude` or single
/// values. Planning evaluates them, then checks and expands the result
/// ([`crate::plan::Axes`]). A matrix without expressions was checked when it was parsed.
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix {
    pub value: serde_json::Value,
    /// Whether any string in [`Self::value`] holds `${{ }}`.
    pub has_expressions: bool,
    pub at: Location,
}
